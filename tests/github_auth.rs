mod common;

use std::collections::HashMap;

use axum::http::{Response, StatusCode};
use common::{TestApp, assert_redirect, body_string, capture_error_logs, cookie_from_response};
use interne::config::SignupMode;
use interne::github::{GitHubError, GitHubProfile};

fn query_parameters(url: &url::Url) -> HashMap<String, String> {
    url.query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

fn callback_uri(code: &str, state: &str) -> String {
    let mut url = url::Url::parse("https://interne.test/auth/github/callback").unwrap();
    url.query_pairs_mut()
        .append_pair("code", code)
        .append_pair("state", state);
    format!("{}?{}", url.path(), url.query().unwrap())
}

fn register_profile(app: &TestApp, code: &str, user_id: &str, login: &str, name: Option<&str>) {
    app.github.profile_for_code(
        code,
        GitHubProfile {
            user_id: user_id.into(),
            login: login.into(),
            name: name.map(str::to_owned),
        },
    );
}

async fn callback_body(response: Response<axum::body::Body>) -> String {
    assert_eq!(response.status(), StatusCode::OK);
    body_string(response).await
}

async fn prepare_legacy_pending_connection(
    app: &TestApp,
    invite_code: &str,
    code: &str,
    github_user_id: &str,
    github_login: &str,
) -> String {
    let cookie = app.legacy_login(invite_code).await;
    let authorization_url = app.begin_github_link(&cookie).await;
    let state = query_parameters(&authorization_url)["state"].clone();
    register_profile(app, code, github_user_id, github_login, None);

    let response = app.get(&callback_uri(code, &state), Some(&cookie)).await;
    assert_redirect(&response, "/auth/github/confirm");

    cookie
}

#[tokio::test]
async fn legacy_user_confirms_github_and_keeps_the_same_user_id() {
    let app = TestApp::new().await;
    let (legacy_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie =
        prepare_legacy_pending_connection(&app, &invite_code, "legacy-code", "901", "legacy-user")
            .await;

    let response = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    assert_redirect(&response, "/");
    let authenticated_cookie = cookie_from_response(
        response
            .headers()
            .get("set-cookie")
            .expect("Confirmation should establish a fresh full session"),
    );

    let home = app.get("/", Some(&authenticated_cookie)).await;
    assert_eq!(home.status(), StatusCode::OK);
    let linked_user_id: String =
        sqlx::query_scalar("SELECT id FROM users WHERE github_user_id = '901'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linked_user_id, legacy_user_id);
    let auth_version: i64 = sqlx::query_scalar("SELECT auth_version FROM users WHERE id = ?")
        .bind(legacy_user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(auth_version, 2);
}

#[tokio::test]
async fn link_confirmation_nulls_legacy_invite_code() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie =
        prepare_legacy_pending_connection(&app, &invite_code, "null-code", "902", "linked").await;

    let response = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    assert_redirect(&response, "/");

    let stored_invite_code: Option<String> =
        sqlx::query_scalar("SELECT invite_code FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(stored_invite_code, None);
}

#[tokio::test]
async fn link_confirmation_rejects_github_id_owned_by_another_user() {
    let app = TestApp::new().await;
    let owner_id = app
        .create_github_user("Owner", "903", "existing-owner")
        .await;
    let (legacy_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = prepare_legacy_pending_connection(
        &app,
        &invite_code,
        "duplicate-code",
        "903",
        "existing-owner",
    )
    .await;

    let (response, logs) =
        capture_error_logs(app.post_form("/auth/github/confirm", "", Some(&cookie))).await;
    let body = callback_body(response).await;
    assert!(body.contains("already connected"));
    assert!(
        logs.is_empty(),
        "duplicate identity is not an operational failure"
    );
    assert!(!body.contains(&owner_id));
    assert!(!body.contains(&legacy_user_id));

    let legacy_state: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT github_user_id, invite_code FROM users WHERE id = ?")
            .bind(legacy_user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(legacy_state.0, None);
    assert_eq!(legacy_state.1.as_deref(), Some(invite_code.as_str()));
}

#[tokio::test]
async fn legacy_confirmation_database_failure_logs_only_safe_operation_context() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_legacy_user("Legacy Secret").await;
    let oauth_code = "secret-legacy-oauth-code";
    let github_user_id = "secret-legacy-github-id";
    let github_login = "secret-legacy-login";
    let cookie = prepare_legacy_pending_connection(
        &app,
        &invite_code,
        oauth_code,
        github_user_id,
        github_login,
    )
    .await;
    let hostile_error = "hostile-legacy-trigger-secret";
    sqlx::query(
        "CREATE TRIGGER reject_legacy_github_link BEFORE UPDATE OF github_user_id ON users \
         BEGIN SELECT RAISE(ABORT, 'hostile-legacy-trigger-secret'); END",
    )
    .execute(&app.db)
    .await
    .unwrap();

    let (response, logs) =
        capture_error_logs(app.post_form("/auth/github/confirm", "", Some(&cookie))).await;
    let status = response.status();
    let body = body_string(response).await;

    assert!(!logs.contains(hostile_error));
    for secret in [
        oauth_code,
        github_user_id,
        github_login,
        invite_code.as_str(),
        user_id.as_str(),
        "UPDATE users",
        "reject_legacy_github_link",
    ] {
        assert!(!logs.contains(secret), "secret leaked to logs: {secret}");
        assert!(
            !body.contains(secret),
            "secret leaked to response: {secret}"
        );
    }
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("couldn’t connect this GitHub account"));
    assert!(logs.contains("legacy GitHub connection database operation failed"));

    let state: (Option<String>, Option<String>, String, i64) = sqlx::query_as(
        "SELECT github_user_id, github_login, invite_code, auth_version FROM users WHERE id = ?",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(state, (None, None, invite_code, 1));
}

#[tokio::test]
async fn link_callback_requires_a_live_migration_session() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = app.legacy_login(&invite_code).await;
    let authorization_url = app.begin_github_link(&cookie).await;
    let state = query_parameters(&authorization_url)["state"].clone();
    app.expire_migration_session(&cookie).await;
    register_profile(&app, "orphan-code", "904", "orphan", None);

    let response = app
        .get(&callback_uri("orphan-code", &state), Some(&cookie))
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t connect this GitHub account"));
    let github_user_id: Option<String> =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE invite_code = ?")
            .bind(invite_code)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(github_user_id, None);
}

#[tokio::test]
async fn link_callback_rejects_a_migration_session_for_a_different_user() {
    let app = TestApp::new().await;
    let (first_user_id, first_invite_code) = app.create_legacy_user("First User").await;
    let (second_user_id, _second_invite_code) = app.create_legacy_user("Second User").await;
    let cookie = app.legacy_login(&first_invite_code).await;
    let authorization_url = app.begin_github_link(&cookie).await;
    let state = query_parameters(&authorization_url)["state"].clone();
    app.retarget_migration_session(&cookie, &second_user_id)
        .await;
    register_profile(&app, "retargeted-code", "912", "retargeted", None);

    let response = app
        .get(&callback_uri("retargeted-code", &state), Some(&cookie))
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t connect this GitHub account"));
    let linked_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM users WHERE id IN (?, ?) AND github_user_id IS NOT NULL",
    )
    .bind(first_user_id)
    .bind(second_user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(linked_count, 0);
}

#[tokio::test]
async fn expired_migration_session_cannot_start_linking() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = app.legacy_login(&invite_code).await;
    let page = body_string(app.get("/auth/connect", Some(&cookie)).await).await;
    assert!(page.contains("replace your legacy invite code"));
    app.expire_migration_session(&cookie).await;

    let page = app.get("/auth/connect", Some(&cookie)).await;
    assert_redirect(&page, "/login");
    let start = app
        .post_form("/auth/github/connect", "", Some(&cookie))
        .await;
    assert_redirect(&start, "/login");
}

#[tokio::test]
async fn confirmation_rechecks_the_current_legacy_user_state() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = prepare_legacy_pending_connection(
        &app,
        &invite_code,
        "stale-user-code",
        "909",
        "stale-user",
    )
    .await;
    sqlx::query("UPDATE users SET invite_code = NULL WHERE id = ?")
        .bind(&user_id)
        .execute(&app.db)
        .await
        .unwrap();

    let response = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t connect this GitHub account"));
    let github_user_id: Option<String> =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(github_user_id, None);
}

#[tokio::test]
async fn completed_confirmation_cannot_be_replayed() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = prepare_legacy_pending_connection(
        &app,
        &invite_code,
        "confirm-replay-code",
        "910",
        "confirm-replay",
    )
    .await;

    let first = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    assert_redirect(&first, "/");
    let replay = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    let body = callback_body(replay).await;
    assert!(body.contains("couldn’t connect this GitHub account"));
}

#[tokio::test]
async fn normal_github_login_clears_legacy_migration_state() {
    let app = TestApp::new().await;
    app.create_github_user("Linked User", "911", "linked-user")
        .await;
    let (_legacy_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let migration_cookie = app.legacy_login(&invite_code).await;
    let (cookie, authorization_url) = app
        .begin_github_login_with_cookie(Some(&migration_cookie))
        .await;
    let state = query_parameters(&authorization_url)["state"].clone();
    register_profile(&app, "normal-code", "911", "linked-user", None);

    let response = app
        .get(&callback_uri("normal-code", &state), Some(&cookie))
        .await;
    assert_redirect(&response, "/");
    let full_cookie = cookie_from_response(
        response
            .headers()
            .get("set-cookie")
            .expect("Normal GitHub login should replace migration state"),
    );
    assert_eq!(
        app.get("/", Some(&full_cookie)).await.status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn pending_confirmation_expires_after_ten_minutes() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = prepare_legacy_pending_connection(
        &app,
        &invite_code,
        "expired-pending-code",
        "905",
        "too-late",
    )
    .await;
    app.expire_pending_connection(&cookie).await;

    let page = app.get("/auth/github/confirm", Some(&cookie)).await;
    let body = callback_body(page).await;
    assert!(body.contains("couldn’t connect this GitHub account"));
    let confirm = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    let body = callback_body(confirm).await;
    assert!(body.contains("couldn’t connect this GitHub account"));

    let github_user_id: Option<String> =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE invite_code = ?")
            .bind(invite_code)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(github_user_id, None);
}

#[tokio::test]
async fn link_confirmation_consumes_active_connection_tokens() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let now = chrono::Utc::now();
    sqlx::query(
        "INSERT INTO auth_connection_tokens \
         (id, user_id, token_hash, purpose, created_at, expires_at, consumed_at) \
         VALUES ('active-token', ?, 'hash', 'recovery', ?, ?, NULL)",
    )
    .bind(&user_id)
    .bind(now.to_rfc3339())
    .bind((now + chrono::Duration::hours(1)).to_rfc3339())
    .execute(&app.db)
    .await
    .unwrap();
    let cookie =
        prepare_legacy_pending_connection(&app, &invite_code, "token-code", "906", "token-user")
            .await;

    let response = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    assert_redirect(&response, "/");
    let consumed_at: Option<String> = sqlx::query_scalar(
        "SELECT consumed_at FROM auth_connection_tokens WHERE id = 'active-token'",
    )
    .fetch_one(&app.db)
    .await
    .unwrap();
    let consumed_at = consumed_at.expect("active token is consumed");
    assert_eq!(consumed_at.len(), 30);
    assert!(consumed_at.ends_with('Z'));
}

#[tokio::test]
async fn github_confirmation_escapes_login_and_accepts_no_identity_fields() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = prepare_legacy_pending_connection(
        &app,
        &invite_code,
        "escape-code",
        "907",
        "<script>alert(1)</script>",
    )
    .await;

    let body = body_string(app.get("/auth/github/confirm", Some(&cookie)).await).await;
    assert!(body.contains("&#60;script&#62;alert(1)&#60;/script&#62;"));
    assert!(!body.contains("<script>alert(1)</script>"));
    assert!(body.contains("action=\"/auth/github/confirm\""));
    assert!(!body.contains("name=\"github"));

    let response = app
        .post_form(
            "/auth/github/confirm",
            "github_user_id=attacker&github_login=attacker",
            Some(&cookie),
        )
        .await;
    assert_redirect(&response, "/");
    let linked_id: String =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE github_login = ?")
            .bind("<script>alert(1)</script>")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linked_id, "907");
}

#[tokio::test]
async fn denied_link_callback_consumes_attempt() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Legacy User").await;
    let cookie = app.legacy_login(&invite_code).await;
    let authorization_url = app.begin_github_link(&cookie).await;
    let state = query_parameters(&authorization_url)["state"].clone();

    let denied = app
        .get(
            &format!("/auth/github/callback?error=access_denied&state={state}"),
            Some(&cookie),
        )
        .await;
    let body = callback_body(denied).await;
    assert!(body.contains("couldn’t connect this GitHub account"));

    register_profile(&app, "link-replay", "908", "replay", None);
    let replay = app
        .get(&callback_uri("link-replay", &state), Some(&cookie))
        .await;
    let body = callback_body(replay).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
}

#[tokio::test]
async fn linked_github_identity_logs_into_existing_user() {
    let app = TestApp::new().await;
    let user_id = app.create_github_user("Axel", "100", "axelav").await;
    let cookie = app.github_login("100", "axelav", Some("Axel")).await;

    let response = app.get("/", Some(&cookie)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let authenticated_id: String =
        sqlx::query_scalar("SELECT id FROM users WHERE github_user_id = '100'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(authenticated_id, user_id);
}

#[tokio::test]
async fn unknown_identity_is_rejected_in_closed_mode() {
    let app = TestApp::new().await;
    let response = app.github_callback_response("200", "visitor", None).await;
    let body = callback_body(response).await;
    assert!(body.contains("Access isn’t open yet"));
    assert!(body.contains("webmaster@honkytonk.in"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn unknown_identity_creates_account_in_public_mode() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let response = app.github_callback_response("200", "visitor", None).await;
    assert_redirect(&response, "/");
    let (name, email, invite_code): (String, Option<String>, Option<String>) =
        sqlx::query_as("SELECT name, email, invite_code FROM users WHERE github_user_id = '200'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(name, "visitor");
    assert_eq!(email, None);
    assert_eq!(invite_code, None);
}

#[tokio::test]
async fn oauth_callback_rejects_mismatched_state() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let (cookie, _) = app.begin_github_login().await;
    register_profile(&app, "valid-code", "300", "attacker", None);

    let response = app
        .get(
            &callback_uri("valid-code", "mismatched-state"),
            Some(&cookie),
        )
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn oauth_callback_rejects_replayed_state() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let (cookie, authorization_url) = app.begin_github_login().await;
    let state = query_parameters(&authorization_url)["state"].clone();
    register_profile(&app, "valid-code", "400", "replay", None);
    let uri = callback_uri("valid-code", &state);

    let first = app.get(&uri, Some(&cookie)).await;
    assert_redirect(&first, "/");
    let replay = app.get(&uri, Some(&cookie)).await;
    let body = callback_body(replay).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE github_user_id = '400'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn oauth_callback_rejects_expired_attempt() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let (cookie, authorization_url) = app.begin_github_login().await;
    let state = query_parameters(&authorization_url)["state"].clone();
    app.expire_oauth_attempt(&cookie).await;
    register_profile(&app, "valid-code", "500", "late", None);

    let response = app
        .get(&callback_uri("valid-code", &state), Some(&cookie))
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn denied_oauth_callback_consumes_attempt() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let (cookie, authorization_url) = app.begin_github_login().await;
    let state = query_parameters(&authorization_url)["state"].clone();

    let denied = app
        .get(
            &format!(
                "/auth/github/callback?error=access_denied&error_description=private-details&state={state}"
            ),
            Some(&cookie),
        )
        .await;
    let body = callback_body(denied).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
    assert!(!body.contains("access_denied"));
    assert!(!body.contains("private-details"));

    register_profile(&app, "valid-code", "510", "denied-replay", None);
    let replay = app
        .get(&callback_uri("valid-code", &state), Some(&cookie))
        .await;
    let body = callback_body(replay).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn denied_callback_validates_state_and_expiry_then_consumes_the_attempt() {
    for case in ["missing", "mismatched", "expired"] {
        let app = TestApp::with_signup_mode(SignupMode::Public).await;
        let (cookie, authorization_url) = app.begin_github_login().await;
        let state = query_parameters(&authorization_url)["state"].clone();
        if case == "expired" {
            app.expire_oauth_attempt(&cookie).await;
        }
        let denied_uri = match case {
            "missing" => "/auth/github/callback?error=access_denied".to_string(),
            "mismatched" => {
                "/auth/github/callback?error=access_denied&state=attacker-state".to_string()
            }
            "expired" => format!(
                "/auth/github/callback?error=access_denied&error_description=private&state={state}"
            ),
            _ => unreachable!(),
        };

        let denied = app.get(&denied_uri, Some(&cookie)).await;
        let body = callback_body(denied).await;
        assert!(body.contains("couldn’t complete GitHub sign-in"));
        assert!(!body.contains("access_denied"));
        assert!(!body.contains("private"));

        register_profile(&app, "denial-replay-code", "511", "replay", None);
        let replay = app
            .get(&callback_uri("denial-replay-code", &state), Some(&cookie))
            .await;
        let body = callback_body(replay).await;
        assert!(body.contains("couldn’t complete GitHub sign-in"));
    }
}

#[tokio::test]
async fn oauth_callback_with_missing_fields_consumes_attempt() {
    for missing_state in [true, false] {
        let app = TestApp::with_signup_mode(SignupMode::Public).await;
        let (cookie, authorization_url) = app.begin_github_login().await;
        let state = query_parameters(&authorization_url)["state"].clone();
        let invalid_uri = if missing_state {
            "/auth/github/callback?code=valid-code".to_string()
        } else {
            format!("/auth/github/callback?state={state}")
        };

        let response = app.get(&invalid_uri, Some(&cookie)).await;
        let body = callback_body(response).await;
        assert!(body.contains("couldn’t complete GitHub sign-in"));

        register_profile(&app, "valid-code", "520", "missing-replay", None);
        let replay = app
            .get(&callback_uri("valid-code", &state), Some(&cookie))
            .await;
        let body = callback_body(replay).await;
        assert!(body.contains("couldn’t complete GitHub sign-in"));
    }
}

#[tokio::test]
async fn oauth_callback_with_malformed_encoding_consumes_attempt() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let (cookie, authorization_url) = app.begin_github_login().await;
    let state = query_parameters(&authorization_url)["state"].clone();

    let response = app
        .get(
            &format!("/auth/github/callback?code=%FF&state={state}"),
            Some(&cookie),
        )
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));

    register_profile(&app, "valid-code", "530", "malformed-replay", None);
    let replay = app
        .get(&callback_uri("valid-code", &state), Some(&cookie))
        .await;
    let body = callback_body(replay).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
}

#[tokio::test]
async fn oauth_callback_rejects_duplicate_state_or_code() {
    for duplicate_state in [true, false] {
        let app = TestApp::with_signup_mode(SignupMode::Public).await;
        let (cookie, authorization_url) = app.begin_github_login().await;
        let state = query_parameters(&authorization_url)["state"].clone();
        register_profile(&app, "valid-code", "540", "duplicate", None);
        let invalid_uri = if duplicate_state {
            format!("/auth/github/callback?code=valid-code&state={state}&state={state}")
        } else {
            format!("/auth/github/callback?code=valid-code&code=valid-code&state={state}")
        };

        let response = app.get(&invalid_uri, Some(&cookie)).await;
        let body = callback_body(response).await;
        assert!(body.contains("couldn’t complete GitHub sign-in"));

        let replay = app
            .get(&callback_uri("valid-code", &state), Some(&cookie))
            .await;
        let body = callback_body(replay).await;
        assert!(body.contains("couldn’t complete GitHub sign-in"));
    }
}

#[tokio::test]
async fn github_login_refreshes_display_username() {
    let app = TestApp::new().await;
    app.create_github_user("Axel", "600", "old-login").await;

    app.github_login("600", "new-login", Some("Axel")).await;

    let github_login: String =
        sqlx::query_scalar("SELECT github_login FROM users WHERE github_user_id = '600'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(github_login, "new-login");
}

#[tokio::test]
async fn provider_failure_shows_safe_error() {
    let app = TestApp::new().await;
    let (cookie, authorization_url) = app.begin_github_login().await;
    let state = query_parameters(&authorization_url)["state"].clone();
    app.github
        .error_for_code("sensitive-code", GitHubError::ProfileFetch);

    let response = app
        .get(&callback_uri("sensitive-code", &state), Some(&cookie))
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
    assert!(!body.contains("sensitive-code"));
    assert!(!body.contains("GitHub profile fetch failed"));
    assert!(!body.contains("ProfileFetch"));
}

#[tokio::test]
async fn provider_failure_logs_safe_stage_without_oauth_code() {
    let app = TestApp::new().await;
    let (cookie, authorization_url) = app.begin_github_login().await;
    let state = query_parameters(&authorization_url)["state"].clone();
    let hostile_code = "hostile-secret-oauth-code";
    app.github
        .error_for_code(hostile_code, GitHubError::ProfileFetch);

    let (response, logs) =
        capture_error_logs(app.get(&callback_uri(hostile_code, &state), Some(&cookie))).await;
    let body = callback_body(response).await;

    assert!(body.contains("couldn’t complete GitHub sign-in"));
    assert!(logs.contains("GitHub profile fetch failed"));
    assert!(logs.contains("OAuth callback exchange failed"));
    assert!(!logs.contains(hostile_code));
}

#[tokio::test]
async fn authorization_redirect_has_pkce_and_no_scope() {
    let app = TestApp::new().await;

    let (_cookie, authorization_url) = app.begin_github_login().await;

    let query = query_parameters(&authorization_url);
    assert_eq!(
        authorization_url.as_str().split('?').next().unwrap(),
        "https://github.test/authorize"
    );
    assert_eq!(
        query["redirect_uri"],
        "https://interne.test/auth/github/callback"
    );
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(query["state"].len(), 43);
    assert_eq!(query["code_challenge"].len(), 43);
    assert!(!query.contains_key("scope"));
}

#[tokio::test]
async fn starting_new_oauth_attempt_replaces_previous_attempt() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let (cookie, first_url) = app.begin_github_login().await;
    let (cookie, second_url) = app.begin_github_login_with_cookie(Some(&cookie)).await;
    let first_state = query_parameters(&first_url)["state"].clone();
    let second_state = query_parameters(&second_url)["state"].clone();
    assert_ne!(first_state, second_state);
    register_profile(&app, "valid-code", "700", "latest", None);

    let response = app
        .get(&callback_uri("valid-code", &first_state), Some(&cookie))
        .await;
    let body = callback_body(response).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
}

#[tokio::test]
async fn public_signup_ignores_blank_display_name() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;

    let response = app
        .github_callback_response("800", "fallback-login", Some("  \t "))
        .await;

    assert_redirect(&response, "/");
    let name: String = sqlx::query_scalar("SELECT name FROM users WHERE github_user_id = '800'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(name, "fallback-login");
}

#[tokio::test]
async fn login_page_prioritizes_github_and_hides_unavailable_legacy_form() {
    let app = TestApp::new().await;

    let body = body_string(app.get("/login", None).await).await;

    assert!(body.contains("Continue with GitHub"));
    assert!(!body.contains("name=\"invite_code\""));
}

#[tokio::test]
async fn login_page_keeps_legacy_form_for_unlinked_users() {
    let app = TestApp::new().await;
    app.create_legacy_user("Legacy User").await;

    let body = body_string(app.get("/login", None).await).await;

    assert!(body.contains("Continue with GitHub"));
    assert!(body.contains("name=\"invite_code\""));
}
