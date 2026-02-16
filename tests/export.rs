mod common;

use axum::http::StatusCode;
use common::{body_string, TestApp};

#[tokio::test]
async fn export_returns_json_with_entries() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create an entry with tags
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Export Test")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let tag_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
        .bind(&tag_id)
        .bind("rust")
        .bind(&now)
        .execute(&app.db)
        .await
        .unwrap();

    sqlx::query("INSERT INTO entry_tags (entry_id, tag_id) VALUES (?, ?)")
        .bind(&entry_id)
        .bind(&tag_id)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app.get("/export", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Check Content-Disposition header
    let content_disposition = resp
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_disposition.starts_with("attachment; filename="));
    assert!(content_disposition.contains("interne-export-"));

    // Check JSON content
    let body = body_string(resp).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert!(json["exported_at"].is_string());
    assert_eq!(json["entries"].as_array().unwrap().len(), 1);
    assert_eq!(json["entries"][0]["title"], "Export Test");
    assert_eq!(json["entries"][0]["tags"][0], "rust");
}

#[tokio::test]
async fn export_unauthenticated_redirects() {
    let app = TestApp::new().await;
    let resp = app.get("/export", None).await;
    common::assert_redirect(&resp, "/login");
}
