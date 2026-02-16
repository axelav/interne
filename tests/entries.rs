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

#[tokio::test]
async fn visit_entry_updates_availability() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create an available entry (never dismissed)
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Visit Me")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Entry should appear on home page (available)
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Visit Me"));

    // Visit the entry
    let resp = app
        .post_form(&format!("/entries/{}/visit", entry_id), "", Some(&cookie))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify visit record was created
    let visit_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits WHERE entry_id = ?")
        .bind(&entry_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(visit_count.0, 1);

    // Verify dismissed_at was set
    let entry: (Option<String>,) =
        sqlx::query_as("SELECT dismissed_at FROM entries WHERE id = ?")
            .bind(&entry_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(entry.0.is_some());

    // Entry should NOT appear on home page (no longer available)
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(!html.contains("Visit Me"));

    // But should appear on /all
    let resp = app.get("/all", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Visit Me"));
}

#[tokio::test]
async fn home_shows_only_available_entries() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Available entry (dismissed long ago)
    let e1 = uuid::Uuid::new_v4().to_string();
    let old = (now - chrono::Duration::days(30)).to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, dismissed_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&e1)
    .bind(&user_id)
    .bind("https://example.com/1")
    .bind("Available Entry")
    .bind(3)
    .bind("days")
    .bind(&old)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&app.db)
    .await
    .unwrap();

    // Not-yet-due entry (dismissed just now)
    let e2 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, dismissed_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&e2)
    .bind(&user_id)
    .bind("https://example.com/2")
    .bind("Not Yet Due")
    .bind(3)
    .bind("days")
    .bind(&now_str)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&app.db)
    .await
    .unwrap();

    // Home page should show only available
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Available Entry"));
    assert!(!html.contains("Not Yet Due"));

    // /all should show both
    let resp = app.get("/all", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Available Entry"));
    assert!(html.contains("Not Yet Due"));
}

#[tokio::test]
async fn collection_member_sees_shared_entries() {
    let app = TestApp::new().await;
    let (owner_id, _owner_invite) = app.create_user("Owner").await;
    let (member_id, member_invite) = app.create_user("Member").await;

    // Create collection
    let collection_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&owner_id)
    .bind("Shared")
    .bind("col-invite")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Add member
    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Create entry in collection (owned by owner)
    let entry_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, collection_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind(&collection_id)
    .bind("https://example.com")
    .bind("Shared Entry")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Member should see the entry
    let cookie = app.login(&member_invite).await;
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Shared Entry"));
}

#[tokio::test]
async fn leaving_collection_hides_shared_entries() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (member_id, member_invite) = app.create_user("Member").await;

    let collection_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&owner_id)
    .bind("Shared")
    .bind("col-invite-2")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let entry_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, collection_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind(&collection_id)
    .bind("https://example.com")
    .bind("Shared Entry")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let cookie = app.login(&member_invite).await;

    // Member leaves collection
    let resp = app
        .post_form(
            &format!("/collections/{}/leave", collection_id),
            "",
            Some(&cookie),
        )
        .await;
    assert_redirect(&resp, "/collections");

    // Entry should no longer appear
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(!html.contains("Shared Entry"));
}

#[tokio::test]
async fn create_entry_with_tags() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Tagged+Entry&description=&duration=3&interval=days&tags=rust%2C+web&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    // Verify tags were created
    let tags: Vec<(String,)> = sqlx::query_as("SELECT name FROM tags ORDER BY name")
        .fetch_all(&app.db)
        .await
        .unwrap();
    let tag_names: Vec<&str> = tags.iter().map(|(n,)| n.as_str()).collect();
    assert!(tag_names.contains(&"rust"));
    assert!(tag_names.contains(&"web"));

    // Verify entry_tags links
    let links: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entry_tags")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(links.0, 2);
}

#[tokio::test]
async fn update_entry_replaces_tags() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create entry with tags
    let body = "url=https%3A%2F%2Fexample.com&title=Tagged&description=&duration=3&interval=days&tags=rust%2C+web&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    // Find the entry
    let (entry_id,): (String,) =
        sqlx::query_as("SELECT id FROM entries WHERE user_id = ?")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Update with different tags
    let body = "url=https%3A%2F%2Fexample.com&title=Tagged&description=&duration=3&interval=days&tags=python%2C+api&collection_id=";
    let resp = app
        .post_form(&format!("/entries/{}", entry_id), body, Some(&cookie))
        .await;
    assert_redirect(&resp, "/");

    // Verify old tags unlinked, new tags linked
    let links: Vec<(String,)> = sqlx::query_as(
        "SELECT t.name FROM tags t JOIN entry_tags et ON et.tag_id = t.id WHERE et.entry_id = ? ORDER BY t.name",
    )
    .bind(&entry_id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    let linked: Vec<&str> = links.iter().map(|(n,)| n.as_str()).collect();
    assert_eq!(linked, vec!["api", "python"]);
}
