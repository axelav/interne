mod common;

use axum::http::StatusCode;
use common::{TestApp, body_string};

#[tokio::test]
async fn tags_page_requires_auth() {
    let app = TestApp::new().await;
    let resp = app.get("/tags", None).await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn tags_page_empty_state() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let resp = app.get("/tags", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("No tags yet."));
}

#[tokio::test]
async fn tags_page_shows_user_tags() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    let (_user1_id, github_user_id1) = app.create_user("User 1").await;
    let cookie1 = app.login(&github_user_id1).await;

    // User 1 creates entry with tag
    let body = "url=https%3A%2F%2Fexample.com&title=Entry1&description=&duration=3&interval=days&tags=secret&collection_id=";
    app.post_form("/entries", body, Some(&cookie1)).await;

    // User 2 should not see User 1's tags
    let (_user2_id, github_user_id2) = app.create_user("User 2").await;
    let cookie2 = app.login(&github_user_id2).await;

    let resp = app.get("/tags", Some(&cookie2)).await;
    let html = body_string(resp).await;
    assert!(!html.contains("secret"));
    assert!(html.contains("No tags yet."));
}

#[tokio::test]
async fn tag_detail_shows_entries_for_tag() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

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
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let resp = app.get("/tags/nonexistent", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("No links with this tag."));
}

#[tokio::test]
async fn tag_detail_has_back_link() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let resp = app.get("/tags/anything", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("href=\"/tags\""));
}

#[tokio::test]
async fn tag_cloud_has_inline_styles() {
    let app = TestApp::new().await;
    let (_user_id, github_user_id) = app.create_user("Test User").await;
    let cookie = app.login(&github_user_id).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Entry&description=&duration=3&interval=days&tags=styled&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    let resp = app.get("/tags", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("font-size:"));
    assert!(html.contains("hsl("));
}

#[tokio::test]
async fn tag_search_matches_literal_prefixes_in_the_users_own_vocabulary() {
    let app = TestApp::new().await;
    let (_, owner) = app.create_user("Owner").await;
    let owner_cookie = app.login(&owner).await;
    let (_, other) = app.create_user("Other").await;
    let other_cookie = app.login(&other).await;

    let owner_entry =
        "url=https%3A%2F%2Fexample.com&title=Mine&duration=1&interval=weeks&tags=r_own%2Crust";
    let other_entry =
        "url=https%3A%2F%2Fexample.net&title=Theirs&duration=1&interval=weeks&tags=r_other";
    for (body, cookie) in [(owner_entry, &owner_cookie), (other_entry, &other_cookie)] {
        let resp = app.post_form("/entries", body, Some(cookie)).await;
        common::assert_redirect(&resp, "/");
    }

    let resp = app
        .get("/api/tags/search?q=+R_+", Some(&owner_cookie))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains(">r_own<"));
    assert!(!html.contains(">rust<"));
    assert!(!html.contains(">r_other<"));
}

#[tokio::test]
async fn tag_search_only_offers_names_that_the_form_can_save() {
    let app = TestApp::new().await;
    let (_, user) = app.create_user("Tag Owner").await;
    let cookie = app.login(&user).await;
    let body = "url=https%3A%2F%2Fexample.com&title=Link&duration=1&interval=weeks&tags=research";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    common::assert_redirect(&resp, "/");

    // A legacy import can store names the single-line, comma-separated field cannot.
    sqlx::query("INSERT INTO tags (id, name) VALUES ('imported-comma', 'research, methods'), ('imported-linebreak', 'research' || char(10) || 'methods')")
        .execute(&app.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_tags (entry_id, tag_id) SELECT id, 'imported-comma' FROM entries UNION ALL SELECT id, 'imported-linebreak' FROM entries")
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app.get("/api/tags/search?q=research", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains(">research<"));
    assert!(!html.contains("methods"));
}

#[tokio::test]
async fn tag_search_requires_a_full_session() {
    let app = TestApp::new().await;
    let (_, invite_code) = app.create_legacy_user("Legacy User").await;
    let migration_cookie = app.legacy_login(&invite_code).await;

    for cookie in [None, Some(migration_cookie.as_str())] {
        let resp = app.get("/api/tags/search?q=r", cookie).await;
        common::assert_redirect(&resp, "/login");
    }
}

#[tokio::test]
async fn tag_search_returns_no_suggestions_for_empty_or_unmatched_queries() {
    let app = TestApp::new().await;
    let (_, user) = app.create_user("Tag Owner").await;
    let cookie = app.login(&user).await;
    let body = "url=https%3A%2F%2Fexample.com&title=Link&duration=1&interval=weeks&tags=rust";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    common::assert_redirect(&resp, "/");

    for path in [
        "/api/tags/search",
        "/api/tags/search?q=+",
        "/api/tags/search?q=zzz",
    ] {
        let resp = app.get(path, Some(&cookie)).await;
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(body_string(resp).await.trim().is_empty());
    }
}
