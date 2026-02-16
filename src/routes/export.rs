use axum::{
    extract::State,
    http::{header, HeaderMap, HeaderValue},
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use sqlx::FromRow;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::models::Interval;
use crate::AppState;

#[derive(FromRow)]
#[allow(dead_code)]
struct EntryWithTags {
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
    tags: Option<String>,
}

#[derive(Serialize)]
struct ExportEntry {
    id: String,
    url: String,
    title: String,
    description: Option<String>,
    duration: i64,
    interval: Interval,
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
) -> Result<impl IntoResponse, AppError> {
    let rows: Vec<EntryWithTags> = sqlx::query_as(
        r#"
        SELECT e.*, GROUP_CONCAT(t.name) as tags
        FROM entries e
        LEFT JOIN entry_tags et ON et.entry_id = e.id
        LEFT JOIN tags t ON t.id = et.tag_id
        WHERE e.user_id = ?
        GROUP BY e.id
        ORDER BY e.created_at
        "#,
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let export_entries: Vec<ExportEntry> = rows
        .into_iter()
        .map(|row| {
            let tags = row
                .tags
                .map(|t| t.split(',').map(|s| s.to_string()).collect())
                .unwrap_or_default();
            ExportEntry {
                id: row.id,
                url: row.url,
                title: row.title,
                description: row.description,
                duration: row.duration,
                interval: row.interval,
                dismissed_at: row.dismissed_at,
                created_at: row.created_at,
                updated_at: row.updated_at,
                tags,
            }
        })
        .collect();

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
        HeaderValue::from_str(&content_disposition).expect("date format produces valid header chars"),
    );

    Ok((headers, Json(export)))
}
