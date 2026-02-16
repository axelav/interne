# Testing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add unit and integration tests covering all critical paths — availability logic, validation, auth, CRUD, permissions, and visibility.

**Architecture:** Unit tests inline in source files (`#[cfg(test)]`), integration tests in `tests/`. Extract `lib.rs` + `build_app()` so integration tests can construct the app. Use `tower::ServiceExt::oneshot()` for HTTP-level tests with an in-memory SQLite backend.

**Tech Stack:** Rust, axum 0.8, sqlx (SQLite), tower (oneshot), http-body-util

---

## Phase 1: Refactoring for Testability

### Task 1: Add dev-dependencies

**Files:**
- Modify: `Cargo.toml`

**Step 1: Add dev-dependencies section**

Add this block at the end of `Cargo.toml`:

```toml
[dev-dependencies]
tower = { version = "0.5", features = ["util"] }
http-body-util = "0.1"
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: compiles with same warning as before (`visit_count` never read)

**Step 3: Commit**

```
git add Cargo.toml
git commit -m "chore: add tower and http-body-util dev-dependencies for testing"
```

---

### Task 2: Extract lib.rs and build_app()

Move module declarations and app-building logic from `main.rs` into `lib.rs` so integration tests can import them.

**Files:**
- Create: `src/lib.rs`
- Modify: `src/main.rs`

**Step 1: Create src/lib.rs**

```rust
pub mod auth;
pub mod cli;
pub mod db;
pub mod error;
pub mod models;
pub mod routes;

use axum::{routing::get, Router};
use sqlx::SqlitePool;
use time::Duration;
use tower_http::services::ServeDir;
use tower_sessions::{cookie::SameSite, Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

async fn health() -> &'static str {
    "ok"
}

/// Build the full Axum application router.
///
/// Caller is responsible for running database migrations on `pool` beforehand.
/// This function sets up the session store (and migrates its table), then
/// assembles all route modules, middleware, and state.
pub async fn build_app(pool: SqlitePool, secure_cookies: bool) -> Router {
    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("Failed to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)))
        .with_secure(secure_cookies)
        .with_http_only(true)
        .with_same_site(SameSite::Lax);

    let state = AppState { db: pool };

    Router::new()
        .route("/health", get(health))
        .merge(routes::auth::router())
        .merge(routes::entries::router())
        .merge(routes::collections::router())
        .merge(routes::export::router())
        .nest_service("/static", ServeDir::new("static"))
        .layer(session_layer)
        .with_state(state)
}
```

**Step 2: Slim down src/main.rs**

Replace the entire file with:

```rust
use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();

    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data/interne.db".to_string());

    let pool = interne::db::init_pool(&database_url).await;

    // Handle CLI commands
    if args.len() > 1 {
        match args[1].as_str() {
            "import" => {
                if args.len() < 4 {
                    eprintln!("Usage: interne import <file.json> <user_id>");
                    std::process::exit(1);
                }
                if let Err(e) = interne::cli::import_data(&pool, &args[2], &args[3]).await {
                    eprintln!("Import failed: {}", e);
                    std::process::exit(1);
                }
                return;
            }
            "create-user" => {
                if args.len() < 3 {
                    eprintln!("Usage: interne create-user <name> [email]");
                    std::process::exit(1);
                }
                let email = args.get(3).map(|s| s.as_str());
                if let Err(e) = interne::cli::create_user(&pool, &args[2], email).await {
                    eprintln!("Failed to create user: {}", e);
                    std::process::exit(1);
                }
                return;
            }
            "help" | "--help" | "-h" => {
                println!("Interne - Spaced repetition for websites");
                println!();
                println!("Usage: interne [command]");
                println!();
                println!("Commands:");
                println!("  (none)              Start the web server");
                println!("  create-user <name>  Create a new user");
                println!("  import <file> <id>  Import legacy JSON data");
                println!("  help                Show this help");
                return;
            }
            cmd => {
                eprintln!("Unknown command: {}", cmd);
                eprintln!("Run 'interne help' for usage");
                std::process::exit(1);
            }
        }
    }

    // Start web server
    let secure =
        env::var("SECURE_COOKIES").unwrap_or_else(|_| "true".to_string()) == "true";

    let app = interne::build_app(pool, secure).await;

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
```

**Step 3: Verify it compiles and runs**

Run: `cargo build`
Expected: compiles successfully (same warning as before)

**Step 4: Commit**

```
git add src/lib.rs src/main.rs
git commit -m "refactor: extract lib.rs and build_app() for testability"
```

---

### Task 3: Refactor calculate_availability and format_last_viewed

Both functions call `Utc::now()` internally, making them non-deterministic. Add a `now` parameter so tests can control time. Also fix singular forms ("1 day" not "1 days") and add weeks to `format_last_viewed`.

**Files:**
- Modify: `src/routes/entries.rs`

**Step 1: Refactor calculate_availability**

Replace the existing `calculate_availability` function (lines 140–171) with:

```rust
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
```

**Step 2: Refactor format_last_viewed**

Replace the existing `format_last_viewed` function (lines 173–190) with:

```rust
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
```

**Step 3: Update all callers to pass `Utc::now()`**

There are 4 call sites in the same file. Update each:

In `list_entries` (~line 220), add `let now = Utc::now();` before the iterator and change:
```rust
let (is_available, available_in) = calculate_availability(&entry);
```
to:
```rust
let (is_available, available_in) = calculate_availability(&entry, now);
```
and:
```rust
last_viewed: format_last_viewed(&entry.dismissed_at),
```
to:
```rust
last_viewed: format_last_viewed(&entry.dismissed_at, now),
```

Same changes in `list_all_entries` (~line 258).

In `visit_entry` (~line 334), add `let now_dt = Utc::now();` and change:
```rust
let (is_available, available_in) = calculate_availability(&entry);
```
to:
```rust
let (is_available, available_in) = calculate_availability(&entry, now_dt);
```
and:
```rust
last_viewed: format_last_viewed(&entry.dismissed_at),
```
to:
```rust
last_viewed: format_last_viewed(&entry.dismissed_at, now_dt),
```

Note: `visit_entry` already has a `let now = Utc::now().to_rfc3339();` string variable, so name the `DateTime` variable `now_dt` to avoid conflict.

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: compiles successfully

**Step 5: Commit**

```
git add src/routes/entries.rs
git commit -m "refactor: make calculate_availability and format_last_viewed deterministic, fix singular forms"
```

---

## Phase 2: Unit Tests

### Task 4: Unit tests for calculate_availability

**Files:**
- Modify: `src/routes/entries.rs` (append `#[cfg(test)]` module at end of file)

**Step 1: Add test module**

Append at the end of `src/routes/entries.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone, Utc};
    use crate::models::{Entry, Interval};

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
        // 30 days - 1 day = 29 days remaining
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
}
```

**Step 2: Run tests**

Run: `cargo test -- tests::availability`
Expected: all tests pass

**Step 3: Commit**

```
git add src/routes/entries.rs
git commit -m "test: add unit tests for calculate_availability"
```

---

### Task 5: Unit tests for format_last_viewed

**Files:**
- Modify: `src/routes/entries.rs` (append to existing `mod tests`)

**Step 1: Add tests to the existing test module**

Add inside the `mod tests` block (after the calculate_availability tests):

```rust
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
        let dismissed = Some((now - Duration::weeks(1)).to_rfc3339());
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
```

**Step 2: Run tests**

Run: `cargo test -- tests::last_viewed`
Expected: all tests pass

**Step 3: Commit**

```
git add src/routes/entries.rs
git commit -m "test: add unit tests for format_last_viewed"
```

---

### Task 6: Unit tests for validate_entry_form

**Files:**
- Modify: `src/routes/entries.rs` (append to existing `mod tests`)

**Step 1: Add tests**

Add inside the `mod tests` block:

```rust
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
```

**Step 2: Run tests**

Run: `cargo test -- tests::entry_form`
Expected: all tests pass

**Step 3: Commit**

```
git add src/routes/entries.rs
git commit -m "test: add unit tests for validate_entry_form"
```

---

### Task 7: Unit tests for validate_collection_form

**Files:**
- Modify: `src/routes/collections.rs` (append `#[cfg(test)]` module)

**Step 1: Add test module**

Append at the end of `src/routes/collections.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_form_valid() {
        let form = CollectionForm {
            name: "My Collection".to_string(),
        };
        assert!(validate_collection_form(&form).is_empty());
    }

    #[test]
    fn collection_form_empty_name() {
        let form = CollectionForm {
            name: "   ".to_string(),
        };
        let errors = validate_collection_form(&form);
        assert!(errors.contains_key("name"));
    }

    #[test]
    fn collection_form_name_too_long() {
        let form = CollectionForm {
            name: "a".repeat(101),
        };
        let errors = validate_collection_form(&form);
        assert!(errors.contains_key("name"));
    }
}
```

**Step 2: Run tests**

Run: `cargo test -- routes::collections::tests`
Expected: all 3 tests pass

**Step 3: Commit**

```
git add src/routes/collections.rs
git commit -m "test: add unit tests for validate_collection_form"
```

---

### Task 8: Unit tests for Interval serde

**Files:**
- Modify: `src/models/entry.rs` (append `#[cfg(test)]` module)

**Step 1: Add test module**

Append at the end of `src/models/entry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_serde_roundtrip() {
        let variants = vec![
            (Interval::Hours, "\"hours\""),
            (Interval::Days, "\"days\""),
            (Interval::Weeks, "\"weeks\""),
            (Interval::Months, "\"months\""),
            (Interval::Years, "\"years\""),
        ];
        for (variant, expected_json) in variants {
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, expected_json);
            let deserialized: Interval = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized, variant);
        }
    }

    #[test]
    fn interval_display() {
        assert_eq!(Interval::Hours.to_string(), "hours");
        assert_eq!(Interval::Days.to_string(), "days");
        assert_eq!(Interval::Weeks.to_string(), "weeks");
        assert_eq!(Interval::Months.to_string(), "months");
        assert_eq!(Interval::Years.to_string(), "years");
    }
}
```

**Step 2: Run tests**

Run: `cargo test -- models::entry::tests`
Expected: both tests pass

**Step 3: Commit**

```
git add src/models/entry.rs
git commit -m "test: add unit tests for Interval serde and display"
```

---

## Phase 3: Integration Test Infrastructure

### Task 9: Create test helpers

**Files:**
- Create: `tests/common/mod.rs`

**Step 1: Create the directory and file**

Create `tests/common/mod.rs` with:

```rust
use axum::body::Body;
use http_body_util::BodyExt;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

pub struct TestApp {
    pub router: Router,
    pub db: SqlitePool,
}

impl TestApp {
    pub async fn new() -> Self {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("Failed to create in-memory SQLite pool");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        let router = interne::build_app(pool.clone(), false).await;

        Self { router, db: pool }
    }

    /// Send a request through the app and return the response.
    pub async fn request(&self, req: Request<Body>) -> Response {
        tower::ServiceExt::oneshot(self.router.clone(), req)
            .await
            .unwrap()
    }

    /// Create a user in the database and return (user_id, invite_code).
    pub async fn create_user(&self, name: &str) -> (String, String) {
        let id = uuid::Uuid::new_v4().to_string();
        let invite_code = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO users (id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(&invite_code)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("Failed to create test user");

        (id, invite_code)
    }

    /// Log in as the given user and return the session cookie string.
    pub async fn login(&self, invite_code: &str) -> String {
        let req = Request::builder()
            .uri("/login")
            .method("POST")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(format!("invite_code={}", invite_code)))
            .unwrap();

        let resp = self.request(req).await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);

        resp.headers()
            .get("set-cookie")
            .expect("Login should set a session cookie")
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_string()
    }

    /// Send a GET request with an optional session cookie.
    pub async fn get(&self, uri: &str, cookie: Option<&str>) -> Response {
        let mut builder = Request::builder().uri(uri);
        if let Some(cookie) = cookie {
            builder = builder.header("cookie", cookie);
        }
        let req = builder.body(Body::empty()).unwrap();
        self.request(req).await
    }

    /// Send a POST form request with an optional session cookie.
    pub async fn post_form(&self, uri: &str, body: &str, cookie: Option<&str>) -> Response {
        let mut builder = Request::builder()
            .uri(uri)
            .method("POST")
            .header("content-type", "application/x-www-form-urlencoded");
        if let Some(cookie) = cookie {
            builder = builder.header("cookie", cookie);
        }
        let req = builder.body(Body::from(body.to_string())).unwrap();
        self.request(req).await
    }

    /// Send a DELETE request with an optional session cookie.
    pub async fn delete(&self, uri: &str, cookie: Option<&str>) -> Response {
        let mut builder = Request::builder().uri(uri).method("DELETE");
        if let Some(cookie) = cookie {
            builder = builder.header("cookie", cookie);
        }
        let req = builder.body(Body::empty()).unwrap();
        self.request(req).await
    }
}

/// Read the full response body as a String.
pub async fn body_string(resp: Response) -> String {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// Assert that a response is a redirect to the given location.
pub fn assert_redirect(resp: &Response, expected_location: &str) {
    assert!(
        resp.status().is_redirection(),
        "Expected redirect, got {}",
        resp.status()
    );
    let location = resp
        .headers()
        .get("location")
        .expect("Redirect should have location header")
        .to_str()
        .unwrap();
    assert_eq!(location, expected_location);
}

/// Assert that an HX-Redirect header points to the expected location.
pub fn assert_hx_redirect(resp: &Response, expected_location: &str) {
    let hx = resp
        .headers()
        .get("hx-redirect")
        .expect("Expected HX-Redirect header")
        .to_str()
        .unwrap();
    assert_eq!(hx, expected_location);
}
```

**Step 2: Verify it compiles**

Run: `cargo test --no-run`
Expected: compiles (no test files yet import this module, so no tests run)

**Step 3: Commit**

```
git add tests/common/mod.rs
git commit -m "test: add integration test helpers (TestApp, request builders, assertions)"
```

---

## Phase 4: Integration Tests

### Task 10: Auth integration tests

**Files:**
- Create: `tests/auth.rs`

**Step 1: Create tests/auth.rs**

```rust
mod common;

use axum::http::StatusCode;
use common::{assert_redirect, body_string, TestApp};

#[tokio::test]
async fn login_with_valid_invite_code() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;

    let resp = app
        .post_form("/login", &format!("invite_code={}", invite_code), None)
        .await;

    assert_redirect(&resp, "/");
    assert!(resp.headers().get("set-cookie").is_some());
}

#[tokio::test]
async fn login_with_invalid_invite_code() {
    let app = TestApp::new().await;

    let resp = app
        .post_form("/login", "invite_code=bad-code", None)
        .await;

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_string(resp).await;
    assert!(body.contains("Invalid invite code"));
}

#[tokio::test]
async fn logout_clears_session() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let resp = app.post_form("/logout", "", Some(&cookie)).await;
    assert_redirect(&resp, "/login");

    // After logout, accessing / should redirect to login
    let resp = app.get("/", Some(&cookie)).await;
    assert_redirect(&resp, "/login");
}

#[tokio::test]
async fn unauthenticated_index_redirects_to_login() {
    let app = TestApp::new().await;
    let resp = app.get("/", None).await;
    assert_redirect(&resp, "/login");
}

#[tokio::test]
async fn unauthenticated_new_entry_redirects_to_login() {
    let app = TestApp::new().await;
    let resp = app.get("/entries/new", None).await;
    assert_redirect(&resp, "/login");
}
```

**Step 2: Run tests**

Run: `cargo test --test auth`
Expected: all 5 tests pass

**Step 3: Commit**

```
git add tests/auth.rs
git commit -m "test: add auth integration tests"
```

---

### Task 11: Entry CRUD integration tests

**Files:**
- Create: `tests/entries.rs`

**Step 1: Create tests/entries.rs**

```rust
mod common;

use axum::http::StatusCode;
use common::{assert_hx_redirect, assert_redirect, body_string, TestApp};

#[tokio::test]
async fn create_entry_with_valid_form() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Test+Entry&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    // Verify entry appears on home page
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Test Entry"));
}

#[tokio::test]
async fn create_entry_with_empty_title_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Title is required"));
}

#[tokio::test]
async fn create_entry_with_bad_url_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=ftp%3A%2F%2Fexample.com&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("URL must start with http"));
}

#[tokio::test]
async fn create_entry_with_zero_duration_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Test&description=&duration=0&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Duration must be at least 1"));
}

#[tokio::test]
async fn edit_entry_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create entry directly in DB
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Original Title")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // GET edit form
    let resp = app
        .get(&format!("/entries/{}/edit", entry_id), Some(&cookie))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Original Title"));

    // POST update
    let body = format!(
        "url=https%3A%2F%2Fexample.com&title=Updated+Title&description=&duration=5&interval=weeks&tags=&collection_id="
    );
    let resp = app
        .post_form(&format!("/entries/{}", entry_id), &body, Some(&cookie))
        .await;
    assert_redirect(&resp, "/");
}

#[tokio::test]
async fn edit_entry_as_non_owner_redirects() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (_, other_invite) = app.create_user("Other").await;
    let cookie = app.login(&other_invite).await;

    // Create entry owned by someone else
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind("https://example.com")
    .bind("Not Yours")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Other user tries to edit
    let resp = app
        .get(&format!("/entries/{}/edit", entry_id), Some(&cookie))
        .await;
    assert_redirect(&resp, "/");
}

#[tokio::test]
async fn delete_entry_as_owner() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Delete Me")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .delete(&format!("/entries/{}", entry_id), Some(&cookie))
        .await;
    assert_hx_redirect(&resp, "/");

    // Verify entry is gone
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entries WHERE id = ?")
        .bind(&entry_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count.0, 0);
}

#[tokio::test]
async fn delete_entry_as_non_owner() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (_, other_invite) = app.create_user("Other").await;
    let cookie = app.login(&other_invite).await;

    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind("https://example.com")
    .bind("Not Yours")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let resp = app
        .delete(&format!("/entries/{}", entry_id), Some(&cookie))
        .await;
    // Should redirect without deleting
    assert_hx_redirect(&resp, "/");

    // Entry still exists
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entries WHERE id = ?")
        .bind(&entry_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count.0, 1);
}
```

**Step 2: Run tests**

Run: `cargo test --test entries`
Expected: all tests pass

**Step 3: Commit**

```
git add tests/entries.rs
git commit -m "test: add entry CRUD integration tests"
```

---

### Task 12: Entry visit, availability, visibility, and tag tests

Add more tests to `tests/entries.rs` for the remaining entry flows.

**Files:**
- Modify: `tests/entries.rs`

**Step 1: Add visit and availability tests**

Append to `tests/entries.rs`:

```rust
#[tokio::test]
async fn visit_entry_updates_availability() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create an available entry (never dismissed)
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Visit Me")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Entry should appear on home page (available)
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Visit Me"));

    // Visit the entry
    let resp = app
        .post_form(&format!("/entries/{}/visit", entry_id), "", Some(&cookie))
        .await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify visit record was created
    let visit_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM visits WHERE entry_id = ?")
        .bind(&entry_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(visit_count.0, 1);

    // Verify dismissed_at was set
    let entry: (Option<String>,) =
        sqlx::query_as("SELECT dismissed_at FROM entries WHERE id = ?")
            .bind(&entry_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert!(entry.0.is_some());

    // Entry should NOT appear on home page (no longer available)
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(!html.contains("Visit Me"));

    // But should appear on /all
    let resp = app.get("/all", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Visit Me"));
}

#[tokio::test]
async fn home_shows_only_available_entries() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Available entry (dismissed long ago)
    let e1 = uuid::Uuid::new_v4().to_string();
    let old = (now - chrono::Duration::days(30)).to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, dismissed_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&e1)
    .bind(&user_id)
    .bind("https://example.com/1")
    .bind("Available Entry")
    .bind(3)
    .bind("days")
    .bind(&old)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&app.db)
    .await
    .unwrap();

    // Not-yet-due entry (dismissed just now)
    let e2 = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, dismissed_at, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&e2)
    .bind(&user_id)
    .bind("https://example.com/2")
    .bind("Not Yet Due")
    .bind(3)
    .bind("days")
    .bind(&now_str)
    .bind(&now_str)
    .bind(&now_str)
    .execute(&app.db)
    .await
    .unwrap();

    // Home page should show only available
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Available Entry"));
    assert!(!html.contains("Not Yet Due"));

    // /all should show both
    let resp = app.get("/all", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Available Entry"));
    assert!(html.contains("Not Yet Due"));
}

#[tokio::test]
async fn collection_member_sees_shared_entries() {
    let app = TestApp::new().await;
    let (owner_id, owner_invite) = app.create_user("Owner").await;
    let (member_id, member_invite) = app.create_user("Member").await;

    // Create collection
    let collection_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&owner_id)
    .bind("Shared")
    .bind("col-invite")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Add member
    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Create entry in collection (owned by owner)
    let entry_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, collection_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind(&collection_id)
    .bind("https://example.com")
    .bind("Shared Entry")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    // Member should see the entry
    let cookie = app.login(&member_invite).await;
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(html.contains("Shared Entry"));
}

#[tokio::test]
async fn leaving_collection_hides_shared_entries() {
    let app = TestApp::new().await;
    let (owner_id, _) = app.create_user("Owner").await;
    let (member_id, member_invite) = app.create_user("Member").await;

    let collection_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&owner_id)
    .bind("Shared")
    .bind("col-invite-2")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)",
    )
    .bind(&collection_id)
    .bind(&member_id)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let entry_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO entries (id, user_id, collection_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&owner_id)
    .bind(&collection_id)
    .bind("https://example.com")
    .bind("Shared Entry")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let cookie = app.login(&member_invite).await;

    // Member leaves collection
    let resp = app
        .post_form(
            &format!("/collections/{}/leave", collection_id),
            "",
            Some(&cookie),
        )
        .await;
    assert_redirect(&resp, "/collections");

    // Entry should no longer appear
    let resp = app.get("/", Some(&cookie)).await;
    let html = body_string(resp).await;
    assert!(!html.contains("Shared Entry"));
}

#[tokio::test]
async fn create_entry_with_tags() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Tagged+Entry&description=&duration=3&interval=days&tags=rust%2C+web&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    // Verify tags were created
    let tags: Vec<(String,)> = sqlx::query_as("SELECT name FROM tags ORDER BY name")
        .fetch_all(&app.db)
        .await
        .unwrap();
    let tag_names: Vec<&str> = tags.iter().map(|(n,)| n.as_str()).collect();
    assert!(tag_names.contains(&"rust"));
    assert!(tag_names.contains(&"web"));

    // Verify entry_tags links
    let links: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM entry_tags")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(links.0, 2);
}

#[tokio::test]
async fn update_entry_replaces_tags() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create entry with tags
    let body = "url=https%3A%2F%2Fexample.com&title=Tagged&description=&duration=3&interval=days&tags=rust%2C+web&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    // Find the entry
    let (entry_id,): (String,) =
        sqlx::query_as("SELECT id FROM entries WHERE user_id = ?")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();

    // Update with different tags
    let body = "url=https%3A%2F%2Fexample.com&title=Tagged&description=&duration=3&interval=days&tags=python%2C+api&collection_id=";
    let resp = app
        .post_form(&format!("/entries/{}", entry_id), body, Some(&cookie))
        .await;
    assert_redirect(&resp, "/");

    // Verify old tags unlinked, new tags linked
    let links: Vec<(String,)> = sqlx::query_as(
        "SELECT t.name FROM tags t JOIN entry_tags et ON et.tag_id = t.id WHERE et.entry_id = ? ORDER BY t.name",
    )
    .bind(&entry_id)
    .fetch_all(&app.db)
    .await
    .unwrap();
    let linked: Vec<&str> = links.iter().map(|(n,)| n.as_str()).collect();
    assert_eq!(linked, vec!["api", "python"]);
}
```

**Step 2: Run tests**

Run: `cargo test --test entries`
Expected: all tests pass

**Step 3: Commit**

```
git add tests/entries.rs
git commit -m "test: add entry visit, availability, visibility, and tag integration tests"
```

---

### Task 13: Collection integration tests

**Files:**
- Create: `tests/collections.rs`

**Step 1: Create tests/collections.rs**

```rust
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
```

**Step 2: Run tests**

Run: `cargo test --test collections`
Expected: all tests pass

**Step 3: Commit**

```
git add tests/collections.rs
git commit -m "test: add collection CRUD, permissions, and membership integration tests"
```

---

### Task 14: Export integration test

**Files:**
- Create: `tests/export.rs`

**Step 1: Create tests/export.rs**

```rust
mod common;

use axum::http::StatusCode;
use common::{body_string, TestApp};

#[tokio::test]
async fn export_returns_json_with_entries() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // Create an entry with tags
    let entry_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO entries (id, user_id, url, title, duration, interval, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&entry_id)
    .bind(&user_id)
    .bind("https://example.com")
    .bind("Export Test")
    .bind(3)
    .bind("days")
    .bind(&now)
    .bind(&now)
    .execute(&app.db)
    .await
    .unwrap();

    let tag_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
        .bind(&tag_id)
        .bind("rust")
        .bind(&now)
        .execute(&app.db)
        .await
        .unwrap();

    sqlx::query("INSERT INTO entry_tags (entry_id, tag_id) VALUES (?, ?)")
        .bind(&entry_id)
        .bind(&tag_id)
        .execute(&app.db)
        .await
        .unwrap();

    let resp = app.get("/export", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // Check Content-Disposition header
    let content_disposition = resp
        .headers()
        .get("content-disposition")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_disposition.starts_with("attachment; filename="));
    assert!(content_disposition.contains("interne-export-"));

    // Check JSON content
    let body = body_string(resp).await;
    let json: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert!(json["exported_at"].is_string());
    assert_eq!(json["entries"].as_array().unwrap().len(), 1);
    assert_eq!(json["entries"][0]["title"], "Export Test");
    assert_eq!(json["entries"][0]["tags"][0], "rust");
}

#[tokio::test]
async fn export_unauthenticated_redirects() {
    let app = TestApp::new().await;
    let resp = app.get("/export", None).await;
    common::assert_redirect(&resp, "/login");
}
```

**Step 2: Run tests**

Run: `cargo test --test export`
Expected: both tests pass

**Step 3: Commit**

```
git add tests/export.rs
git commit -m "test: add export integration tests"
```

---

## Final Verification

### Task 15: Run full test suite

**Step 1: Run all tests**

Run: `cargo test`
Expected: all ~60 tests pass, 0 failures

**Step 2: Verify build is clean**

Run: `cargo build`
Expected: compiles with no new warnings
