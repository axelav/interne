mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::{TestApp, assert_redirect, body_string};
use serde_json::json;

#[tokio::test]
async fn only_signed_in_users_can_request_suggestions_and_missing_key_does_not_block_save() {
    let app = TestApp::new().await;
    let (_, github_id) = app.create_user("Reader").await;
    let cookie = app.login(&github_id).await;
    let target = "https://example.com/article";

    let request = || Request::post("/entries/suggest").header("content-type", "application/json");
    let payload = || Body::from(json!({"url": target}).to_string());
    let anonymous = app.request(request().body(payload()).unwrap()).await;
    assert_redirect(&anonymous, "/login");

    let form = app.get("/entries/new", Some(&cookie)).await;
    assert!(body_string(form).await.contains("Suggest details"));
    let unavailable = app
        .request(request().header("cookie", &cookie).body(payload()).unwrap())
        .await;
    assert_eq!(unavailable.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(body_string(unavailable).await.contains("unavailable"));

    let saved = app
        .post_form(
            "/entries",
            "url=https%3A%2F%2Fexample.com%2Farticle&title=Manual+title&description=Manual+description&duration=1&interval=weeks",
            Some(&cookie),
        )
        .await;
    assert_redirect(&saved, "/");
    let links = body_string(app.get("/", Some(&cookie)).await).await;
    assert!(links.contains(target));
    assert!(links.contains("Manual title"));
    assert!(links.contains("Manual description"));
}
