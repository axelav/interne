use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;

pub async fn init_pool(database_url: &str) -> SqlitePool {
    // Ensure data directory exists
    if let Some(path) = database_url.strip_prefix("sqlite:") {
        if let Some(parent) = Path::new(path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .expect("Failed to connect to database");

    // Run migrations
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("Failed to run migrations");

    pool
}
