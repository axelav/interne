use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;

use crate::auth::AuthUser;
use crate::models::Entry;
use crate::AppState;

#[derive(Serialize)]
struct ExportEntry {
    id: String,
    url: String,
    title: String,
    description: Option<String>,
    duration: i64,
    interval: String,
    dismissed_at: Option<String>,
    created_at: String,
    updated_at: String,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct ExportData {
    exported_at: String,
    entries: Vec<ExportEntry>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/export", get(export_data))
}

async fn export_data(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> impl IntoResponse {
    let entries: Vec<Entry> = sqlx::query_as(
        "SELECT * FROM entries WHERE user_id = ? ORDER BY created_at"
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let mut export_entries = Vec::new();

    for entry in entries {
        let tags: Vec<(String,)> = sqlx::query_as(
            "SELECT t.name FROM tags t JOIN entry_tags et ON et.tag_id = t.id WHERE et.entry_id = ?"
        )
        .bind(&entry.id)
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();

        export_entries.push(ExportEntry {
            id: entry.id,
            url: entry.url,
            title: entry.title,
            description: entry.description,
            duration: entry.duration,
            interval: entry.interval,
            dismissed_at: entry.dismissed_at,
            created_at: entry.created_at,
            updated_at: entry.updated_at,
            tags: tags.into_iter().map(|(name,)| name).collect(),
        });
    }

    let export = ExportData {
        exported_at: chrono::Utc::now().to_rfc3339(),
        entries: export_entries,
    };

    let filename = format!("interne-export-{}.json", chrono::Local::now().format("%Y-%m-%d"));
    let content_disposition = format!("attachment; filename=\"{}\"", filename);

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition).unwrap(),
    );

    (headers, Json(export))
}
