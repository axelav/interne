use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    routing::{delete, get, post},
    Form, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::FromRow;
use std::collections::HashMap;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::models::{Collection, Entry, Interval, User, Visit};
use crate::AppState;

#[derive(Template)]
#[template(path = "entries/list.html")]
struct EntryListTemplate {
    entries: Vec<EntryView>,
    filter: String,

    user: Option<User>,
}

#[derive(Template)]
#[template(path = "entries/entry.html")]
struct EntryTemplate {
    entry: EntryView,
}

pub struct EntryView {
    pub id: String,
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub last_viewed: Option<String>,
    pub available_in: Option<String>,
    pub is_available: bool,
    pub visit_count: i64,
}

/// Entry with visit count for queries that join entries with visits
#[derive(FromRow)]
struct EntryWithCount {
    // Entry fields
    id: String,
    user_id: String,
    collection_id: Option<String>,
    url: String,
    title: String,
    description: Option<String>,
    duration: i64,
    interval: Interval,
    dismissed_at: Option<String>,
    created_at: String,
    updated_at: String,
    // Extra field
    visit_count: i64,
}

impl EntryWithCount {
    fn into_entry_and_count(self) -> (Entry, i64) {
        let entry = Entry {
            id: self.id,
            user_id: self.user_id,
            collection_id: self.collection_id,
            url: self.url,
            title: self.title,
            description: self.description,
            duration: self.duration,
            interval: self.interval,
            dismissed_at: self.dismissed_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
        };
        (entry, self.visit_count)
    }
}

#[derive(Template)]
#[template(path = "entries/form.html")]
struct EntryFormTemplate {
    entry: Option<Entry>,
    collections: Vec<Collection>,
    tags_string: String,
    errors: HashMap<String, String>,

    user: Option<User>,
}

#[derive(Deserialize)]
pub struct EntryForm {
    url: String,
    title: String,
    description: Option<String>,
    duration: i64,
    interval: Interval,
    tags: Option<String>,
    collection_id: Option<String>,
}

fn validate_entry_form(form: &EntryForm) -> HashMap<String, String> {
    let mut errors = HashMap::new();

    if form.duration < 1 {
        errors.insert("duration".to_string(), "Duration must be at least 1".to_string());
    }

    if !form.url.is_empty() {
        if !form.url.starts_with("http://") && !form.url.starts_with("https://") {
            errors.insert("url".to_string(), "URL must start with http:// or https://".to_string());
        }
    }

    if form.title.trim().is_empty() {
        errors.insert("title".to_string(), "Title is required".to_string());
    }

    if form.title.len() > 500 {
        errors.insert("title".to_string(), "Title must be under 500 characters".to_string());
    }

    errors
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_entries))
        .route("/all", get(list_all_entries))
        .route("/entries/new", get(new_entry_form))
        .route("/entries", post(create_entry))
        .route("/entries/{id}/edit", get(edit_entry_form))
        .route("/entries/{id}", post(update_entry))
        .route("/entries/{id}", delete(delete_entry))
        .route("/entries/{id}/visit", post(visit_entry))
}

fn calculate_availability(entry: &Entry) -> (bool, Option<String>) {
    let Some(dismissed_at) = &entry.dismissed_at else {
        return (true, None);
    };

    let dismissed: DateTime<Utc> = dismissed_at.parse().unwrap_or_else(|_| Utc::now());

    let duration = match entry.interval {
        Interval::Hours => Duration::hours(entry.duration),
        Interval::Days => Duration::days(entry.duration),
        Interval::Weeks => Duration::weeks(entry.duration),
        Interval::Months => Duration::days(entry.duration * 30),
        Interval::Years => Duration::days(entry.duration * 365),
    };

    let available_at = dismissed + duration;
    let now = Utc::now();

    if now >= available_at {
        (true, None)
    } else {
        let diff = available_at - now;
        let available_in = if diff.num_days() > 0 {
            format!("in {} days", diff.num_days())
        } else if diff.num_hours() > 0 {
            format!("in {} hours", diff.num_hours())
        } else {
            format!("in {} minutes", diff.num_minutes())
        };
        (false, Some(available_in))
    }
}

fn format_last_viewed(dismissed_at: &Option<String>) -> Option<String> {
    let dismissed_at = dismissed_at.as_ref()?;
    let dismissed: DateTime<Utc> = dismissed_at.parse().ok()?;
    let now = Utc::now();
    let diff = now - dismissed;

    Some(if diff.num_days() > 365 {
        format!("{} years ago", diff.num_days() / 365)
    } else if diff.num_days() > 30 {
        format!("{} months ago", diff.num_days() / 30)
    } else if diff.num_days() > 0 {
        format!("{} days ago", diff.num_days())
    } else if diff.num_hours() > 0 {
        format!("{} hours ago", diff.num_hours())
    } else {
        "just now".to_string()
    })
}

async fn fetch_entries_for_user(db: &sqlx::SqlitePool, user_id: &str) -> Vec<(Entry, i64)> {
    let entries: Vec<EntryWithCount> = sqlx::query_as(
        r#"
        SELECT e.*, COUNT(v.id) as visit_count
        FROM entries e
        LEFT JOIN visits v ON v.entry_id = e.id
        WHERE e.user_id = ? OR e.collection_id IN (
            SELECT collection_id FROM collection_members WHERE user_id = ?
        )
        GROUP BY e.id
        ORDER BY e.dismissed_at DESC NULLS FIRST
        "#
    )
    .bind(user_id)
    .bind(user_id)
    .fetch_all(db)
    .await
    .unwrap_or_default();

    entries.into_iter().map(|e| e.into_entry_and_count()).collect()
}

async fn list_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let entries = fetch_entries_for_user(&state.db, &user.id).await;

    let entry_views: Vec<EntryView> = entries
        .into_iter()
        .filter_map(|(entry, visit_count)| {
            let (is_available, available_in) = calculate_availability(&entry);
            if !is_available {
                return None;
            }
            Some(EntryView {
                id: entry.id,
                url: entry.url,
                title: entry.title,
                description: entry.description,
                last_viewed: format_last_viewed(&entry.dismissed_at),
                available_in,
                is_available,
                visit_count,
            })
        })
        .collect();

    let template = EntryListTemplate {
        entries: entry_views,
        filter: "available".to_string(),

        user: Some(user),
    };
    Ok(Html(template.render()?))
}

async fn list_all_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let entries = fetch_entries_for_user(&state.db, &user.id).await;

    let entry_views: Vec<EntryView> = entries
        .into_iter()
        .map(|(entry, visit_count)| {
            let (is_available, available_in) = calculate_availability(&entry);
            EntryView {
                id: entry.id,
                url: entry.url,
                title: entry.title,
                description: entry.description,
                last_viewed: format_last_viewed(&entry.dismissed_at),
                available_in,
                is_available,
                visit_count,
            }
        })
        .collect();

    let template = EntryListTemplate {
        entries: entry_views,
        filter: "all".to_string(),

        user: Some(user),
    };
    Ok(Html(template.render()?))
}

async fn visit_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Verify user has access to this entry
    let entry: Option<Entry> = sqlx::query_as(
        r#"
        SELECT * FROM entries WHERE id = ? AND (user_id = ? OR collection_id IN (
            SELECT collection_id FROM collection_members WHERE user_id = ?
        ))
        "#
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?;

    let Some(mut entry) = entry else {
        return Err(AppError::NotFound);
    };

    let now = Utc::now().to_rfc3339();

    // Create visit record
    let visit = Visit::new(id.clone(), user.id.clone());
    sqlx::query(
        "INSERT INTO visits (id, entry_id, user_id, visited_at) VALUES (?, ?, ?, ?)"
    )
    .bind(&visit.id)
    .bind(&visit.entry_id)
    .bind(&visit.user_id)
    .bind(&visit.visited_at)
    .execute(&state.db)
    .await?;

    // Update entry dismissed_at
    sqlx::query("UPDATE entries SET dismissed_at = ?, updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&now)
        .bind(&id)
        .execute(&state.db)
        .await?;

    // Update local entry for correct availability calculation
    entry.dismissed_at = Some(now);

    let visit_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits WHERE entry_id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await?;

    let (is_available, available_in) = calculate_availability(&entry);

    let template = EntryTemplate {
        entry: EntryView {
            id: entry.id,
            url: entry.url,
            title: entry.title,
            description: entry.description,
            last_viewed: format_last_viewed(&entry.dismissed_at),
            available_in,
            is_available,
            visit_count: visit_count.0,
        },
    };
    Ok(Html(template.render()?))
}

async fn new_entry_form(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let collections: Vec<Collection> = sqlx::query_as(
        r#"
        SELECT c.* FROM collections c
        LEFT JOIN collection_members cm ON cm.collection_id = c.id
        WHERE c.owner_id = ? OR cm.user_id = ?
        "#
    )
    .bind(&user.id)
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let template = EntryFormTemplate {
        entry: None,
        collections,
        tags_string: String::new(),
        errors: HashMap::new(),

        user: Some(user),
    };
    Ok(Html(template.render()?))
}

async fn create_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<EntryForm>,
) -> Result<impl IntoResponse, AppError> {
    let errors = validate_entry_form(&form);
    if !errors.is_empty() {
        let collections: Vec<Collection> = sqlx::query_as(
            r#"
            SELECT c.* FROM collections c
            LEFT JOIN collection_members cm ON cm.collection_id = c.id
            WHERE c.owner_id = ? OR cm.user_id = ?
            "#
        )
        .bind(&user.id)
        .bind(&user.id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let template = EntryFormTemplate {
            entry: None,
            collections,
            tags_string: form.tags.as_deref().unwrap_or("").to_string(),
            errors,
            user: Some(user),
        };
        return Ok(Html(template.render()?).into_response());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();

    let collection_id = form.collection_id.filter(|s| !s.is_empty());

    sqlx::query(
        r#"
        INSERT INTO entries (id, user_id, collection_id, url, title, description, duration, interval, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&collection_id)
    .bind(&form.url)
    .bind(&form.title)
    .bind(&form.description)
    .bind(form.duration)
    .bind(&form.interval)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Handle tags
    if let Some(tags) = form.tags {
        for tag_name in tags.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            // Get or create tag
            let tag_id: Option<(String,)> = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
                .bind(&tag_name)
                .fetch_optional(&state.db)
                .await?;

            let tag_id = match tag_id {
                Some((id,)) => id,
                None => {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
                        .bind(&new_id)
                        .bind(&tag_name)
                        .bind(&now)
                        .execute(&state.db)
                        .await?;
                    new_id
                }
            };

            // Link tag to entry
            sqlx::query("INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?, ?)")
                .bind(&id)
                .bind(&tag_id)
                .execute(&state.db)
                .await?;
        }
    }

    Ok(Redirect::to("/").into_response())
}

async fn edit_entry_form(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Verify user has access to this entry
    let entry: Option<Entry> = sqlx::query_as(
        r#"
        SELECT * FROM entries WHERE id = ? AND (user_id = ? OR collection_id IN (
            SELECT collection_id FROM collection_members WHERE user_id = ?
        ))
        "#
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?;

    let Some(entry) = entry else {
        return Ok(Redirect::to("/").into_response());
    };

    let collections: Vec<Collection> = sqlx::query_as(
        r#"
        SELECT c.* FROM collections c
        LEFT JOIN collection_members cm ON cm.collection_id = c.id
        WHERE c.owner_id = ? OR cm.user_id = ?
        "#
    )
    .bind(&user.id)
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let tags: Vec<(String,)> = sqlx::query_as(
        "SELECT t.name FROM tags t JOIN entry_tags et ON et.tag_id = t.id WHERE et.entry_id = ?"
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let tags_string = tags.into_iter().map(|(name,)| name).collect::<Vec<_>>().join(", ");

    let template = EntryFormTemplate {
        entry: Some(entry),
        collections,
        tags_string,
        errors: HashMap::new(),

        user: Some(user),
    };
    Ok(Html(template.render()?).into_response())
}

async fn update_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Form(form): Form<EntryForm>,
) -> Result<impl IntoResponse, AppError> {
    // Verify user has access to this entry
    let entry: Option<Entry> = sqlx::query_as(
        r#"
        SELECT * FROM entries WHERE id = ? AND (user_id = ? OR collection_id IN (
            SELECT collection_id FROM collection_members WHERE user_id = ?
        ))
        "#
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?;

    let Some(entry) = entry else {
        return Ok(Redirect::to("/").into_response());
    };

    let errors = validate_entry_form(&form);
    if !errors.is_empty() {
        let collections: Vec<Collection> = sqlx::query_as(
            r#"
            SELECT c.* FROM collections c
            LEFT JOIN collection_members cm ON cm.collection_id = c.id
            WHERE c.owner_id = ? OR cm.user_id = ?
            "#
        )
        .bind(&user.id)
        .bind(&user.id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        let template = EntryFormTemplate {
            entry: Some(entry),
            collections,
            tags_string: form.tags.as_deref().unwrap_or("").to_string(),
            errors,
            user: Some(user),
        };
        return Ok(Html(template.render()?).into_response());
    }

    let now = chrono::Utc::now().to_rfc3339();
    let collection_id = form.collection_id.filter(|s| !s.is_empty());

    sqlx::query(
        r#"
        UPDATE entries
        SET url = ?, title = ?, description = ?, duration = ?, interval = ?, collection_id = ?, updated_at = ?
        WHERE id = ?
        "#
    )
    .bind(&form.url)
    .bind(&form.title)
    .bind(&form.description)
    .bind(form.duration)
    .bind(&form.interval)
    .bind(&collection_id)
    .bind(&now)
    .bind(&id)
    .execute(&state.db)
    .await?;

    // Clear existing tags and re-add
    sqlx::query("DELETE FROM entry_tags WHERE entry_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    if let Some(tags) = form.tags {
        for tag_name in tags.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            let tag_id: Option<(String,)> = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
                .bind(&tag_name)
                .fetch_optional(&state.db)
                .await?;

            let tag_id = match tag_id {
                Some((id,)) => id,
                None => {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
                        .bind(&new_id)
                        .bind(&tag_name)
                        .bind(&now)
                        .execute(&state.db)
                        .await?;
                    new_id
                }
            };

            sqlx::query("INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?, ?)")
                .bind(&id)
                .bind(&tag_id)
                .execute(&state.db)
                .await?;
        }
    }

    Ok(Redirect::to("/").into_response())
}

async fn delete_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Verify user has access to this entry
    let entry: Option<Entry> = sqlx::query_as(
        r#"
        SELECT * FROM entries WHERE id = ? AND (user_id = ? OR collection_id IN (
            SELECT collection_id FROM collection_members WHERE user_id = ?
        ))
        "#
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await?;

    if entry.is_none() {
        return Ok(([("HX-Redirect", "/")], "").into_response());
    }

    sqlx::query("DELETE FROM entries WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await?;

    // htmx expects empty response to remove element
    Ok(([("HX-Redirect", "/")], "").into_response())
}
