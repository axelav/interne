mod common;

use axum::http::StatusCode;
use common::{assert_redirect, body_string, TestApp};

#[tokio::test]
async fn login_with_valid_invite_code() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;

    let resp = app
        .post_form("/login", &format!("invite_code={}", invite_code), None)
        .await;

    assert_redirect(&resp, "/");
    assert!(resp.headers().get("set-cookie").is_some());
}

#[tokio::test]
async fn login_with_invalid_invite_code() {
    let app = TestApp::new().await;

    let resp = app
        .post_form("/login", "invite_code=bad-code", None)
        .await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("Invalid invite code"));
}

#[tokio::test]
async fn logout_clears_session() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let resp = app.post_form("/logout", "", Some(&cookie)).await;
    assert_redirect(&resp, "/login");

    // After logout, accessing / should redirect to login
    let resp = app.get("/", Some(&cookie)).await;
    assert_redirect(&resp, "/login");
}

#[tokio::test]
async fn unauthenticated_index_redirects_to_login() {
    let app = TestApp::new().await;
    let resp = app.get("/", None).await;
    assert_redirect(&resp, "/login");
}

#[tokio::test]
async fn unauthenticated_new_entry_redirects_to_login() {
    let app = TestApp::new().await;
    let resp = app.get("/entries/new", None).await;
    assert_redirect(&resp, "/login");
}
