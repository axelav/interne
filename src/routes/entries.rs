use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Duration, Utc};
use sqlx::FromRow;

use crate::auth::AuthUser;
use crate::models::{Entry, User, Visit};
use crate::AppState;

#[derive(Template)]
#[template(path = "entries/list.html")]
struct EntryListTemplate {
    entries: Vec<EntryView>,
    filter: String,
    current_date: String,
    user_name: String,
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
    interval: String,
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

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_entries))
        .route("/all", get(list_all_entries))
        .route("/entries/{id}/visit", post(visit_entry))
}

fn calculate_availability(entry: &Entry) -> (bool, Option<String>) {
    let Some(dismissed_at) = &entry.dismissed_at else {
        return (true, None);
    };

    let dismissed: DateTime<Utc> = dismissed_at.parse().unwrap_or_else(|_| Utc::now());

    let duration = match entry.interval.as_str() {
        "hours" => Duration::hours(entry.duration),
        "days" => Duration::days(entry.duration),
        "weeks" => Duration::weeks(entry.duration),
        "months" => Duration::days(entry.duration * 30),
        "years" => Duration::days(entry.duration * 365),
        _ => Duration::days(entry.duration),
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

    Some(if diff.num_days() > 30 {
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
) -> impl IntoResponse {
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
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user_name: user.name.clone(),
        user: Some(user),
    };
    Html(template.render().unwrap())
}

async fn list_all_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> impl IntoResponse {
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
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user_name: user.name.clone(),
        user: Some(user),
    };
    Html(template.render().unwrap())
}

async fn visit_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
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
    .await
    .unwrap();

    let Some(mut entry) = entry else {
        return Html("Entry not found".to_string()).into_response();
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
    .await
    .unwrap();

    // Update entry dismissed_at
    sqlx::query("UPDATE entries SET dismissed_at = ?, updated_at = ? WHERE id = ?")
        .bind(&now)
        .bind(&now)
        .bind(&id)
        .execute(&state.db)
        .await
        .unwrap();

    let visit_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits WHERE entry_id = ?")
        .bind(&id)
        .fetch_one(&state.db)
        .await
        .unwrap();

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
    Html(template.render().unwrap()).into_response()
}
