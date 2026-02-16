mod common;

use axum::http::StatusCode;
use common::{assert_hx_redirect, assert_redirect, body_string, TestApp};

#[tokio::test]
async fn create_entry_with_valid_form() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Test+Entry&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    // Verify entry appears on home page
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Test Entry"));
}

#[tokio::test]
async fn create_entry_with_empty_title_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Title is required"));
}

#[tokio::test]
async fn create_entry_with_bad_url_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=ftp%3A%2F%2Fexample.com&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("URL must start with http"));
}

#[tokio::test]
async fn create_entry_with_zero_duration_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Test&description=&duration=0&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Duration must be at least 1"));
}

#[tokio::test]
async fn edit_entry_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create entry directly in DB
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Original Title")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // GET edit form
    let resp = app
        .get(&format!("/entries/{}/edit", entry_id), Some(&cookie))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Original Title"));

    // POST update
    let body = format!(
        "url=https%3A%2F%2Fexample.com&title=Updated+Title&description=&duration=5&interval=weeks&tags=&collection_id="
    );
    let resp = app
        .post_form(&format!("/entries/{}", entry_id), &body, Some(&cookie))
        .await;
    assert_redirect(&resp, "/");
}

#[tokio::test]
async fn edit_entry_as_non_owner_redirects() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (_, other_invite) = app.create_user("Other").await;
    let cookie = app.login(&other_invite).await;

    // Create entry owned by someone else
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind("https://example.com")
    .bind("Not Yours")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Other user tries to edit
    let resp = app
        .get(&format!("/entries/{}/edit", entry_id), Some(&cookie))
        .await;
    assert_redirect(&resp, "/");
}

#[tokio::test]
async fn delete_entry_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Delete Me")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .delete(&format!("/entries/{}", entry_id), Some(&cookie))
        .await;
    assert_hx_redirect(&resp, "/");

    // Verify entry is gone
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entries WHERE id = ?")
        .bind(&entry_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn delete_entry_as_non_owner() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (_, other_invite) = app.create_user("Other").await;
    let cookie = app.login(&other_invite).await;

    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind("https://example.com")
    .bind("Not Yours")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .delete(&format!("/entries/{}", entry_id), Some(&cookie))
        .await;
    // Should redirect without deleting
    assert_hx_redirect(&resp, "/");

    // Entry still exists
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entries WHERE id = ?")
        .bind(&entry_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}
