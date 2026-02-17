mod common;

use axum::http::StatusCode;
use common::{body_string, TestApp};

#[tokio::test]
async fn tags_page_requires_auth() {
    let app = TestApp::new().await;
    let resp = app.get("/tags", None).await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn tags_page_empty_state() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let resp = app.get("/tags", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("No tags yet."));
}

#[tokio::test]
async fn tags_page_shows_user_tags() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create entry with tags
    let body = "url=https%3A%2F%2Fexample.com&title=Tagged+Entry&description=&duration=3&interval=days&tags=rust%2C+music&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    let resp = app.get("/tags", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("rust"));
    assert!(html.contains("music"));
}

#[tokio::test]
async fn tags_page_does_not_show_other_users_tags() {
    let app = TestApp::new().await;
    let (_user1_id, invite1) = app.create_user("User 1").await;
    let cookie1 = app.login(&invite1).await;

    // User 1 creates entry with tag
    let body = "url=https%3A%2F%2Fexample.com&title=Entry1&description=&duration=3&interval=days&tags=secret&collection_id=";
    app.post_form("/entries", body, Some(&cookie1)).await;

    // User 2 should not see User 1's tags
    let (_user2_id, invite2) = app.create_user("User 2").await;
    let cookie2 = app.login(&invite2).await;

    let resp = app.get("/tags", Some(&cookie2)).await;
    let html = body_string(resp).await;
    assert!(!html.contains("secret"));
    assert!(html.contains("No tags yet."));
}

#[tokio::test]
async fn tag_detail_shows_entries_for_tag() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create two entries, one with "rust" tag, one without
    let body = "url=https%3A%2F%2Fexample.com%2F1&title=Rust+Article&description=&duration=3&interval=days&tags=rust&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    let body = "url=https%3A%2F%2Fexample.com%2F2&title=Untagged+Article&description=&duration=3&interval=days&tags=&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    let resp = app.get("/tags/rust", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Rust Article"));
    assert!(!html.contains("Untagged Article"));
    assert!(html.contains("rust (1)"));
}

#[tokio::test]
async fn tag_detail_shows_empty_for_nonexistent_tag() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let resp = app.get("/tags/nonexistent", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("No links with this tag."));
}

#[tokio::test]
async fn tag_detail_has_back_link() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let resp = app.get("/tags/anything", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("href=\"/tags\""));
}

#[tokio::test]
async fn tag_cloud_has_inline_styles() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Entry&description=&duration=3&interval=days&tags=styled&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    let resp = app.get("/tags", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("font-size:"));
    assert!(html.contains("hsl("));
}
