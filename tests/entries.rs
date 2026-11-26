mod common;

use axum::http::StatusCode;
use common::{TestApp, assert_hx_redirect, assert_redirect, body_string};

#[tokio::test]
async fn create_entry_with_valid_form() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let body = "url=https%3A%2F%2Fexample.com&title=&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Title is required"));
}

#[tokio::test]
async fn create_entry_with_bare_domain_normalizes_url() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    // "yahoo.com" should be accepted and normalized to "https://yahoo.com/"
    let body =
        "url=yahoo.com&title=Yahoo&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "https://yahoo.com/");
}

#[tokio::test]
async fn create_entry_with_https_url_preserves_it() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let body = "url=https%3A%2F%2Fexample.com%2Fpath&title=Example&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "https://example.com/path");
}

#[tokio::test]
async fn create_entry_with_http_url_preserves_it() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let body = "url=http%3A%2F%2Fexample.com&title=Example&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "http://example.com/");
}

#[tokio::test]
async fn create_entry_with_invalid_url_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    // "not a url" has no valid domain structure
    let body =
        "url=not+a+url&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Please enter a valid URL"));
}

#[tokio::test]
async fn create_entry_with_bare_word_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    // "yahoo" alone is not a valid URL even after normalization
    let body = "url=yahoo&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Please enter a valid URL"));
}

#[tokio::test]
async fn create_entry_with_ftp_url_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let body = "url=ftp%3A%2F%2Fexample.com&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Please enter a valid URL"));
}

#[tokio::test]
async fn create_entry_with_path_and_query_normalizes() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let body = "url=example.com%2Fpath%3Fq%3D1&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "https://example.com/path?q=1");
}

#[tokio::test]
async fn create_entry_with_zero_duration_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Test&description=&duration=0&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Duration must be at least 1"));
}

#[tokio::test]
async fn edit_entry_as_owner() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    let body = "url=https%3A%2F%2Fexample.com&title=Updated+Title&description=&duration=5&interval=weeks&tags=&collection_id=";
    let resp = app
        .post_form(&format!("/entries/{}", entry_id), body, Some(&cookie))
        .await;
    assert_redirect(&resp, "/");
}

#[tokio::test]
async fn edit_entry_as_non_owner_redirects() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (_, other_github_user_id) = app.create_user("Other").await;
    let cookie = app.login(&other_github_user_id).await;

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
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    let (_, other_github_user_id) = app.create_user("Other").await;
    let cookie = app.login(&other_github_user_id).await;

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
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    let entry: (Option<String>,) = sqlx::query_as("SELECT dismissed_at FROM entries WHERE id = ?")
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
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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

async fn shared_collection(app: &TestApp) -> (String, String, String) {
    let (_, owner_login) = app.create_user("Owner").await;
    let (_, member_login) = app.create_user("Member").await;
    let owner = app.login(&owner_login).await;
    let member = app.login(&member_login).await;
    let response = app
        .post_form("/collections", "name=Shared", Some(&owner))
        .await;
    assert_redirect(&response, "/collections");
    let (id, invite): (String, String) = sqlx::query_as("SELECT id, invite_code FROM collections")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let response = app
        .post_form(
            "/collections/join",
            &format!("invite_code={invite}"),
            Some(&member),
        )
        .await;
    assert_redirect(&response, "/collections");
    (id, owner, member)
}

fn collection_entry_form(title: &str, collection_id: &str, tag: &str) -> String {
    format!(
        "url=example.com&title={title}&duration=3&interval=days&collection_id={collection_id}&tags={tag}"
    )
}

#[tokio::test]
async fn owner_and_member_can_create_and_move_entries_between_shared_and_private() {
    let app = TestApp::new().await;
    let (collection, owner, member) = shared_collection(&app).await;

    for (cookie, title) in [(&owner, "First"), (&member, "Second")] {
        let saved_entry = || {
            sqlx::query_as::<_, (String, Option<String>)>(
                "SELECT id, collection_id FROM entries WHERE title = ?",
            )
            .bind(title)
            .fetch_one(&app.db)
        };
        let shared = collection_entry_form(title, &collection, "");
        let response = app.post_form("/entries", &shared, Some(cookie)).await;
        assert_redirect(&response, "/");
        let (id, destination) = saved_entry().await.unwrap();
        assert_eq!(destination.as_ref(), Some(&collection));

        let path = format!("/entries/{id}");
        let private = collection_entry_form(title, "", "");
        let response = app.post_form(&path, &private, Some(cookie)).await;
        assert_redirect(&response, "/");
        assert_eq!(saved_entry().await.unwrap().1, None);

        let response = app.post_form(&path, &shared, Some(cookie)).await;
        assert_redirect(&response, "/");
        assert_eq!(saved_entry().await.unwrap().1.as_ref(), Some(&collection));
    }
}

#[tokio::test]
async fn stale_collection_forms_leave_entries_and_tags_unchanged_after_membership_ends() {
    let app = TestApp::new().await;
    let (collection, owner, member) = shared_collection(&app).await;
    let private = "url=example.com&title=Original&duration=3&interval=days&tags=original";
    let response = app.post_form("/entries", private, Some(&member)).await;
    assert_redirect(&response, "/");
    let (id, updated_at): (String, String) = sqlx::query_as("SELECT id, updated_at FROM entries")
        .fetch_one(&app.db)
        .await
        .unwrap();
    let path = format!("/entries/{id}");
    for form in ["/entries/new".to_owned(), format!("{path}/edit")] {
        let response = app.get(&form, Some(&member)).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_string(response).await.contains(&collection));
    }
    let draft = collection_entry_form("Revised", &collection, "revised");
    let submit_drafts = || async {
        for action in ["/entries", &path] {
            let response = app.post_form(action, &draft, Some(&member)).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(body_string(response).await, "Not found");
        }

        let entries: Vec<(String, String, Option<String>, String)> =
            sqlx::query_as("SELECT id, title, collection_id, updated_at FROM entries")
                .fetch_all(&app.db)
                .await
                .unwrap();
        assert_eq!(
            entries,
            [(id.clone(), "Original".to_owned(), None, updated_at.clone())]
        );
        let tags: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT t.name, et.entry_id FROM tags t LEFT JOIN entry_tags et ON et.tag_id = t.id",
        )
        .fetch_all(&app.db)
        .await
        .unwrap();
        assert_eq!(tags, [("original".to_owned(), Some(id.clone()))]);
    };

    let response = app
        .post_form(
            &format!("/collections/{collection}/leave"),
            "",
            Some(&member),
        )
        .await;
    assert_redirect(&response, "/collections");
    submit_drafts().await;

    let response = app
        .delete(&format!("/collections/{collection}"), Some(&owner))
        .await;
    assert_hx_redirect(&response, "/collections");
    submit_drafts().await;
}

#[tokio::test]
async fn collection_member_sees_shared_entries() {
    let app = TestApp::new().await;
    let (owner_id, _owner_github_user_id) = app.create_user("Owner").await;
    let (member_id, member_github_user_id) = app.create_user("Member").await;

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
    let cookie = app.login(&member_github_user_id).await;
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Shared Entry"));
}

#[tokio::test]
async fn leaving_collection_hides_shared_entries() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (member_id, member_github_user_id) = app.create_user("Member").await;

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

    let cookie = app.login(&member_github_user_id).await;

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
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    // Create entry with tags
    let body = "url=https%3A%2F%2Fexample.com&title=Tagged&description=&duration=3&interval=days&tags=rust%2C+web&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    // Find the entry
    let (entry_id,): (String,) = sqlx::query_as("SELECT id FROM entries WHERE user_id = ?")
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

#[tokio::test]
async fn waiting_shows_only_not_yet_due_entries() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    .bind("Ready Entry")
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
    .bind("Waiting Entry")
    .bind(3)
    .bind("days")
    .bind(&now_str)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&app.db)
    .await
    .unwrap();

    // /waiting should show only the not-yet-due entry
    let resp = app.get("/waiting", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Waiting Entry"));
    assert!(!html.contains("Ready Entry"));
}

#[tokio::test]
async fn unseen_shows_only_unvisited_entries() {
    let app = TestApp::new().await;
    let (user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let now = chrono::Utc::now().to_rfc3339();

    // Entry with no visits (unseen)
    let e1 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&e1)
    .bind(&user_id)
    .bind("https://example.com/1")
    .bind("Unseen Entry")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Entry with a visit (seen)
    let e2 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&e2)
    .bind(&user_id)
    .bind("https://example.com/2")
    .bind("Seen Entry")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Add a visit for the seen entry
    let visit_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO visits (id, entry_id, user_id, visited_at) VALUES (?, ?, ?, ?)")
        .bind(&visit_id)
        .bind(&e2)
        .bind(&user_id)
        .bind(&now)
        .execute(&app.db)
        .await
        .unwrap();

    // /unseen should show only the unvisited entry
    let resp = app.get("/unseen", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Unseen Entry"));
    assert!(!html.contains("Seen Entry"));
}

#[tokio::test]
async fn create_entry_with_long_description_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let long_desc = "a".repeat(5001);
    let body = format!(
        "url=https%3A%2F%2Fexample.com&title=Test&description={}&duration=3&interval=days&tags=&collection_id=",
        long_desc
    );
    let resp = app.post_form("/entries", &body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Description must be under 5000 characters"));
}
