mod common;

use axum::http::StatusCode;
use common::{assert_hx_redirect, assert_redirect, body_string, TestApp};

// --- CRUD ---

#[tokio::test]
async fn create_collection() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let resp = app
        .post_form("/collections", "name=My+Collection", Some(&cookie))
        .await;
    assert_redirect(&resp, "/collections");

    // Verify it appears in list
    let resp = app.get("/collections", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("My Collection"));
}

#[tokio::test]
async fn create_collection_empty_name_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let resp = app
        .post_form("/collections", "name=", Some(&cookie))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Name is required"));
}

#[tokio::test]
async fn show_collection_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Owner").await;
    let cookie = app.login(&invite_code).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&user_id)
    .bind("Test Col")
    .bind("invite-123")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .get(&format!("/collections/{}", col_id), Some(&cookie))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Test Col"));
    assert!(html.contains("invite-123")); // Owner sees invite code
}

#[tokio::test]
async fn show_collection_as_non_member_redirects() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (_, outsider_invite) = app.create_user("Outsider").await;
    let cookie = app.login(&outsider_invite).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&owner_id)
    .bind("Private Col")
    .bind("invite-xyz")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .get(&format!("/collections/{}", col_id), Some(&cookie))
        .await;
    assert_redirect(&resp, "/collections");
}

#[tokio::test]
async fn update_collection_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Owner").await;
    let cookie = app.login(&invite_code).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&user_id)
    .bind("Old Name")
    .bind("invite-456")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .post_form(
            &format!("/collections/{}", col_id),
            "name=New+Name",
            Some(&cookie),
        )
        .await;
    assert_redirect(&resp, "/collections");

    // Verify name changed
    let (name,): (String,) = sqlx::query_as("SELECT name FROM collections WHERE id = ?")
        .bind(&col_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(name, "New Name");
}

#[tokio::test]
async fn update_collection_as_member_does_nothing() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (member_id, member_invite) = app.create_user("Member").await;
    let cookie = app.login(&member_invite).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&owner_id)
    .bind("Original")
    .bind("invite-789")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Member tries to update - the SQL WHERE clause requires owner_id match,
    // so this silently does nothing
    app.post_form(
        &format!("/collections/{}", col_id),
        "name=Hacked",
        Some(&cookie),
    )
    .await;

    let (name,): (String,) = sqlx::query_as("SELECT name FROM collections WHERE id = ?")
        .bind(&col_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(name, "Original");
}

#[tokio::test]
async fn delete_collection_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Owner").await;
    let cookie = app.login(&invite_code).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&user_id)
    .bind("Delete Me")
    .bind("invite-del")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .delete(&format!("/collections/{}", col_id), Some(&cookie))
        .await;
    assert_hx_redirect(&resp, "/collections");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM collections WHERE id = ?")
        .bind(&col_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn delete_collection_as_member_does_nothing() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (member_id, member_invite) = app.create_user("Member").await;
    let cookie = app.login(&member_invite).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&owner_id)
    .bind("Protected")
    .bind("invite-prot")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    app.delete(&format!("/collections/{}", col_id), Some(&cookie))
        .await;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM collections WHERE id = ?")
        .bind(&col_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}

// --- Membership ---

#[tokio::test]
async fn join_collection_via_invite_code() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (_, member_invite) = app.create_user("Member").await;
    let cookie = app.login(&member_invite).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&owner_id)
    .bind("Join Me")
    .bind("join-code-123")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .post_form(
            "/collections/join",
            "invite_code=join-code-123",
            Some(&cookie),
        )
        .await;
    assert_redirect(&resp, "/collections");

    // Verify membership
    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM collection_members WHERE collection_id = ?")
            .bind(&col_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count.0, 1);
}

#[tokio::test]
async fn regenerate_invite_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Owner").await;
    let cookie = app.login(&invite_code).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&user_id)
    .bind("Regen Test")
    .bind("old-code")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    app.post_form(
        &format!("/collections/{}/regenerate-invite", col_id),
        "",
        Some(&cookie),
    )
    .await;

    let (new_code,): (String,) =
        sqlx::query_as("SELECT invite_code FROM collections WHERE id = ?")
            .bind(&col_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_ne!(new_code, "old-code");
}

#[tokio::test]
async fn owner_removes_member() {
    let app = TestApp::new().await;
    let (owner_id, owner_invite) = app.create_user("Owner").await;
    let (member_id, _) = app.create_user("Member").await;
    let cookie = app.login(&owner_invite).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&owner_id)
    .bind("Remove Test")
    .bind("rm-code")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .delete(
            &format!("/collections/{}/members/{}", col_id, member_id),
            Some(&cookie),
        )
        .await;
    assert_hx_redirect(&resp, &format!("/collections/{}", col_id));

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM collection_members WHERE collection_id = ?")
            .bind(&col_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn member_leaves_collection() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (member_id, member_invite) = app.create_user("Member").await;
    let cookie = app.login(&member_invite).await;

    let col_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&owner_id)
    .bind("Leave Test")
    .bind("leave-code")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&col_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .post_form(
            &format!("/collections/{}/leave", col_id),
            "",
            Some(&cookie),
        )
        .await;
    assert_redirect(&resp, "/collections");

    let count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM collection_members WHERE collection_id = ?")
            .bind(&col_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(count.0, 0);
}
