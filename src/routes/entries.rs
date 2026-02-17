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
    static_hash: &'static str,
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
    static_hash: &'static str,
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

    if let Some(ref desc) = form.description {
        if desc.len() > 5000 {
            errors.insert("description".to_string(), "Description must be under 5000 characters".to_string());
        }
    }

    errors
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_entries))
        .route("/all", get(list_all_entries))
        .route("/waiting", get(list_waiting_entries))
        .route("/unseen", get(list_unseen_entries))
        .route("/entries/new", get(new_entry_form))
        .route("/entries", post(create_entry))
        .route("/entries/{id}/edit", get(edit_entry_form))
        .route("/entries/{id}", post(update_entry))
        .route("/entries/{id}", delete(delete_entry))
        .route("/entries/{id}/visit", post(visit_entry))
}

fn calculate_availability(entry: &Entry, now: DateTime<Utc>) -> (bool, Option<String>) {
    let Some(dismissed_at) = &entry.dismissed_at else {
        return (true, None);
    };

    let dismissed: DateTime<Utc> = dismissed_at.parse().unwrap_or(now);

    let duration = match entry.interval {
        Interval::Hours => Duration::hours(entry.duration),
        Interval::Days => Duration::days(entry.duration),
        Interval::Weeks => Duration::weeks(entry.duration),
        Interval::Months => Duration::days(entry.duration * 30),
        Interval::Years => Duration::days(entry.duration * 365),
    };

    let available_at = dismissed + duration;

    if now >= available_at {
        (true, None)
    } else {
        let diff = available_at - now;
        let available_in = if diff.num_days() > 0 {
            let d = diff.num_days();
            if d == 1 {
                "in 1 day".to_string()
            } else {
                format!("in {} days", d)
            }
        } else if diff.num_hours() > 0 {
            let h = diff.num_hours();
            if h == 1 {
                "in 1 hour".to_string()
            } else {
                format!("in {} hours", h)
            }
        } else {
            let m = diff.num_minutes();
            if m == 1 {
                "in 1 minute".to_string()
            } else {
                format!("in {} minutes", m)
            }
        };
        (false, Some(available_in))
    }
}

fn format_last_viewed(dismissed_at: &Option<String>, now: DateTime<Utc>) -> Option<String> {
    let dismissed_at = dismissed_at.as_ref()?;
    let dismissed: DateTime<Utc> = dismissed_at.parse().ok()?;
    let diff = now - dismissed;

    Some(if diff.num_days() > 365 {
        let y = diff.num_days() / 365;
        if y == 1 {
            "1 year ago".to_string()
        } else {
            format!("{} years ago", y)
        }
    } else if diff.num_days() > 30 {
        let m = diff.num_days() / 30;
        if m == 1 {
            "1 month ago".to_string()
        } else {
            format!("{} months ago", m)
        }
    } else if diff.num_days() > 7 {
        let w = diff.num_days() / 7;
        if w == 1 {
            "1 week ago".to_string()
        } else {
            format!("{} weeks ago", w)
        }
    } else if diff.num_days() > 0 {
        let d = diff.num_days();
        if d == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", d)
        }
    } else if diff.num_hours() > 0 {
        let h = diff.num_hours();
        if h == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", h)
        }
    } else if diff.num_minutes() > 0 {
        let m = diff.num_minutes();
        if m == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", m)
        }
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

pub fn build_entry_view(entry: Entry, visit_count: i64, now: DateTime<Utc>) -> EntryView {
    let (is_available, available_in) = calculate_availability(&entry, now);
    EntryView {
        id: entry.id,
        url: entry.url,
        title: entry.title,
        description: entry.description,
        last_viewed: format_last_viewed(&entry.dismissed_at, now),
        available_in,
        is_available,
        visit_count,
    }
}

async fn list_filtered_entries(
    db: &sqlx::SqlitePool,
    user: User,
    filter: &str,
) -> Result<Html<String>, AppError> {
    let entries = fetch_entries_for_user(db, &user.id).await;
    let now = Utc::now();

    let entry_views: Vec<EntryView> = entries
        .into_iter()
        .map(|(entry, visit_count)| build_entry_view(entry, visit_count, now))
        .filter(|ev| match filter {
            "ready" => ev.is_available,
            "waiting" => !ev.is_available,
            "unseen" => ev.visit_count == 0,
            _ => true, // "all"
        })
        .collect();

    let template = EntryListTemplate {
        entries: entry_views,
        filter: filter.to_string(),
        static_hash: crate::STATIC_HASH,
        user: Some(user),
    };
    Ok(Html(template.render()?))
}

async fn list_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "ready").await
}

async fn list_all_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "all").await
}

async fn list_waiting_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "waiting").await
}

async fn list_unseen_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "unseen").await
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

    let now_dt = Utc::now();
    let (is_available, available_in) = calculate_availability(&entry, now_dt);

    let template = EntryTemplate {
        entry: EntryView {
            id: entry.id,
            url: entry.url,
            title: entry.title,
            description: entry.description,
            last_viewed: format_last_viewed(&entry.dismissed_at, now_dt),
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
        static_hash: crate::STATIC_HASH,
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
            static_hash: crate::STATIC_HASH,
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
    // Verify user owns this entry
    let entry: Option<Entry> = sqlx::query_as(
        "SELECT * FROM entries WHERE id = ? AND user_id = ?"
    )
    .bind(&id)
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
        static_hash: crate::STATIC_HASH,
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
    // Verify user owns this entry
    let entry: Option<Entry> = sqlx::query_as(
        "SELECT * FROM entries WHERE id = ? AND user_id = ?"
    )
    .bind(&id)
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
            static_hash: crate::STATIC_HASH,
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
    // Verify user owns this entry
    let entry: Option<Entry> = sqlx::query_as(
        "SELECT * FROM entries WHERE id = ? AND user_id = ?"
    )
    .bind(&id)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn make_entry(duration: i64, interval: Interval, dismissed_at: Option<String>) -> Entry {
        Entry {
            id: "test-id".to_string(),
            user_id: "test-user".to_string(),
            collection_id: None,
            url: "https://example.com".to_string(),
            title: "Test".to_string(),
            description: None,
            duration,
            interval,
            dismissed_at,
            created_at: "2025-01-01T00:00:00+00:00".to_string(),
            updated_at: "2025-01-01T00:00:00+00:00".to_string(),
        }
    }

    // --- calculate_availability ---

    #[test]
    fn availability_never_dismissed_is_available() {
        let entry = make_entry(3, Interval::Days, None);
        let now = Utc::now();
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(available);
        assert!(remaining.is_none());
    }

    #[test]
    fn availability_just_dismissed_not_available() {
        let now = Utc::now();
        let dismissed = (now - Duration::seconds(1)).to_rfc3339();
        let entry = make_entry(3, Interval::Days, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(!available);
        assert!(remaining.is_some());
        assert!(remaining.unwrap().starts_with("in "));
    }

    #[test]
    fn availability_past_boundary_is_available() {
        let now = Utc::now();
        let dismissed = (now - Duration::days(4)).to_rfc3339();
        let entry = make_entry(3, Interval::Days, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(available);
        assert!(remaining.is_none());
    }

    #[test]
    fn availability_exactly_at_boundary_is_available() {
        let now = Utc::now();
        let dismissed = (now - Duration::days(3)).to_rfc3339();
        let entry = make_entry(3, Interval::Days, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(available);
        assert!(remaining.is_none());
    }

    #[test]
    fn availability_hours_interval() {
        let now = Utc::now();
        let dismissed = (now - Duration::hours(1)).to_rfc3339();
        let entry = make_entry(2, Interval::Hours, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(!available);
        assert_eq!(remaining.unwrap(), "in 1 hour");
    }

    #[test]
    fn availability_weeks_interval() {
        let now = Utc::now();
        let dismissed = (now - Duration::weeks(1)).to_rfc3339();
        let entry = make_entry(2, Interval::Weeks, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(!available);
        assert_eq!(remaining.unwrap(), "in 7 days");
    }

    #[test]
    fn availability_months_interval() {
        let now = Utc::now();
        let dismissed = (now - Duration::days(1)).to_rfc3339();
        let entry = make_entry(1, Interval::Months, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(!available);
        assert_eq!(remaining.unwrap(), "in 29 days");
    }

    #[test]
    fn availability_years_interval() {
        let now = Utc::now();
        let dismissed = (now - Duration::days(1)).to_rfc3339();
        let entry = make_entry(1, Interval::Years, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(!available);
        assert!(remaining.unwrap().contains("days"));
    }

    #[test]
    fn availability_singular_day() {
        let now = Utc::now();
        let dismissed = (now - Duration::days(2)).to_rfc3339();
        let entry = make_entry(3, Interval::Days, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(!available);
        assert_eq!(remaining.unwrap(), "in 1 day");
    }

    #[test]
    fn availability_plural_days() {
        let now = Utc::now();
        let dismissed = now.to_rfc3339();
        let entry = make_entry(3, Interval::Days, Some(dismissed));
        let (available, remaining) = calculate_availability(&entry, now);
        assert!(!available);
        assert_eq!(remaining.unwrap(), "in 3 days");
    }

    // --- format_last_viewed ---

    #[test]
    fn last_viewed_none_returns_none() {
        let now = Utc::now();
        assert!(format_last_viewed(&None, now).is_none());
    }

    #[test]
    fn last_viewed_just_now() {
        let now = Utc::now();
        let dismissed = Some(now.to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "just now");
    }

    #[test]
    fn last_viewed_singular_minute() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::minutes(1)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "1 minute ago");
    }

    #[test]
    fn last_viewed_plural_minutes() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::minutes(45)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "45 minutes ago");
    }

    #[test]
    fn last_viewed_singular_hour() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::hours(1)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "1 hour ago");
    }

    #[test]
    fn last_viewed_plural_hours() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::hours(5)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "5 hours ago");
    }

    #[test]
    fn last_viewed_singular_day() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::days(1)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "1 day ago");
    }

    #[test]
    fn last_viewed_plural_days() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::days(5)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "5 days ago");
    }

    #[test]
    fn last_viewed_singular_week() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::days(8)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "1 week ago");
    }

    #[test]
    fn last_viewed_plural_weeks() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::weeks(3)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "3 weeks ago");
    }

    #[test]
    fn last_viewed_singular_month() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::days(31)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "1 month ago");
    }

    #[test]
    fn last_viewed_plural_months() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::days(90)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "3 months ago");
    }

    #[test]
    fn last_viewed_singular_year() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::days(400)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "1 year ago");
    }

    #[test]
    fn last_viewed_plural_years() {
        let now = Utc::now();
        let dismissed = Some((now - Duration::days(800)).to_rfc3339());
        assert_eq!(format_last_viewed(&dismissed, now).unwrap(), "2 years ago");
    }

    // --- validate_entry_form ---

    fn make_valid_entry_form() -> EntryForm {
        EntryForm {
            url: "https://example.com".to_string(),
            title: "Test Title".to_string(),
            description: None,
            duration: 3,
            interval: Interval::Days,
            tags: None,
            collection_id: None,
        }
    }

    #[test]
    fn entry_form_valid() {
        let form = make_valid_entry_form();
        assert!(validate_entry_form(&form).is_empty());
    }

    #[test]
    fn entry_form_empty_title() {
        let mut form = make_valid_entry_form();
        form.title = "   ".to_string();
        let errors = validate_entry_form(&form);
        assert!(errors.contains_key("title"));
    }

    #[test]
    fn entry_form_title_too_long() {
        let mut form = make_valid_entry_form();
        form.title = "a".repeat(501);
        let errors = validate_entry_form(&form);
        assert!(errors.contains_key("title"));
    }

    #[test]
    fn entry_form_bad_url_scheme() {
        let mut form = make_valid_entry_form();
        form.url = "ftp://example.com".to_string();
        let errors = validate_entry_form(&form);
        assert!(errors.contains_key("url"));
    }

    #[test]
    fn entry_form_empty_url_allowed() {
        let mut form = make_valid_entry_form();
        form.url = "".to_string();
        let errors = validate_entry_form(&form);
        assert!(!errors.contains_key("url"));
    }

    #[test]
    fn entry_form_duration_zero() {
        let mut form = make_valid_entry_form();
        form.duration = 0;
        let errors = validate_entry_form(&form);
        assert!(errors.contains_key("duration"));
    }

    #[test]
    fn entry_form_negative_duration() {
        let mut form = make_valid_entry_form();
        form.duration = -1;
        let errors = validate_entry_form(&form);
        assert!(errors.contains_key("duration"));
    }

    #[test]
    fn entry_form_description_too_long() {
        let mut form = make_valid_entry_form();
        form.description = Some("a".repeat(5001));
        let errors = validate_entry_form(&form);
        assert!(errors.contains_key("description"));
    }

    #[test]
    fn entry_form_description_at_limit_ok() {
        let mut form = make_valid_entry_form();
        form.description = Some("a".repeat(5000));
        let errors = validate_entry_form(&form);
        assert!(!errors.contains_key("description"));
    }
}
