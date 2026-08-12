mod common;

use axum::http::{Response, StatusCode};
use chrono::{Duration, SecondsFormat, TimeZone, Utc};
use interne::connection_tokens::{
    ConnectionPurpose, ConnectionTokenError, connection_url, consume_and_link, issue_invitation,
    reset_auth, validate_token,
};
use interne::github::GitHubProfile;
use std::process::Command;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration as StdDuration;
use url::Url;

use common::{TestApp, assert_redirect, body_string, capture_error_logs, cookie_from_response};

fn query_parameter(url: &Url, name: &str) -> String {
    url.query_pairs()
        .find_map(|(key, value)| (key == name).then(|| value.into_owned()))
        .unwrap_or_else(|| panic!("URL should contain {name}"))
}

fn callback_uri(code: &str, state: &str) -> String {
    let mut url = Url::parse("https://interne.test/auth/github/callback").unwrap();
    url.query_pairs_mut()
        .append_pair("code", code)
        .append_pair("state", state);
    format!("{}?{}", url.path(), url.query().unwrap())
}

async fn begin_connection_oauth(
    app: &TestApp,
    plaintext_token: &str,
) -> (String, Url, Response<axum::body::Body>) {
    let entry = app
        .get(&format!("/recover?token={plaintext_token}"), None)
        .await;
    assert_redirect(&entry, "/auth/github/recover");
    assert_eq!(entry.headers()["referrer-policy"], "no-referrer");
    assert!(
        !entry.headers()["location"]
            .to_str()
            .unwrap()
            .contains(plaintext_token)
    );
    let cookie = cookie_from_response(
        entry
            .headers()
            .get("set-cookie")
            .expect("Recovery entry should establish a claim session"),
    );

    let clean = app.get("/auth/github/recover", Some(&cookie)).await;
    assert_eq!(clean.headers()["referrer-policy"], "no-referrer");
    let authorization_url = Url::parse(
        clean
            .headers()
            .get("location")
            .expect("Clean recovery path should redirect to GitHub")
            .to_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(authorization_url.host_str(), Some("github.test"));
    assert!(!authorization_url.as_str().contains(plaintext_token));

    (cookie, authorization_url, clean)
}

async fn prepare_token_confirmation(
    app: &TestApp,
    plaintext_token: &str,
    code: &str,
    github_user_id: &str,
    github_login: &str,
) -> String {
    let (cookie, authorization_url, _) = begin_connection_oauth(app, plaintext_token).await;
    let state = query_parameter(&authorization_url, "state");
    app.github.profile_for_code(
        code,
        GitHubProfile {
            user_id: github_user_id.to_owned(),
            login: github_login.to_owned(),
            name: None,
        },
    );

    let callback = app.get(&callback_uri(code, &state), Some(&cookie)).await;
    assert_redirect(&callback, "/auth/github/confirm");
    cookie
}

async fn confirm_token_connection(app: &TestApp, cookie: &str) -> String {
    let confirmation = app
        .post_form("/auth/github/confirm", "", Some(cookie))
        .await;
    assert_redirect(&confirmation, "/");
    cookie_from_response(
        confirmation
            .headers()
            .get("set-cookie")
            .expect("Confirmation should rotate to a full session"),
    )
}

#[tokio::test]
async fn recovery_url_connects_confirmed_github_identity_and_logs_in() {
    let app = TestApp::new().await;
    let (user_id, previous_github_id) = app.create_user("Axel").await;
    let old_cookie = app.login(&previous_github_id).await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();

    let cookie = prepare_token_confirmation(
        &app,
        &issued.plaintext_token,
        "recover-code",
        "900",
        "axelav",
    )
    .await;
    let confirmation_page = body_string(app.get("/auth/github/confirm", Some(&cookie)).await).await;
    assert!(confirmation_page.contains("Recover account"));
    let new_cookie = confirm_token_connection(&app, &cookie).await;

    assert_eq!(
        app.get("/", Some(&new_cookie)).await.status(),
        StatusCode::OK
    );
    assert_redirect(&app.get("/", Some(&old_cookie)).await, "/login");
    let linked: (String, String, i64) =
        sqlx::query_as("SELECT github_user_id, github_login, auth_version FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linked, ("900".into(), "axelav".into(), 3));
}

#[tokio::test]
async fn invitation_url_connects_the_precreated_user() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Invited", Utc::now())
        .await
        .unwrap();
    let cookie = prepare_token_confirmation(
        &app,
        &issued.plaintext_token,
        "invite-code",
        "901",
        "invited",
    )
    .await;

    let confirmation_page = body_string(app.get("/auth/github/confirm", Some(&cookie)).await).await;
    assert!(confirmation_page.contains("Accept invitation"));
    let full_cookie = confirm_token_connection(&app, &cookie).await;

    assert_eq!(
        app.get("/", Some(&full_cookie)).await.status(),
        StatusCode::OK
    );
    let linked_user_id: String =
        sqlx::query_scalar("SELECT id FROM users WHERE github_user_id = '901'")
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linked_user_id, issued.user_id);
}

#[tokio::test]
async fn recovery_token_is_not_consumed_before_confirmation() {
    let app = TestApp::new().await;
    let (user_id, _) = app.create_user("Recovering").await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();

    prepare_token_confirmation(
        &app,
        &issued.plaintext_token,
        "pending-code",
        "902",
        "pending",
    )
    .await;

    let consumed_at: Option<String> =
        sqlx::query_scalar("SELECT consumed_at FROM auth_connection_tokens WHERE user_id = ?")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(consumed_at, None);
}

#[tokio::test]
async fn successful_confirmation_consumes_token() {
    let app = TestApp::new().await;
    let (user_id, _) = app.create_user("Recovering").await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();
    let now = Utc::now();
    sqlx::query(
        "INSERT INTO auth_connection_tokens \
         (id, user_id, token_hash, purpose, created_at, expires_at, consumed_at) \
         VALUES ('other-active-token', ?, 'other-active-hash', 'recovery', ?, ?, NULL)",
    )
    .bind(&user_id)
    .bind(now.to_rfc3339_opts(SecondsFormat::Nanos, true))
    .bind((now + Duration::hours(1)).to_rfc3339_opts(SecondsFormat::Nanos, true))
    .execute(&app.db)
    .await
    .unwrap();
    let cookie = prepare_token_confirmation(
        &app,
        &issued.plaintext_token,
        "consume-code",
        "903",
        "consumed",
    )
    .await;

    confirm_token_connection(&app, &cookie).await;

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM auth_connection_tokens WHERE user_id = ? AND consumed_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(active_count, 0);
}

#[tokio::test]
async fn used_recovery_url_cannot_be_replayed() {
    let app = TestApp::new().await;
    let (user_id, _) = app.create_user("Recovering").await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();
    let cookie =
        prepare_token_confirmation(&app, &issued.plaintext_token, "once-code", "904", "once").await;
    confirm_token_connection(&app, &cookie).await;

    let replay = app
        .get(&format!("/recover?token={}", issued.plaintext_token), None)
        .await;
    let body = body_string(replay).await;
    assert!(body.contains("invalid or expired"));
    assert!(!body.contains(&issued.plaintext_token));
}

#[tokio::test]
async fn superseded_recovery_url_is_rejected() {
    let app = TestApp::new().await;
    let (user_id, _) = app.create_user("Recovering").await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();
    let entry = app
        .get(&format!("/recover?token={}", issued.plaintext_token), None)
        .await;
    assert_redirect(&entry, "/auth/github/recover");
    let cookie = cookie_from_response(entry.headers().get("set-cookie").unwrap());
    reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();

    let response = app.get("/auth/github/recover", Some(&cookie)).await;
    let body = body_string(response).await;
    assert!(body.contains("invalid or expired"));
    let linked: Option<String> =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linked, None);
}

#[tokio::test]
async fn expired_recovery_url_is_rejected() {
    let app = TestApp::new().await;
    let (user_id, _) = app.create_user("Recovering").await;
    let issued = reset_auth(&app.db, &user_id, Utc::now() - Duration::hours(5))
        .await
        .unwrap();

    let response = app
        .get(&format!("/recover?token={}", issued.plaintext_token), None)
        .await;
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    let body = body_string(response).await;
    assert!(body.contains("invalid or expired"));
    assert!(!body.contains(&issued.plaintext_token));
}

#[tokio::test]
async fn token_confirmation_rechecks_expiration() {
    let app = TestApp::new().await;
    let (user_id, _) = app.create_user("Recovering").await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();
    let claim = validate_token(&app.db, &issued.plaintext_token, Utc::now())
        .await
        .unwrap();
    let cookie =
        prepare_token_confirmation(&app, &issued.plaintext_token, "late-code", "905", "late").await;
    sqlx::query("UPDATE auth_connection_tokens SET expires_at = ? WHERE id = ?")
        .bind((Utc::now() - Duration::seconds(1)).to_rfc3339_opts(SecondsFormat::Nanos, true))
        .bind(claim.token_id)
        .execute(&app.db)
        .await
        .unwrap();

    let confirm = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    let body = body_string(confirm).await;
    assert!(body.contains("invalid or expired"));
    let linked: Option<String> =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linked, None);
}

#[tokio::test]
async fn token_confirmation_rechecks_supersession() {
    let app = TestApp::new().await;
    let (user_id, _) = app.create_user("Recovering").await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();
    let cookie = prepare_token_confirmation(
        &app,
        &issued.plaintext_token,
        "superseded-code",
        "908",
        "superseded",
    )
    .await;
    let replacement = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();

    let confirm = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    let body = body_string(confirm).await;
    assert!(body.contains("invalid or expired"));
    let linked: Option<String> =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(linked, None);
    assert!(
        validate_token(&app.db, &replacement.plaintext_token, Utc::now())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn token_confirmation_rejects_duplicate_github_identity() {
    let app = TestApp::new().await;
    let owner_id = app.create_github_user("Owner", "906", "owner").await;
    let issued = issue_invitation(&app.db, "Invited", Utc::now())
        .await
        .unwrap();
    let cookie = prepare_token_confirmation(
        &app,
        &issued.plaintext_token,
        "duplicate-code",
        "906",
        "owner",
    )
    .await;

    let response = app
        .post_form("/auth/github/confirm", "", Some(&cookie))
        .await;
    let body = body_string(response).await;
    assert!(body.contains("already connected"));
    assert!(!body.contains(&owner_id));
    assert!(!body.contains(&issued.user_id));
    let target_github: Option<String> =
        sqlx::query_scalar("SELECT github_user_id FROM users WHERE id = ?")
            .bind(&issued.user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(target_github, None);
    assert!(
        validate_token(&app.db, &issued.plaintext_token, Utc::now())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn token_query_is_removed_before_redirecting_to_github() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Invited", Utc::now())
        .await
        .unwrap();

    let (_, authorization_url, clean_response) =
        begin_connection_oauth(&app, &issued.plaintext_token).await;

    assert_eq!(clean_response.headers()["referrer-policy"], "no-referrer");
    assert_eq!(authorization_url.path(), "/authorize");
    assert!(!authorization_url.as_str().contains(&issued.plaintext_token));
}

#[tokio::test]
async fn recovery_callback_consumes_attempt_before_rejecting_provider_denial() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Invited", Utc::now())
        .await
        .unwrap();
    let (cookie, authorization_url, _) =
        begin_connection_oauth(&app, &issued.plaintext_token).await;
    let state = query_parameter(&authorization_url, "state");

    let denied = app
        .get(
            &format!(
                "/auth/github/callback?error=access_denied&error_description=private&state={state}"
            ),
            Some(&cookie),
        )
        .await;
    let body = body_string(denied).await;
    assert!(body.contains("invalid or expired"));
    assert!(!body.contains("access_denied"));
    assert!(!body.contains("private"));

    app.github.profile_for_code(
        "replay-code",
        GitHubProfile {
            user_id: "907".into(),
            login: "replay".into(),
            name: None,
        },
    );
    let replay = app
        .get(&callback_uri("replay-code", &state), Some(&cookie))
        .await;
    let body = body_string(replay).await;
    assert!(body.contains("couldn’t complete GitHub sign-in"));
}

#[tokio::test]
async fn denied_recovery_callback_validates_its_connection_claim_before_provider_error() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Invited", Utc::now())
        .await
        .unwrap();
    let (cookie, authorization_url, _) =
        begin_connection_oauth(&app, &issued.plaintext_token).await;
    let state = query_parameter(&authorization_url, "state");
    sqlx::query("DROP TABLE auth_connection_tokens")
        .execute(&app.db)
        .await
        .unwrap();

    let (denied, logs) = capture_error_logs(app.get(
        &format!("/auth/github/callback?code=must-not-exchange&error=access_denied&state={state}"),
        Some(&cookie),
    ))
    .await;
    let body = body_string(denied).await;

    assert!(body.contains("invalid or expired"));
    assert!(logs.contains("connection token database operation failed"));
    assert!(!logs.contains("must-not-exchange"));
    assert!(!logs.contains(&issued.plaintext_token));
}

#[tokio::test]
async fn malformed_and_duplicate_recovery_queries_show_the_same_safe_error() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Invited", Utc::now())
        .await
        .unwrap();

    for uri in [
        "/recover?token=%FF".to_owned(),
        format!("/recover?token={0}&token={0}", issued.plaintext_token),
        "/recover?unrelated=value".to_owned(),
    ] {
        let response = app.get(&uri, None).await;
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        let body = body_string(response).await;
        assert!(body.contains("invalid or expired"));
        assert!(!body.contains(&issued.plaintext_token));
        assert!(!body.contains("Database"));
    }

    assert!(
        validate_token(&app.db, &issued.plaintext_token, Utc::now())
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn recovery_token_database_failure_logs_only_safe_operation_context() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Secret", Utc::now())
        .await
        .unwrap();
    let hostile_token = issued.plaintext_token;
    sqlx::query("DROP TABLE auth_connection_tokens")
        .execute(&app.db)
        .await
        .unwrap();

    let (response, logs) =
        capture_error_logs(app.get(&format!("/recover?token={hostile_token}"), None)).await;
    let body = body_string(response).await;

    assert!(body.contains("invalid or expired"));
    assert!(logs.contains("connection token validation failed"));
    assert!(!logs.contains(&hostile_token));
    assert!(!logs.contains("no such table"));
    assert!(!logs.contains("auth_connection_tokens"));
}

#[tokio::test]
async fn invalid_recovery_token_does_not_log_an_operational_failure() {
    let app = TestApp::new().await;

    let (response, logs) = capture_error_logs(app.get("/recover?token=invalid", None)).await;
    let body = body_string(response).await;

    assert!(body.contains("invalid or expired"));
    assert!(
        logs.is_empty(),
        "expected invalid tokens should not log: {logs}"
    );
}

#[tokio::test]
async fn consume_and_link_allows_exactly_one_concurrent_redeemer() {
    let database = FileDatabase::new("concurrent-redemption");
    let pool = database.pool(5).await;
    let now = Utc::now();
    let issued = issue_invitation(&pool, "Concurrent", now).await.unwrap();
    let claim = validate_token(&pool, &issued.plaintext_token, now)
        .await
        .unwrap();
    let profile = GitHubProfile {
        user_id: "concurrent-github-id".into(),
        login: "concurrent-login".into(),
        name: None,
    };
    let barrier = Arc::new(tokio::sync::Barrier::new(3));
    let first = {
        let pool = pool.clone();
        let claim = claim.clone();
        let profile = profile.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            consume_and_link(&pool, &claim, &profile, now).await
        })
    };
    let second = {
        let pool = pool.clone();
        let claim = claim.clone();
        let profile = profile.clone();
        let barrier = barrier.clone();
        tokio::spawn(async move {
            barrier.wait().await;
            consume_and_link(&pool, &claim, &profile, now).await
        })
    };

    barrier.wait().await;
    let results = [first.await.unwrap(), second.await.unwrap()];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(ConnectionTokenError::InvalidToken)))
            .count(),
        1
    );
    let user: (Option<String>, Option<String>, i64) =
        sqlx::query_as("SELECT github_user_id, github_login, auth_version FROM users WHERE id = ?")
            .bind(&issued.user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        user,
        (
            Some("concurrent-github-id".into()),
            Some("concurrent-login".into()),
            2
        )
    );
    let consumed_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM auth_connection_tokens WHERE user_id = ? AND consumed_at IS NOT NULL",
    )
    .bind(&issued.user_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(consumed_count, 1);
    pool.close().await;
}

#[tokio::test]
async fn consume_and_link_rolls_back_token_transition_when_user_update_fails() {
    let database = FileDatabase::new("redemption-rollback");
    let pool = database.pool(2).await;
    let now = Utc::now();
    let issued = issue_invitation(&pool, "Rollback", now).await.unwrap();
    let claim = validate_token(&pool, &issued.plaintext_token, now)
        .await
        .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_github_link BEFORE UPDATE OF github_user_id ON users \
         BEGIN SELECT RAISE(ABORT, 'hostile-trigger-secret'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let profile = GitHubProfile {
        user_id: "rollback-github-id".into(),
        login: "rollback-login".into(),
        name: None,
    };

    let error = consume_and_link(&pool, &claim, &profile, now)
        .await
        .unwrap_err();

    assert!(matches!(error, ConnectionTokenError::Database(_)));
    let consumed_at: Option<String> =
        sqlx::query_scalar("SELECT consumed_at FROM auth_connection_tokens WHERE id = ?")
            .bind(&claim.token_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(consumed_at, None);
    let user: (Option<String>, Option<String>, i64) =
        sqlx::query_as("SELECT github_user_id, github_login, auth_version FROM users WHERE id = ?")
            .bind(&claim.user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(user, (None, None, 1));
    pool.close().await;
}

#[tokio::test]
async fn invitation_is_hashed_single_use_and_expires_in_four_hours() {
    let app = TestApp::new().await;
    let now = Utc::now();
    let issued = issue_invitation(&app.db, "Guest", now).await.unwrap();

    assert_eq!(issued.expires_at, now + Duration::hours(4));
    let stored: String =
        sqlx::query_scalar("SELECT token_hash FROM auth_connection_tokens WHERE user_id = ?")
            .bind(&issued.user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_ne!(stored, issued.plaintext_token);

    let claim = validate_token(&app.db, &issued.plaintext_token, now)
        .await
        .unwrap();
    assert_eq!(claim.user_id, issued.user_id);
    assert_eq!(claim.purpose, ConnectionPurpose::Invite);
}

#[tokio::test]
async fn new_invitation_supersedes_an_older_token() {
    let app = TestApp::new().await;
    let first_now = Utc::now();
    let invitation = issue_invitation(&app.db, "Guest", first_now).await.unwrap();

    let replacement_now = first_now + Duration::minutes(5);
    let replacement = reset_auth(&app.db, &invitation.user_id, replacement_now)
        .await
        .unwrap();

    assert_eq!(replacement.user_id, invitation.user_id);
    let consumed_at: Option<String> = sqlx::query_scalar(
        "SELECT consumed_at FROM auth_connection_tokens WHERE user_id = ? AND purpose = 'invite'",
    )
    .bind(&invitation.user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        consumed_at.as_deref(),
        Some(
            replacement_now
                .to_rfc3339_opts(SecondsFormat::Nanos, true)
                .as_str()
        )
    );
    assert!(
        validate_token(&app.db, &invitation.plaintext_token, replacement_now)
            .await
            .is_err()
    );
    assert_eq!(
        validate_token(&app.db, &replacement.plaintext_token, replacement_now)
            .await
            .unwrap()
            .purpose,
        ConnectionPurpose::Recovery
    );
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let app = TestApp::new().await;
    let now = Utc::now();
    let issued = issue_invitation(&app.db, "Guest", now).await.unwrap();

    let error = validate_token(&app.db, &issued.plaintext_token, now + Duration::hours(4))
        .await
        .unwrap_err();

    assert!(matches!(error, ConnectionTokenError::InvalidToken));
}

#[tokio::test]
async fn exact_second_expiry_is_active_before_and_expired_at_the_boundary() {
    let app = TestApp::new().await;
    let now = Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap();
    let issued = issue_invitation(&app.db, "Exact", now).await.unwrap();
    let (created_at, expires_at): (String, String) = sqlx::query_as(
        "SELECT created_at, expires_at FROM auth_connection_tokens WHERE user_id = ?",
    )
    .bind(&issued.user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();

    assert_eq!(created_at, now.to_rfc3339_opts(SecondsFormat::Nanos, true));
    assert_eq!(
        expires_at,
        issued
            .expires_at
            .to_rfc3339_opts(SecondsFormat::Nanos, true)
    );
    assert!(
        validate_token(
            &app.db,
            &issued.plaintext_token,
            issued.expires_at - Duration::nanoseconds(1),
        )
        .await
        .is_ok()
    );
    assert!(
        validate_token(&app.db, &issued.plaintext_token, issued.expires_at)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn fractional_expiry_is_active_before_and_expired_after_the_boundary() {
    let app = TestApp::new().await;
    let now =
        Utc.with_ymd_and_hms(2026, 8, 12, 12, 0, 0).unwrap() + Duration::nanoseconds(1_234_000);
    let issued = issue_invitation(&app.db, "Fractional", now).await.unwrap();
    let expires_at: String =
        sqlx::query_scalar("SELECT expires_at FROM auth_connection_tokens WHERE user_id = ?")
            .bind(&issued.user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    assert_eq!(
        expires_at,
        issued
            .expires_at
            .to_rfc3339_opts(SecondsFormat::Nanos, true)
    );
    assert!(
        validate_token(
            &app.db,
            &issued.plaintext_token,
            issued.expires_at - Duration::nanoseconds(1),
        )
        .await
        .is_ok()
    );
    assert!(
        validate_token(
            &app.db,
            &issued.plaintext_token,
            issued.expires_at + Duration::nanoseconds(1),
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn malformed_token_is_rejected_like_an_expired_token() {
    let app = TestApp::new().await;
    let now = Utc::now();
    let issued = issue_invitation(&app.db, "Guest", now).await.unwrap();
    let expired = validate_token(&app.db, &issued.plaintext_token, now + Duration::hours(4))
        .await
        .unwrap_err();

    for malformed in ["short", "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"] {
        let error = validate_token(&app.db, malformed, now).await.unwrap_err();
        assert_eq!(error.to_string(), expired.to_string());
        assert_eq!(format!("{error:?}"), format!("{expired:?}"));
    }
}

#[tokio::test]
async fn reset_auth_disconnects_github_and_increments_auth_version() {
    let app = TestApp::new().await;
    let user_id = app.create_github_user("Guest", "12345", "octoguest").await;
    sqlx::query("UPDATE users SET invite_code = 'legacy-code' WHERE id = ?")
        .bind(&user_id)
        .execute(&app.db)
        .await
        .unwrap();
    let now = Utc::now();

    let issued = reset_auth(&app.db, &user_id, now).await.unwrap();

    let user: (Option<String>, Option<String>, Option<String>, i64, String) = sqlx::query_as(
        "SELECT github_user_id, github_login, invite_code, auth_version, updated_at FROM users WHERE id = ?",
    )
    .bind(&user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        user,
        (
            None,
            None,
            None,
            2,
            now.to_rfc3339_opts(SecondsFormat::Nanos, true)
        )
    );
    assert_eq!(issued.user_id, user_id);
    assert_eq!(issued.expires_at, now + Duration::hours(4));
    assert_eq!(
        validate_token(&app.db, &issued.plaintext_token, now)
            .await
            .unwrap()
            .purpose,
        ConnectionPurpose::Recovery
    );
}

#[tokio::test]
async fn reset_auth_rejects_an_unknown_user() {
    let app = TestApp::new().await;

    let error = match reset_auth(&app.db, "missing-user", Utc::now()).await {
        Ok(_) => panic!("an unknown user must not receive a recovery token"),
        Err(error) => error,
    };

    assert!(matches!(error, ConnectionTokenError::UserNotFound));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_connection_tokens")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn connection_url_uses_public_base_and_percent_encoding() {
    let base = Url::parse("https://interne.test/").unwrap();

    let url = connection_url(&base, "a/b? c");

    assert_eq!(
        url.as_str(),
        "https://interne.test/recover?token=a%2Fb%3F+c"
    );
}

#[tokio::test]
async fn plaintext_token_is_never_written_to_sqlite() {
    let database = FileDatabase::new("token-storage");
    let options = sqlx::sqlite::SqliteConnectOptions::from_str(&database.url())
        .unwrap()
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let now = Utc::now();
    let issued = issue_invitation(&pool, "Guest", now).await.unwrap();

    let stored_hash: String =
        sqlx::query_scalar("SELECT token_hash FROM auth_connection_tokens WHERE user_id = ?")
            .bind(&issued.user_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(stored_hash, issued.plaintext_token);
    assert!(
        validate_token(&pool, &issued.plaintext_token, now)
            .await
            .is_ok()
    );

    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .fetch_all(&pool)
        .await
        .unwrap();
    pool.close().await;

    let plaintext = issued.plaintext_token.as_bytes();
    for artifact in database
        .artifacts()
        .into_iter()
        .filter(|path| path.exists())
    {
        let bytes = std::fs::read(&artifact).unwrap();
        assert!(
            !bytes
                .windows(plaintext.len())
                .any(|window| window == plaintext),
            "plaintext token found in {}",
            artifact.display()
        );
    }
}

#[tokio::test]
async fn invitation_rolls_back_user_creation_when_token_insert_fails() {
    let app = TestApp::new().await;
    sqlx::query(
        "CREATE TRIGGER reject_connection_tokens BEFORE INSERT ON auth_connection_tokens BEGIN SELECT RAISE(ABORT, 'rejected'); END",
    )
    .execute(&app.db)
    .await
    .unwrap();

    assert!(
        issue_invitation(&app.db, "Rolled Back", Utc::now())
            .await
            .is_err()
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE name = 'Rolled Back'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn reset_auth_rolls_back_disconnection_when_token_insert_fails() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Guest", Utc::now())
        .await
        .unwrap();
    let user_id = issued.user_id;
    let original_updated_at = "2025-02-03T04:05:06.000000000Z";
    sqlx::query(
        "UPDATE users SET invite_code = 'legacy-code', github_user_id = '12345', \
         github_login = 'octoguest', auth_version = 7, updated_at = ? WHERE id = ?",
    )
    .bind(original_updated_at)
    .bind(&user_id)
    .execute(&app.db)
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER reject_connection_tokens BEFORE INSERT ON auth_connection_tokens BEGIN SELECT RAISE(ABORT, 'rejected'); END",
    )
    .execute(&app.db)
    .await
    .unwrap();

    assert!(reset_auth(&app.db, &user_id, Utc::now()).await.is_err());

    let user: (Option<String>, Option<String>, Option<String>, i64, String) = sqlx::query_as(
        "SELECT invite_code, github_user_id, github_login, auth_version, updated_at FROM users WHERE id = ?",
    )
    .bind(&user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        user,
        (
            Some("legacy-code".into()),
            Some("12345".into()),
            Some("octoguest".into()),
            7,
            original_updated_at.into()
        )
    );
    let predecessor_consumed_at: Option<String> =
        sqlx::query_scalar("SELECT consumed_at FROM auth_connection_tokens WHERE user_id = ?")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(predecessor_consumed_at, None);
}

struct FileDatabase {
    path: std::path::PathBuf,
}

impl FileDatabase {
    fn new(label: &str) -> Self {
        Self {
            path: std::env::temp_dir().join(format!("interne-{label}-{}.db", uuid::Uuid::new_v4())),
        }
    }

    fn url(&self) -> String {
        format!("sqlite:{}", self.path.display())
    }

    fn artifacts(&self) -> [std::path::PathBuf; 3] {
        let with_suffix = |suffix: &str| {
            let mut path = self.path.as_os_str().to_os_string();
            path.push(suffix);
            std::path::PathBuf::from(path)
        };
        [self.path.clone(), with_suffix("-wal"), with_suffix("-shm")]
    }

    async fn pool(&self, max_connections: u32) -> sqlx::SqlitePool {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str(&self.url())
            .unwrap()
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(StdDuration::from_secs(5));
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(max_connections)
            .connect_with(options)
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }
}

impl Drop for FileDatabase {
    fn drop(&mut self) {
        for path in self.artifacts() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn run_cli(database: &FileDatabase, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_interne"))
        .args(arguments)
        .env_clear()
        .env("DATABASE_URL", database.url())
        .env("PUBLIC_BASE_URL", "https://interne.test")
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

#[tokio::test]
async fn invitation_and_reset_commands_do_not_require_github_credentials() {
    let database = FileDatabase::new("cli");
    let invite = run_cli(&database, &["invite-user", "CLI Guest"]);
    assert!(
        invite.status.success(),
        "{}",
        String::from_utf8_lossy(&invite.stderr)
    );
    let stdout = String::from_utf8(invite.stdout).unwrap();
    assert_eq!(stdout.matches("/recover?token=").count(), 1);
    let user_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("User ID: "))
        .expect("invite command prints the user ID");

    let reset = run_cli(&database, &["reset-auth", user_id]);
    assert!(
        reset.status.success(),
        "{}",
        String::from_utf8_lossy(&reset.stderr)
    );
    let stdout = String::from_utf8(reset.stdout).unwrap();
    assert_eq!(stdout.matches("/recover?token=").count(), 1);
}

#[tokio::test]
async fn deprecated_create_user_alias_ignores_email_and_issues_an_invitation() {
    let database = FileDatabase::new("cli");

    let output = run_cli(
        &database,
        &["create-user", "Legacy Guest", "ignored@example.com"],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stdout.matches("/recover?token=").count(), 1);
    assert!(stderr.contains("deprecated"));
    assert!(stderr.contains("email is ignored"));

    let options = sqlx::sqlite::SqliteConnectOptions::from_str(&database.url()).unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let email: Option<String> =
        sqlx::query_scalar("SELECT email FROM users WHERE name = 'Legacy Guest'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(email, None);
}

#[test]
fn cli_help_names_connection_arguments_and_four_hour_expiry() {
    let database = FileDatabase::new("cli");

    let output = run_cli(&database, &["help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("invite-user <name>"));
    assert!(stdout.contains("reset-auth <user-id>"));
    assert!(stdout.contains("four hours"));
}
