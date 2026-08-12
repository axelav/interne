mod common;

use std::collections::HashMap;

use axum::http::{Response, StatusCode};
use common::{TestApp, assert_redirect, body_string};
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
