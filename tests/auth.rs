mod common;

use axum::http::StatusCode;
use common::{TestApp, assert_redirect, body_string, cookie_from_response};

#[tokio::test]
async fn valid_legacy_code_redirects_to_github_connection() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Test User").await;

    let response = app
        .post_form("/login", &format!("invite_code={invite_code}"), None)
        .await;

    assert_redirect(&response, "/auth/connect");
    assert!(response.headers().get("set-cookie").is_some());
}

#[tokio::test]
async fn legacy_session_cannot_access_application_data() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Test User").await;
    let cookie = app.legacy_login(&invite_code).await;

    let response = app.get("/", Some(&cookie)).await;

    assert_redirect(&response, "/login");
}

#[tokio::test]
async fn login_with_invalid_invite_code() {
    let app = TestApp::new().await;

    let resp = app.post_form("/login", "invite_code=bad-code", None).await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("Invalid invite code"));
}

#[tokio::test]
async fn linked_and_retired_legacy_codes_use_the_generic_invalid_error() {
    for retire_by_linking in [true, false] {
        let app = TestApp::new().await;
        let (user_id, invite_code) = app.create_legacy_user("Test User").await;
        if retire_by_linking {
            sqlx::query(
                "UPDATE users SET github_user_id = 'already-linked', github_login = 'linked' \
                 WHERE id = ?",
            )
            .bind(user_id)
            .execute(&app.db)
            .await
            .unwrap();
        } else {
            sqlx::query("UPDATE users SET invite_code = NULL WHERE id = ?")
                .bind(user_id)
                .execute(&app.db)
                .await
                .unwrap();
        }

        let response = app
            .post_form("/login", &format!("invite_code={invite_code}"), None)
            .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_string(response).await.contains("Invalid invite code"));
    }
}

#[tokio::test]
async fn legacy_login_replaces_an_existing_full_session() {
    let app = TestApp::new().await;
    let (_linked_user_id, github_user_id) = app.create_user("Linked User").await;
    let full_cookie = app.login(&github_user_id).await;
    let (_legacy_user_id, invite_code) = app.create_legacy_user("Legacy User").await;

    let response = app
        .post_form(
            "/login",
            &format!("invite_code={invite_code}"),
            Some(&full_cookie),
        )
        .await;
    assert_redirect(&response, "/auth/connect");
    let migration_cookie = cookie_from_response(
        response
            .headers()
            .get("set-cookie")
            .expect("Legacy login should cycle the session"),
    );

    assert_redirect(&app.get("/", Some(&full_cookie)).await, "/login");
    assert_redirect(&app.get("/", Some(&migration_cookie)).await, "/login");
}

#[tokio::test]
async fn logout_clears_session() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let resp = app.post_form("/logout", "", Some(&cookie)).await;
    assert_redirect(&resp, "/login");

    // After logout, accessing / should redirect to login
    let resp = app.get("/", Some(&cookie)).await;
    assert_redirect(&resp, "/login");
}

#[tokio::test]
async fn changing_auth_version_revokes_an_existing_session() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    sqlx::query("UPDATE users SET auth_version = auth_version + 1 WHERE id = ?")
        .bind(&user_id)
        .execute(&app.db)
        .await
        .unwrap();

    let response = app.get("/", Some(&cookie)).await;
    assert_redirect(&response, "/login");
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
