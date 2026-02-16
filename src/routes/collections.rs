use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    routing::{delete, get, post},
    Form, Router,
};
use serde::Deserialize;
use sqlx::FromRow;
use std::collections::HashMap;

use crate::auth::AuthUser;
use crate::models::{Collection, CollectionMember, User};
use crate::AppState;

#[derive(Template)]
#[template(path = "collections/list.html")]
struct CollectionListTemplate {
    collections: Vec<CollectionView>,
    current_date: String,
    user: Option<User>,
}

#[derive(Template)]
#[template(path = "collections/form.html")]
struct CollectionFormTemplate {
    collection: Option<Collection>,
    errors: HashMap<String, String>,
    current_date: String,
    user: Option<User>,
}

#[derive(Template)]
#[template(path = "collections/show.html")]
struct CollectionShowTemplate {
    collection: Collection,
    members: Vec<User>,
    is_owner: bool,
    current_date: String,
    user: Option<User>,
}

struct CollectionView {
    id: String,
    name: String,
    is_owner: bool,
    member_count: i64,
}

/// Collection with member count for queries that join with collection_members
#[derive(FromRow)]
struct CollectionWithCount {
    // Collection fields
    id: String,
    owner_id: String,
    name: String,
    invite_code: String,
    created_at: String,
    updated_at: String,
    // Extra field
    member_count: i64,
}

impl CollectionWithCount {
    fn into_collection_and_count(self) -> (Collection, i64) {
        let collection = Collection {
            id: self.id,
            owner_id: self.owner_id,
            name: self.name,
            invite_code: self.invite_code,
            created_at: self.created_at,
            updated_at: self.updated_at,
        };
        (collection, self.member_count)
    }
}

#[derive(Deserialize)]
pub struct CollectionForm {
    name: String,
}

#[derive(Deserialize)]
pub struct JoinForm {
    invite_code: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/collections", get(list_collections))
        .route("/collections/new", get(new_collection_form))
        .route("/collections", post(create_collection))
        .route("/collections/join", post(join_collection))
        .route("/collections/{id}", get(show_collection))
        .route("/collections/{id}/edit", get(edit_collection_form))
        .route("/collections/{id}", post(update_collection))
        .route("/collections/{id}", delete(delete_collection))
        .route("/collections/{id}/regenerate-invite", post(regenerate_invite))
        .route("/collections/{id}/members/{user_id}", delete(remove_member))
}

async fn list_collections(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> impl IntoResponse {
    let collections: Vec<CollectionWithCount> = sqlx::query_as(
        r#"
        SELECT c.*, COUNT(DISTINCT cm.user_id) + 1 as member_count
        FROM collections c
        LEFT JOIN collection_members cm ON cm.collection_id = c.id
        WHERE c.owner_id = ? OR c.id IN (SELECT collection_id FROM collection_members WHERE user_id = ?)
        GROUP BY c.id
        ORDER BY c.name
        "#
    )
    .bind(&user.id)
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let views: Vec<CollectionView> = collections
        .into_iter()
        .map(|cwc| {
            let (c, count) = cwc.into_collection_and_count();
            CollectionView {
                is_owner: c.owner_id == user.id,
                id: c.id,
                name: c.name,
                member_count: count,
            }
        })
        .collect();

    let template = CollectionListTemplate {
        collections: views,
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap())
}

async fn new_collection_form(AuthUser(user): AuthUser) -> impl IntoResponse {
    let template = CollectionFormTemplate {
        collection: None,
        errors: HashMap::new(),
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap())
}

async fn create_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<CollectionForm>,
) -> impl IntoResponse {
    let collection = Collection::new(user.id, form.name);

    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&collection.id)
    .bind(&collection.owner_id)
    .bind(&collection.name)
    .bind(&collection.invite_code)
    .bind(&collection.created_at)
    .bind(&collection.updated_at)
    .execute(&state.db)
    .await
    .unwrap();

    Redirect::to("/collections")
}

async fn join_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<JoinForm>,
) -> impl IntoResponse {
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE invite_code = ?"
    )
    .bind(&form.invite_code)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    if let Some(collection) = collection {
        let member = CollectionMember::new(collection.id, user.id);
        sqlx::query(
            "INSERT OR IGNORE INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)"
        )
        .bind(&member.collection_id)
        .bind(&member.user_id)
        .bind(&member.joined_at)
        .execute(&state.db)
        .await
        .unwrap();
    }

    Redirect::to("/collections")
}

async fn show_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Verify user has access (owner or member)
    let collection: Option<Collection> = sqlx::query_as(
        r#"
        SELECT c.* FROM collections c
        WHERE c.id = ? AND (c.owner_id = ? OR c.id IN (
            SELECT collection_id FROM collection_members WHERE user_id = ?
        ))
        "#
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    let Some(collection) = collection else {
        return Redirect::to("/collections").into_response();
    };

    let members: Vec<User> = sqlx::query_as(
        r#"
        SELECT u.* FROM users u
        JOIN collection_members cm ON cm.user_id = u.id
        WHERE cm.collection_id = ?
        "#
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let template = CollectionShowTemplate {
        is_owner: collection.owner_id == user.id,
        collection,
        members,
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap()).into_response()
}

async fn edit_collection_form(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE id = ? AND owner_id = ?"
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    let Some(collection) = collection else {
        return Redirect::to("/collections").into_response();
    };

    let template = CollectionFormTemplate {
        collection: Some(collection),
        errors: HashMap::new(),
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap()).into_response()
}

async fn update_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Form(form): Form<CollectionForm>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE collections SET name = ?, updated_at = ? WHERE id = ? AND owner_id = ?")
        .bind(&form.name)
        .bind(&now)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .unwrap();

    Redirect::to("/collections")
}

async fn delete_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    sqlx::query("DELETE FROM collections WHERE id = ? AND owner_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .unwrap();

    ([("HX-Redirect", "/collections")], "")
}

async fn regenerate_invite(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let new_code = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE collections SET invite_code = ?, updated_at = ? WHERE id = ? AND owner_id = ?")
        .bind(&new_code)
        .bind(&now)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .unwrap();

    Redirect::to(&format!("/collections/{}", id))
}

async fn remove_member(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((collection_id, member_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // Verify user is owner
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE id = ? AND owner_id = ?"
    )
    .bind(&collection_id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    if collection.is_some() {
        sqlx::query("DELETE FROM collection_members WHERE collection_id = ? AND user_id = ?")
            .bind(&collection_id)
            .bind(&member_id)
            .execute(&state.db)
            .await
            .unwrap();
    }

    Redirect::to(&format!("/collections/{}", collection_id))
}
