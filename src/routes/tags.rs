use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::models::User;
use crate::AppState;

struct TagWithCount {
    name: String,
    count: i64,
}

struct TagCloudItem {
    name: String,
    count: i64,
    font_size: String,
    color: String,
}

#[derive(Template)]
#[template(path = "tags/list.html")]
struct TagListTemplate {
    tags: Vec<TagCloudItem>,
    static_hash: &'static str,
    user: Option<User>,
}

fn build_tag_cloud(tags: Vec<TagWithCount>) -> Vec<TagCloudItem> {
    if tags.is_empty() {
        return vec![];
    }

    let max_count = tags.iter().map(|t| t.count).max().unwrap_or(1) as f64;
    let min_count = tags.iter().map(|t| t.count).min().unwrap_or(1) as f64;

    // Size range: 0.75rem to 2.5rem
    let min_size: f64 = 0.75;
    let max_size: f64 = 2.5;

    // Cool color palette: light teal (low) to deep indigo (high)
    // HSL: hue 180 (teal) -> 260 (indigo), saturation 40-60%, lightness 70% -> 35%
    let min_hue: f64 = 180.0;
    let max_hue: f64 = 260.0;
    let min_sat: f64 = 40.0;
    let max_sat: f64 = 60.0;
    let max_light: f64 = 70.0; // light for low count
    let min_light: f64 = 35.0; // dark for high count

    tags.into_iter()
        .map(|tag| {
            // Logarithmic scaling
            let ratio = if max_count == min_count {
                0.5
            } else {
                let log_count = (tag.count as f64).ln();
                let log_min = min_count.ln();
                let log_max = max_count.ln();
                if log_max == log_min {
                    0.5
                } else {
                    (log_count - log_min) / (log_max - log_min)
                }
            };

            let font_size = min_size + ratio * (max_size - min_size);
            let hue = min_hue + ratio * (max_hue - min_hue);
            let sat = min_sat + ratio * (max_sat - min_sat);
            let light = max_light - ratio * (max_light - min_light); // inverted: high count = low lightness

            TagCloudItem {
                name: tag.name,
                count: tag.count,
                font_size: format!("{:.2}rem", font_size),
                color: format!("hsl({:.0}, {:.0}%, {:.0}%)", hue, sat, light),
            }
        })
        .collect()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tags", get(list_tags))
        .route("/tags/{name}", get(show_tag))
}

async fn list_tags(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let tags: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT t.name, COUNT(et.entry_id) as count
        FROM tags t
        JOIN entry_tags et ON et.tag_id = t.id
        JOIN entries e ON e.id = et.entry_id
        WHERE e.user_id = ?
        GROUP BY t.id
        ORDER BY t.name ASC
        "#
    )
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let tag_counts: Vec<TagWithCount> = tags
        .into_iter()
        .map(|(name, count)| TagWithCount { name, count })
        .collect();

    let cloud_items = build_tag_cloud(tag_counts);

    let template = TagListTemplate {
        tags: cloud_items,
        static_hash: crate::STATIC_HASH,
        user: Some(user),
    };
    Ok(Html(template.render()?))
}

async fn show_tag(
    State(_state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(_name): Path<String>,
) -> Result<Html<String>, AppError> {
    // Placeholder — implemented in Task 2
    todo!()
}
