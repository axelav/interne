# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build                              # compile
cargo run                                # start server on :3000
cargo test                               # run test suite
cargo run -- create-user "Name"          # create user, prints invite code
cargo run -- import data.json <user-id>  # import legacy JSON entries
```

## Architecture

Rust web app: Axum server, SQLite (sqlx), Askama templates, htmx for interactivity.

### Request flow

1. `main.rs` builds the Axum router, merging sub-routers from each `routes/*.rs` module
2. `AuthUser` extractor (in `auth.rs`) checks session for user — redirects to `/login` if missing
3. Route handlers return `Result<impl IntoResponse, AppError>` — use `?` for DB/template/session errors
4. Askama templates in `templates/` render HTML; htmx attributes handle partial page updates

### Key patterns

- **`AppError`** (`error.rs`): Central error type with `From` impls for sqlx, askama, and tower-sessions errors. All route handlers should return `Result<_, AppError>` and propagate with `?`.
- **`Interval` enum** (`models/entry.rs`): Type-safe interval values (hours/days/weeks/months/years) with serde + sqlx derives. Used in `Entry`, `EntryForm`, and `calculate_availability()`. Never store intervals as raw strings.
- **Entry availability**: `calculate_availability()` in `routes/entries.rs` computes whether an entry is due based on `dismissed_at` + duration/interval. Four views: `/` (ready), `/waiting` (not yet due), `/unseen` (zero visits), `/all` (everything). Filter bar uses htmx to swap content without full page reload.
- **URL normalization**: `normalize_url()` in `routes/entries.rs` uses the `url` crate. Prepends `https://` if no scheme, rejects non-http schemes, requires host with a dot. Bare domains like `yahoo.com` are accepted.
- **Form validation**: Client-side JS validation on blur/submit (`novalidate` on form) plus server-side `validate_entry_form()` / `validate_collection_form()`. Description has a 500-char limit with live counter.
- **htmx responses**: State-changing actions (visit, delete) return either an HTML fragment for swap or an `HX-Redirect` header. The "Mark Read" button and entry title link both POST to `/entries/{id}/visit`.
- **Templates**: Askama with Jinja2 syntax. `base.html` defines blocks (`title`, `header_left`, `header_actions`, `content`). Each template struct needs `user: Option<User>` and `static_hash: &'static str` fields.
- **Cache busting**: `build.rs` hashes all files in `static/` at compile time → `STATIC_HASH` env var → appended as `?v=XXXXXXXX` to asset URLs.
- **Tags**: Entries support comma-separated tags. Tag cloud at `/tags` uses logarithmic scaling for font size and color. Per-tag views at `/tags/{name}`.
- **IDs**: All primary keys are UUID v4 strings generated in Rust (`uuid::Uuid::new_v4().to_string()`).
- **Timestamps**: All timestamps are RFC3339 strings from `chrono::Utc::now().to_rfc3339()`. Never rely on SQLite's `datetime('now')` default.

### Database

Single SQLite file at `data/interne.db`. Migrations in `migrations/` run automatically on startup via `sqlx::migrate!()`. SQL queries are inline strings (not compile-time checked) — be careful with column names.

### Auth

Invite-code based. No passwords. Session stored in SQLite via tower-sessions. `AuthUser` extractor reads `user_id` from session and does a per-request `SELECT` to load the user. Session has 30-day inactivity expiry. Cookie flags: `HttpOnly`, `SameSite::Lax`, `Secure` controlled by `SECURE_COOKIES` env var (default `true`, set `false` for local HTTP dev).

### Testing

Integration tests in `tests/` use `TestApp` helper (`tests/common/mod.rs`) which spins up an in-memory SQLite database with a real Axum router. Helpers: `create_user()`, `login()`, `get()`, `post_form()`, `delete()`, `body_string()`, `assert_redirect()`, `assert_hx_redirect()`. Unit tests for availability calculation, time formatting, and form validation are inline in the source files.
