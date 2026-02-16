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
use crate::error::AppError;
use crate::models::{Collection, CollectionMember, User};
use crate::AppState;

#[derive(Template)]
#[template(path = "collections/list.html")]
struct CollectionListTemplate {
    collections: Vec<CollectionView>,

    user: Option<User>,
}

#[derive(Template)]
#[template(path = "collections/form.html")]
struct CollectionFormTemplate {
    collection: Option<Collection>,
    errors: HashMap<String, String>,

    user: Option<User>,
}

#[derive(Template)]
#[template(path = "collections/show.html")]
struct CollectionShowTemplate {
    collection: Collection,
    members: Vec<User>,
    is_owner: bool,

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

fn validate_collection_form(form: &CollectionForm) -> HashMap<String, String> {
    let mut errors = HashMap::new();

    if form.name.trim().is_empty() {
        errors.insert("name".to_string(), "Name is required".to_string());
    }

    if form.name.len() > 100 {
        errors.insert("name".to_string(), "Name must be under 100 characters".to_string());
    }

    errors
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
        .route("/collections/{id}/leave", post(leave_collection))
        .route("/collections/{id}/members/{user_id}", delete(remove_member))
}

async fn list_collections(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
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

        user: Some(user),
    };
    Ok(Html(template.render()?))
}

async fn new_collection_form(AuthUser(user): AuthUser) -> Result<impl IntoResponse, AppError> {
    let template = CollectionFormTemplate {
        collection: None,
        errors: HashMap::new(),

        user: Some(user),
    };
    Ok(Html(template.render()?))
}

async fn create_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<CollectionForm>,
) -> Result<impl IntoResponse, AppError> {
    let errors = validate_collection_form(&form);
    if !errors.is_empty() {
        let template = CollectionFormTemplate {
            collection: None,
            errors,
            user: Some(user),
        };
        return Ok(Html(template.render()?).into_response());
    }

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
    .await?;

    Ok(Redirect::to("/collections").into_response())
}

async fn join_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<JoinForm>,
) -> Result<impl IntoResponse, AppError> {
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE invite_code = ?"
    )
    .bind(&form.invite_code)
    .fetch_optional(&state.db)
    .await?;

    if let Some(collection) = collection {
        if collection.owner_id == user.id {
            return Ok(Redirect::to("/collections"));
        }
        let member = CollectionMember::new(collection.id, user.id);
        sqlx::query(
            "INSERT OR IGNORE INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)"
        )
        .bind(&member.collection_id)
        .bind(&member.user_id)
        .bind(&member.joined_at)
        .execute(&state.db)
        .await?;
    }

    Ok(Redirect::to("/collections"))
}

async fn show_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
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
    .await?;

    let Some(collection) = collection else {
        return Ok(Redirect::to("/collections").into_response());
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

        user: Some(user),
    };
    Ok(Html(template.render()?).into_response())
}

async fn edit_collection_form(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE id = ? AND owner_id = ?"
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?;

    let Some(collection) = collection else {
        return Ok(Redirect::to("/collections").into_response());
    };

    let template = CollectionFormTemplate {
        collection: Some(collection),
        errors: HashMap::new(),

        user: Some(user),
    };
    Ok(Html(template.render()?).into_response())
}

async fn update_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Form(form): Form<CollectionForm>,
) -> Result<impl IntoResponse, AppError> {
    let errors = validate_collection_form(&form);
    if !errors.is_empty() {
        let collection: Option<Collection> = sqlx::query_as(
            "SELECT * FROM collections WHERE id = ? AND owner_id = ?"
        )
        .bind(&id)
        .bind(&user.id)
        .fetch_optional(&state.db)
        .await?;

        let template = CollectionFormTemplate {
            collection,
            errors,
            user: Some(user),
        };
        return Ok(Html(template.render()?).into_response());
    }

    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE collections SET name = ?, updated_at = ? WHERE id = ? AND owner_id = ?")
        .bind(&form.name)
        .bind(&now)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await?;

    Ok(Redirect::to("/collections").into_response())
}

async fn delete_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query("DELETE FROM collections WHERE id = ? AND owner_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await?;

    Ok(([("HX-Redirect", "/collections")], ""))
}

async fn regenerate_invite(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let new_code = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE collections SET invite_code = ?, updated_at = ? WHERE id = ? AND owner_id = ?")
        .bind(&new_code)
        .bind(&now)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await?;

    Ok(Redirect::to(&format!("/collections/{}", id)))
}

async fn leave_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Only members can leave (not owners)
    sqlx::query("DELETE FROM collection_members WHERE collection_id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await?;

    Ok(Redirect::to("/collections"))
}

async fn remove_member(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((collection_id, member_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    // Verify user is owner
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE id = ? AND owner_id = ?"
    )
    .bind(&collection_id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?;

    if collection.is_some() {
        sqlx::query("DELETE FROM collection_members WHERE collection_id = ? AND user_id = ?")
            .bind(&collection_id)
            .bind(&member_id)
            .execute(&state.db)
            .await?;
    }

    Ok(([("HX-Redirect", format!("/collections/{}", collection_id))], "").into_response())
}
