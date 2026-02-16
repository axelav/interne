# Critical Fixes Plan

Based on code review findings #1 and #2.

## Task 1: Define `AppError` type and convert route handlers to return `Result`

### What
Create a custom `AppError` enum that implements `IntoResponse`, then convert all route handlers from `-> impl IntoResponse` to `-> Result<impl IntoResponse, AppError>`. Replace all `.unwrap()` calls on database and template operations with `?`.

### Details

**Create `src/error.rs`:**
```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response, Html};

pub enum AppError {
    Database(sqlx::Error),
    Template(askama::Error),
    Session(tower_sessions::session::Error),
    NotFound,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found").into_response(),
            AppError::Database(e) => {
                tracing::error!("Database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
            AppError::Template(e) => {
                tracing::error!("Template error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
            AppError::Session(e) => {
                tracing::error!("Session error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self { AppError::Database(e) }
}
impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self { AppError::Template(e) }
}
impl From<tower_sessions::session::Error> for AppError {
    fn from(e: tower_sessions::session::Error) -> Self { AppError::Session(e) }
}
```

**Register in `src/main.rs`:**
```rust
mod error;
```

**Convert handlers in:**
- `src/routes/entries.rs` (~27 unwraps)
- `src/routes/collections.rs` (~15 unwraps)
- `src/routes/auth.rs` (~5 unwraps)
- `src/routes/export.rs` (~3 unwraps)

**Pattern for each handler:**
```rust
// Before:
async fn show_entry(...) -> impl IntoResponse {
    let entry = sqlx::query_as::<_, Entry>(...).fetch_optional(&state.db).await.unwrap();
    Html(template.render().unwrap())
}

// After:
async fn show_entry(...) -> Result<impl IntoResponse, AppError> {
    let entry = sqlx::query_as::<_, Entry>(...).fetch_optional(&state.db).await?;
    let entry = entry.ok_or(AppError::NotFound)?;
    Ok(Html(template.render()?))
}
```

**Notes:**
- `.unwrap_or_default()` on `fetch_all` is acceptable — keep those as-is (empty vec on error is fine for list views)
- `.unwrap_or_else()` on `parse()` with fallback is also fine to keep
- Focus on `.unwrap()` that would panic on any error
- The `HeaderValue::from_str().unwrap()` in export.rs is fine (static string, will never fail)

### Verification
- `cargo check` passes with no errors
- `cargo clippy` has no new warnings
- App starts and pages load without errors

---

## Task 2: Use the `Interval` enum throughout instead of `String`

### What
Change `Entry.interval` from `String` to `Interval`. Update `EntryForm` deserialization, the `calculate_availability()` function, and the CLI import to use the typed enum.

### Details

**Update `src/models/entry.rs`:**
- Change `Entry.interval` from `String` to `Interval`
- Remove `Entry::new()` (dead code, addressed later but removing it avoids compile errors from the type change)
- Re-export `Interval` from `src/models/mod.rs`

**Update `src/routes/entries.rs`:**
- Change `EntryForm.interval` from `String` to `Interval`
- Update `calculate_availability()` to match on `Interval` enum variants instead of `.as_str()`
- Update all SQL INSERT/UPDATE queries that pass `entry.interval` — sqlx handles `Interval` via the `sqlx::Type` derive
- Update template structs that display interval to use `Interval` (the `Display` impl handles `.to_string()`)

**Update `src/routes/export.rs`:**
- The `ExportEntry` struct's `interval` field — change to `Interval` or keep as `String` for JSON export. Since `Interval` has `Serialize`, it can stay typed.

**Update `src/cli.rs`:**
- The legacy import maps interval strings — update to parse into `Interval` enum
- Add proper error handling for invalid interval values during import

**Pattern for calculate_availability:**
```rust
// Before:
let interval_duration = match entry.interval.as_str() {
    "hours" => Duration::hours(entry.duration),
    "days" => Duration::days(entry.duration),
    "weeks" => Duration::weeks(entry.duration),
    "months" => Duration::days(entry.duration * 30),
    "years" => Duration::days(entry.duration * 365),
    _ => Duration::days(entry.duration),  // silent fallback
};

// After:
let interval_duration = match entry.interval {
    Interval::Hours => Duration::hours(entry.duration),
    Interval::Days => Duration::days(entry.duration),
    Interval::Weeks => Duration::weeks(entry.duration),
    Interval::Months => Duration::days(entry.duration * 30),
    Interval::Years => Duration::days(entry.duration * 365),
};
```

### Verification
- `cargo check` passes with no errors
- `cargo clippy` has no new warnings
- Invalid interval values in forms produce a proper 422 error (serde deserialization failure) instead of a panic

---

## Execution Order

Task 1 first (AppError), then Task 2 (Interval enum). Task 2 benefits from having proper error handling already in place.
