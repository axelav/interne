# Interne Rebuild Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a spaced repetition system for websites with multi-user support, shared collections, and a brutalist UI.

**Architecture:** Rust + Axum server, SQLite database with sqlx, Askama templates, htmx for interactivity. Server-rendered pages with partial updates for actions like "Mark Read" and "Delete".

**Tech Stack:** Rust 1.90, Axum, sqlx (SQLite), Askama, htmx, Docker

---

## Task 1: Project Scaffold

**Files:**
- Create: `Cargo.toml`
- Create: `src/main.rs`
- Create: `.env`

**Step 1: Initialize Cargo project**

Run:
```bash
cd /Users/axelav/s/interne/.worktrees/rebuild
cargo init --name interne
```

**Step 2: Add dependencies to Cargo.toml**

Replace `Cargo.toml` contents:

```toml
[package]
name = "interne"
version = "0.1.0"
edition = "2024"

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite", "uuid", "chrono"] }
askama = "0.13"
askama_axum = "0.5"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
chrono = { version = "0.4", features = ["serde"] }
tower-http = { version = "0.6", features = ["fs"] }
tower-sessions = "0.14"
tower-sessions-sqlx-store = { version = "0.15", features = ["sqlite"] }
dotenvy = "0.15"
tracing = "0.1"
tracing-subscriber = "0.3"
argon2 = "0.5"
```

**Step 3: Create minimal main.rs**

```rust
use axum::{routing::get, Router};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing_subscriber;

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() {
    tracing_subscriber::init();

    let app = Router::new().route("/health", get(health));

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
```

**Step 4: Create .env**

```
DATABASE_URL=sqlite:data/interne.db
SESSION_SECRET=dev-secret-change-in-prod
```

**Step 5: Verify it compiles and runs**

Run:
```bash
cargo build
```

Expected: Compiles with no errors (warnings ok)

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: project scaffold with axum"
```

---

## Task 2: Database Schema & Migrations

**Files:**
- Create: `migrations/001_initial.sql`
- Modify: `src/main.rs`
- Create: `src/db.rs`

**Step 1: Create migrations directory and initial schema**

Create `migrations/001_initial.sql`:

```sql
-- Users
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    invite_code TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Collections
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    invite_code TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Collection members
CREATE TABLE collection_members (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (collection_id, user_id)
);

-- Entries
CREATE TABLE entries (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    collection_id TEXT REFERENCES collections(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    duration INTEGER NOT NULL,
    interval TEXT NOT NULL CHECK (interval IN ('hours', 'days', 'weeks', 'months', 'years')),
    dismissed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Visits
CREATE TABLE visits (
    id TEXT PRIMARY KEY,
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visited_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tags
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Entry tags
CREATE TABLE entry_tags (
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (entry_id, tag_id)
);

-- Indexes
CREATE INDEX idx_entries_user_id ON entries(user_id);
CREATE INDEX idx_entries_collection_id ON entries(collection_id);
CREATE INDEX idx_entries_dismissed_at ON entries(dismissed_at);
CREATE INDEX idx_entry_tags_tag_id ON entry_tags(tag_id);
CREATE INDEX idx_visits_entry_id ON visits(entry_id);
CREATE INDEX idx_collection_members_user_id ON collection_members(user_id);
```

**Step 2: Create db.rs**

Create `src/db.rs`:

```rust
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
```

**Step 3: Update main.rs to use database**

Replace `src/main.rs`:

```rust
mod db;

use axum::{routing::get, Router};
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tokio::net::TcpListener;

async fn health() -> &'static str {
    "ok"
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::init();
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:data/interne.db".to_string());

    let pool = db::init_pool(&database_url).await;

    let state = AppState { db: pool };

    let app = Router::new()
        .route("/health", get(health))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
```

**Step 4: Verify migrations run**

Run:
```bash
cargo build
cargo run &
sleep 2
curl http://localhost:3000/health
pkill interne
```

Expected: "ok" response, `data/interne.db` created

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: database schema and migrations"
```

---

## Task 3: Models

**Files:**
- Create: `src/models/mod.rs`
- Create: `src/models/user.rs`
- Create: `src/models/entry.rs`
- Create: `src/models/collection.rs`
- Create: `src/models/tag.rs`
- Create: `src/models/visit.rs`
- Modify: `src/main.rs`

**Step 1: Create models directory structure**

Run:
```bash
mkdir -p src/models
```

**Step 2: Create src/models/mod.rs**

```rust
pub mod user;
pub mod entry;
pub mod collection;
pub mod tag;
pub mod visit;

pub use user::User;
pub use entry::Entry;
pub use collection::{Collection, CollectionMember};
pub use tag::Tag;
pub use visit::Visit;
```

**Step 3: Create src/models/user.rs**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub invite_code: String,
    pub created_at: String,
    pub updated_at: String,
}

impl User {
    pub fn new(name: String, email: Option<String>) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            email,
            invite_code: Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
```

**Step 4: Create src/models/entry.rs**

```rust
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(type_name = "TEXT")]
pub enum Interval {
    #[serde(rename = "hours")]
    #[sqlx(rename = "hours")]
    Hours,
    #[serde(rename = "days")]
    #[sqlx(rename = "days")]
    Days,
    #[serde(rename = "weeks")]
    #[sqlx(rename = "weeks")]
    Weeks,
    #[serde(rename = "months")]
    #[sqlx(rename = "months")]
    Months,
    #[serde(rename = "years")]
    #[sqlx(rename = "years")]
    Years,
}

impl std::fmt::Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Interval::Hours => write!(f, "hours"),
            Interval::Days => write!(f, "days"),
            Interval::Weeks => write!(f, "weeks"),
            Interval::Months => write!(f, "months"),
            Interval::Years => write!(f, "years"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Entry {
    pub id: String,
    pub user_id: String,
    pub collection_id: Option<String>,
    pub url: String,
    pub title: String,
    pub description: Option<String>,
    pub duration: i64,
    pub interval: String,
    pub dismissed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Entry {
    pub fn new(
        user_id: String,
        collection_id: Option<String>,
        url: String,
        title: String,
        description: Option<String>,
        duration: i64,
        interval: Interval,
    ) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            user_id,
            collection_id,
            url,
            title,
            description,
            duration,
            interval: interval.to_string(),
            dismissed_at: None,
            created_at: now.clone(),
            updated_at: now,
        }
    }
}
```

**Step 5: Create src/models/collection.rs**

```rust
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Collection {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub invite_code: String,
    pub created_at: String,
    pub updated_at: String,
}

impl Collection {
    pub fn new(owner_id: String, name: String) -> Self {
        let now = Utc::now().to_rfc3339();
        Self {
            id: Uuid::new_v4().to_string(),
            owner_id,
            name,
            invite_code: Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CollectionMember {
    pub collection_id: String,
    pub user_id: String,
    pub joined_at: String,
}

impl CollectionMember {
    pub fn new(collection_id: String, user_id: String) -> Self {
        Self {
            collection_id,
            user_id,
            joined_at: Utc::now().to_rfc3339(),
        }
    }
}
```

**Step 6: Create src/models/tag.rs**

```rust
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub created_at: String,
}

impl Tag {
    pub fn new(name: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name: name.to_lowercase().trim().to_string(),
            created_at: Utc::now().to_rfc3339(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EntryTag {
    pub entry_id: String,
    pub tag_id: String,
}
```

**Step 7: Create src/models/visit.rs**

```rust
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Visit {
    pub id: String,
    pub entry_id: String,
    pub user_id: String,
    pub visited_at: String,
}

impl Visit {
    pub fn new(entry_id: String, user_id: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            entry_id,
            user_id,
            visited_at: Utc::now().to_rfc3339(),
        }
    }
}
```

**Step 8: Update main.rs to include models**

Add after `mod db;`:

```rust
mod models;
```

**Step 9: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles successfully

**Step 10: Commit**

```bash
git add -A
git commit -m "feat: add data models"
```

---

## Task 4: Session & Auth Middleware

**Files:**
- Create: `src/auth.rs`
- Modify: `src/main.rs`

**Step 1: Create src/auth.rs**

```rust
use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::models::User;

const USER_ID_KEY: &str = "user_id";

pub struct AuthUser(pub User);

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user: Option<User> = session.get(USER_ID_KEY).await.ok().flatten();

        user.map(AuthUser).ok_or(AuthRedirect)
    }
}

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::to("/login").into_response()
    }
}

pub async fn login_user(session: &Session, user: User) -> Result<(), tower_sessions::session::Error> {
    session.insert(USER_ID_KEY, user).await
}

pub async fn logout_user(session: &Session) -> Result<(), tower_sessions::session::Error> {
    session.flush().await
}

pub async fn get_current_user(session: &Session) -> Option<User> {
    session.get::<User>(USER_ID_KEY).await.ok().flatten()
}
```

**Step 2: Update main.rs with session layer**

Replace `src/main.rs`:

```rust
mod auth;
mod db;
mod models;

use axum::{routing::get, Router};
use sqlx::SqlitePool;
use std::net::SocketAddr;
use time::Duration;
use tokio::net::TcpListener;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

async fn health() -> &'static str {
    "ok"
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::init();
    dotenvy::dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:data/interne.db".to_string());

    let pool = db::init_pool(&database_url).await;

    // Session store
    let session_store = SqliteStore::new(pool.clone());
    session_store.migrate().await.expect("Failed to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)));

    let state = AppState { db: pool };

    let app = Router::new()
        .route("/health", get(health))
        .layer(session_layer)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
```

**Step 3: Add time crate to Cargo.toml**

Add to `[dependencies]`:

```toml
time = "0.3"
```

**Step 4: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles successfully

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: session management with 30-day sliding expiration"
```

---

## Task 5: Base Template & Static Files

**Files:**
- Create: `templates/base.html`
- Create: `static/style.css`
- Create: `static/htmx.min.js`
- Modify: `src/main.rs`

**Step 1: Create templates directory**

Run:
```bash
mkdir -p templates
mkdir -p static
```

**Step 2: Create templates/base.html**

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{% block title %}Interne{% endblock %}</title>
    <link rel="stylesheet" href="/static/style.css">
    <script src="/static/htmx.min.js" defer></script>
</head>
<body>
    <div class="container">
        <header>
            <a href="/" class="logo">Interne</a>
            {% block header_actions %}{% endblock %}
            <span class="date">{{ current_date }}</span>
        </header>

        <main>
            {% block content %}{% endblock %}
        </main>

        <footer>
            <a href="/export">Export</a>
            {% if user.is_some() %}
            <form action="/logout" method="post" style="display: inline;">
                <button type="submit" class="link-button">Logout</button>
            </form>
            {% endif %}
        </footer>
    </div>

    {% block sidebar %}{% endblock %}
</body>
</html>
```

**Step 3: Create static/style.css**

```css
* {
    margin: 0;
    padding: 0;
    box-sizing: border-box;
}

:root {
    --black: #111;
    --white: #fff;
    --gray: #666;
    --light-gray: #eee;
    --border: 1px solid var(--black);
}

body {
    font-family: system-ui, -apple-system, BlinkMacSystemFont, sans-serif;
    font-size: 16px;
    line-height: 1.5;
    background: var(--white);
    color: var(--black);
}

.container {
    max-width: 700px;
    margin: 0 auto;
    padding: 1rem;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
}

/* Header */
header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 0;
    border-bottom: var(--border);
    margin-bottom: 1rem;
}

.logo {
    font-style: italic;
    font-size: 1.25rem;
    text-decoration: none;
    color: var(--black);
}

.date {
    font-style: italic;
    color: var(--gray);
}

/* Main */
main {
    flex: 1;
}

/* Entry cards */
.entry-list {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
}

.entry {
    border: var(--border);
    padding: 0.75rem;
}

.entry.unavailable {
    opacity: 0.5;
}

.entry-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 0.25rem;
}

.entry-title {
    font-size: 1rem;
    font-weight: 600;
}

.entry-title a {
    color: var(--black);
    text-decoration: none;
}

.entry-title a:hover {
    text-decoration: underline;
}

.entry-meta {
    font-size: 0.875rem;
    color: var(--gray);
}

.entry-actions {
    display: flex;
    gap: 0.5rem;
    font-size: 0.875rem;
}

.entry-actions a,
.entry-actions button {
    color: var(--gray);
    text-decoration: none;
    background: none;
    border: none;
    cursor: pointer;
    font-size: inherit;
    padding: 0;
}

.entry-actions a:hover,
.entry-actions button:hover {
    color: var(--black);
}

/* Forms */
form {
    display: flex;
    flex-direction: column;
    gap: 1rem;
}

.form-group {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
}

.form-row {
    display: flex;
    gap: 1rem;
}

.form-row .form-group {
    flex: 1;
}

label {
    font-weight: 500;
    font-size: 0.875rem;
}

input,
textarea,
select {
    padding: 0.5rem;
    border: var(--border);
    font-size: 1rem;
    font-family: inherit;
    background: var(--white);
}

input:focus,
textarea:focus,
select:focus {
    outline: 2px solid var(--black);
    outline-offset: 1px;
}

textarea {
    resize: vertical;
    min-height: 4rem;
}

.error-message {
    color: red;
    font-size: 0.875rem;
    min-height: 1.25rem;
}

button[type="submit"] {
    padding: 0.75rem 1.5rem;
    background: var(--black);
    color: var(--white);
    border: none;
    cursor: pointer;
    font-size: 1rem;
    align-self: flex-start;
}

button[type="submit"]:hover {
    background: var(--gray);
}

.link-button {
    background: none;
    border: none;
    color: var(--gray);
    cursor: pointer;
    font-size: inherit;
    text-decoration: underline;
}

/* Empty state */
.empty {
    text-align: center;
    padding: 3rem 1rem;
    color: var(--gray);
}

/* Footer */
footer {
    padding: 1rem 0;
    border-top: var(--border);
    margin-top: 1rem;
    display: flex;
    gap: 1rem;
    font-size: 0.875rem;
}

footer a {
    color: var(--gray);
    text-decoration: none;
}

footer a:hover {
    color: var(--black);
}

/* View toggle (left side) */
.view-toggle {
    position: fixed;
    left: -2rem;
    top: 50%;
    transform: rotate(-90deg) translateX(-50%);
    transform-origin: left center;
    font-size: 0.875rem;
    cursor: pointer;
    color: var(--gray);
    background: var(--white);
    padding: 0.25rem 0.5rem;
    border: var(--border);
    text-decoration: none;
}

.view-toggle:hover {
    color: var(--black);
}

/* Utilities */
.sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
}

/* htmx loading states */
.htmx-request {
    opacity: 0.5;
}

.htmx-swapping {
    opacity: 0.5;
}
```

**Step 4: Download htmx**

Run:
```bash
curl -o static/htmx.min.js https://unpkg.com/htmx.org@2.0.4/dist/htmx.min.js
```

**Step 5: Add static file serving to main.rs**

Update the `app` definition in `main.rs`:

```rust
use tower_http::services::ServeDir;

// In main(), replace the app definition:
let app = Router::new()
    .route("/health", get(health))
    .nest_service("/static", ServeDir::new("static"))
    .layer(session_layer)
    .with_state(state);
```

**Step 6: Verify static files serve**

Run:
```bash
cargo run &
sleep 2
curl -I http://localhost:3000/static/style.css
pkill interne
```

Expected: HTTP 200 response

**Step 7: Commit**

```bash
git add -A
git commit -m "feat: base template and static files"
```

---

## Task 6: Login Routes

**Files:**
- Create: `src/routes/mod.rs`
- Create: `src/routes/auth.rs`
- Create: `templates/login.html`
- Modify: `src/main.rs`

**Step 1: Create routes directory**

Run:
```bash
mkdir -p src/routes
```

**Step 2: Create src/routes/mod.rs**

```rust
pub mod auth;
```

**Step 3: Create src/routes/auth.rs**

```rust
use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use sqlx::SqlitePool;
use tower_sessions::Session;

use crate::auth::{login_user, logout_user};
use crate::models::User;
use crate::AppState;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
    current_date: String,
}

#[derive(Deserialize)]
pub struct LoginForm {
    invite_code: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page))
        .route("/login", post(login_submit))
        .route("/logout", post(logout))
}

async fn login_page() -> impl IntoResponse {
    let template = LoginTemplate {
        error: None,
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
    };
    Html(template.render().unwrap())
}

async fn login_submit(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let user: Option<User> = sqlx::query_as(
        "SELECT * FROM users WHERE invite_code = ?"
    )
    .bind(&form.invite_code)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    match user {
        Some(user) => {
            login_user(&session, user).await.unwrap();
            Redirect::to("/").into_response()
        }
        None => {
            let template = LoginTemplate {
                error: Some("Invalid invite code".to_string()),
                current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
            };
            Html(template.render().unwrap()).into_response()
        }
    }
}

async fn logout(session: Session) -> impl IntoResponse {
    logout_user(&session).await.unwrap();
    Redirect::to("/login")
}
```

**Step 4: Create templates/login.html**

```html
{% extends "base.html" %}

{% block title %}Login - Interne{% endblock %}

{% block content %}
<div style="max-width: 300px; margin: 3rem auto;">
    <h1 style="font-size: 1.25rem; margin-bottom: 1.5rem;">Login</h1>

    <form method="post" action="/login">
        <div class="form-group">
            <label for="invite_code">Invite Code</label>
            <input
                type="text"
                id="invite_code"
                name="invite_code"
                required
                autofocus
                autocomplete="off"
            >
            <div class="error-message">
                {% if let Some(err) = error %}
                    {{ err }}
                {% endif %}
            </div>
        </div>

        <button type="submit">Login</button>
    </form>
</div>
{% endblock %}
```

**Step 5: Update main.rs to include auth routes**

Add after `mod models;`:

```rust
mod routes;
```

Update the app definition:

```rust
let app = Router::new()
    .route("/health", get(health))
    .merge(routes::auth::router())
    .nest_service("/static", ServeDir::new("static"))
    .layer(session_layer)
    .with_state(state);
```

**Step 6: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles successfully

**Step 7: Commit**

```bash
git add -A
git commit -m "feat: login and logout routes"
```

---

## Task 7: Entry List & Home Page

**Files:**
- Create: `src/routes/entries.rs`
- Create: `templates/entries/list.html`
- Create: `templates/entries/entry.html`
- Modify: `src/routes/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create templates/entries directory**

Run:
```bash
mkdir -p templates/entries
```

**Step 2: Create src/routes/entries.rs**

```rust
use askama::Template;
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse},
    routing::{get, post},
    Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;

use crate::auth::AuthUser;
use crate::models::{Entry, Visit};
use crate::AppState;

#[derive(Template)]
#[template(path = "entries/list.html")]
struct EntryListTemplate {
    entries: Vec<EntryView>,
    filter: String,
    current_date: String,
    user_name: String,
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

#[derive(Deserialize)]
pub struct ListQuery {
    filter: Option<String>,
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
    sqlx::query_as::<_, (Entry, i64)>(
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
    .unwrap_or_default()
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
        user_name: user.name,
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
        user_name: user.name,
    };
    Html(template.render().unwrap())
}

async fn visit_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
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

    // Fetch updated entry for htmx response
    let entry: Entry = sqlx::query_as("SELECT * FROM entries WHERE id = ?")
        .bind(&id)
        .fetch_one(&state.db)
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
    Html(template.render().unwrap())
}
```

**Step 3: Create templates/entries/list.html**

```html
{% extends "base.html" %}

{% block title %}Interne{% endblock %}

{% block header_actions %}
<a href="/entries/new">+ Add</a>
{% endblock %}

{% block content %}
<div id="entry-list" class="entry-list">
    {% if entries.is_empty() %}
    <p class="empty">
        {% if filter == "available" %}
            Nothing due. Go outside!
        {% else %}
            No entries yet. Add one!
        {% endif %}
    </p>
    {% else %}
        {% for entry in entries %}
            {% include "entries/entry.html" %}
        {% endfor %}
    {% endif %}
</div>
{% endblock %}

{% block sidebar %}
<a
    href="{% if filter == \"available\" %}/all{% else %}/{% endif %}"
    class="view-toggle"
    hx-get="{% if filter == \"available\" %}/all{% else %}/{% endif %}"
    hx-target="#entry-list"
    hx-select="#entry-list"
    hx-swap="outerHTML"
    hx-push-url="true"
>
    {% if filter == "available" %}View All{% else %}View Available{% endif %}
</a>
{% endblock %}
```

**Step 4: Create templates/entries/entry.html**

```html
<div class="entry {% if !entry.is_available %}unavailable{% endif %}" id="entry-{{ entry.id }}">
    <div class="entry-header">
        <div class="entry-title">
            <a href="{{ entry.url }}" target="_blank" rel="noopener noreferrer">
                {{ entry.title }} &rarr;
            </a>
        </div>
        <div class="entry-actions">
            {% if entry.is_available %}
            <button
                hx-post="/entries/{{ entry.id }}/visit"
                hx-target="#entry-{{ entry.id }}"
                hx-swap="outerHTML"
            >
                Mark Read
            </button>
            {% endif %}
            <a href="/entries/{{ entry.id }}/edit">Edit</a>
        </div>
    </div>
    <div class="entry-meta">
        {% match entry.last_viewed %}
            {% when Some with (viewed) %}
                {{ viewed }}
            {% when None %}
                Never viewed
        {% endmatch %}
        {% if let Some(available) = entry.available_in %}
            &middot; Available {{ available }}
        {% endif %}
    </div>
</div>
```

**Step 5: Update src/routes/mod.rs**

```rust
pub mod auth;
pub mod entries;
```

**Step 6: Update main.rs**

Update the app definition:

```rust
let app = Router::new()
    .route("/health", get(health))
    .merge(routes::auth::router())
    .merge(routes::entries::router())
    .nest_service("/static", ServeDir::new("static"))
    .layer(session_layer)
    .with_state(state);
```

**Step 7: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles (may have warnings about unused code)

**Step 8: Commit**

```bash
git add -A
git commit -m "feat: entry list and home page"
```

---

## Task 8: Add/Edit Entry Forms

**Files:**
- Create: `templates/entries/form.html`
- Modify: `src/routes/entries.rs`

**Step 1: Create templates/entries/form.html**

```html
{% extends "base.html" %}

{% block title %}{% if entry.is_some() %}Edit{% else %}Add{% endif %} Entry - Interne{% endblock %}

{% block content %}
<div style="max-width: 500px;">
    <h1 style="font-size: 1.25rem; margin-bottom: 1.5rem;">
        {% if entry.is_some() %}Edit Entry{% else %}Add Entry{% endif %}
    </h1>

    <form method="post" action="{% if let Some(e) = entry %}/entries/{{ e.id }}{% else %}/entries{% endif %}">
        {% if entry.is_some() %}
        <input type="hidden" name="_method" value="PUT">
        {% endif %}

        <div class="form-group">
            <label for="url">URL</label>
            <input
                type="url"
                id="url"
                name="url"
                required
                autofocus
                value="{% if let Some(e) = entry %}{{ e.url }}{% endif %}"
                placeholder="https://example.com"
            >
            <div class="error-message">{% if let Some(err) = errors.get("url") %}{{ err }}{% endif %}</div>
        </div>

        <div class="form-group">
            <label for="title">Title</label>
            <input
                type="text"
                id="title"
                name="title"
                required
                value="{% if let Some(e) = entry %}{{ e.title }}{% endif %}"
            >
            <div class="error-message">{% if let Some(err) = errors.get("title") %}{{ err }}{% endif %}</div>
        </div>

        <div class="form-group">
            <label for="description">Description</label>
            <textarea
                id="description"
                name="description"
                rows="3"
            >{% if let Some(e) = entry %}{% if let Some(desc) = &e.description %}{{ desc }}{% endif %}{% endif %}</textarea>
            <div class="error-message"></div>
        </div>

        <div class="form-row">
            <div class="form-group">
                <label for="duration">Revisit Every</label>
                <input
                    type="number"
                    id="duration"
                    name="duration"
                    required
                    min="1"
                    value="{% if let Some(e) = entry %}{{ e.duration }}{% else %}1{% endif %}"
                >
                <div class="error-message">{% if let Some(err) = errors.get("duration") %}{{ err }}{% endif %}</div>
            </div>

            <div class="form-group">
                <label for="interval">Interval</label>
                <select id="interval" name="interval" required>
                    <option value="hours" {% if let Some(e) = entry %}{% if e.interval == "hours" %}selected{% endif %}{% endif %}>Hours</option>
                    <option value="days" {% if let Some(e) = entry %}{% if e.interval == "days" %}selected{% endif %}{% else %}selected{% endif %}>Days</option>
                    <option value="weeks" {% if let Some(e) = entry %}{% if e.interval == "weeks" %}selected{% endif %}{% endif %}>Weeks</option>
                    <option value="months" {% if let Some(e) = entry %}{% if e.interval == "months" %}selected{% endif %}{% endif %}>Months</option>
                    <option value="years" {% if let Some(e) = entry %}{% if e.interval == "years" %}selected{% endif %}{% endif %}>Years</option>
                </select>
                <div class="error-message"></div>
            </div>
        </div>

        <div class="form-group">
            <label for="tags">Tags</label>
            <input
                type="text"
                id="tags"
                name="tags"
                value="{{ tags_string }}"
                placeholder="comma, separated, tags"
            >
            <div class="error-message"></div>
        </div>

        <div class="form-group">
            <label for="collection_id">Collection</label>
            <select id="collection_id" name="collection_id">
                <option value="">Private</option>
                {% for collection in collections %}
                <option
                    value="{{ collection.id }}"
                    {% if let Some(e) = entry %}{% if e.collection_id == Some(collection.id.clone()) %}selected{% endif %}{% endif %}
                >
                    {{ collection.name }}
                </option>
                {% endfor %}
            </select>
            <div class="error-message"></div>
        </div>

        <div style="display: flex; gap: 1rem; align-items: center;">
            <button type="submit">Save</button>
            <a href="/">Cancel</a>
            {% if entry.is_some() %}
            <button
                type="button"
                class="link-button"
                style="margin-left: auto; color: red;"
                hx-delete="/entries/{{ entry.as_ref().unwrap().id }}"
                hx-confirm="Are you sure you want to delete this entry?"
            >
                Delete
            </button>
            {% endif %}
        </div>
    </form>
</div>
{% endblock %}
```

**Step 2: Add form routes to src/routes/entries.rs**

Add these imports at the top:

```rust
use std::collections::HashMap;
use crate::models::Collection;
```

Add these structs:

```rust
#[derive(Template)]
#[template(path = "entries/form.html")]
struct EntryFormTemplate {
    entry: Option<Entry>,
    collections: Vec<Collection>,
    tags_string: String,
    errors: HashMap<String, String>,
    current_date: String,
    user: Option<crate::models::User>,
}

#[derive(Deserialize)]
pub struct EntryForm {
    url: String,
    title: String,
    description: Option<String>,
    duration: i64,
    interval: String,
    tags: Option<String>,
    collection_id: Option<String>,
}
```

Add these route handlers:

```rust
async fn new_entry_form(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> impl IntoResponse {
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
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap())
}

async fn create_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<EntryForm>,
) -> impl IntoResponse {
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
    .await
    .unwrap();

    // Handle tags
    if let Some(tags) = form.tags {
        for tag_name in tags.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            // Get or create tag
            let tag_id: Option<(String,)> = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
                .bind(&tag_name)
                .fetch_optional(&state.db)
                .await
                .unwrap();

            let tag_id = match tag_id {
                Some((id,)) => id,
                None => {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
                        .bind(&new_id)
                        .bind(&tag_name)
                        .bind(&now)
                        .execute(&state.db)
                        .await
                        .unwrap();
                    new_id
                }
            };

            // Link tag to entry
            sqlx::query("INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?, ?)")
                .bind(&id)
                .bind(&tag_id)
                .execute(&state.db)
                .await
                .unwrap();
        }
    }

    Redirect::to("/").into_response()
}

async fn edit_entry_form(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let entry: Option<Entry> = sqlx::query_as("SELECT * FROM entries WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap();

    let Some(entry) = entry else {
        return Redirect::to("/").into_response();
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
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap()).into_response()
}

async fn update_entry(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Form(form): Form<EntryForm>,
) -> impl IntoResponse {
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
    .await
    .unwrap();

    // Clear existing tags and re-add
    sqlx::query("DELETE FROM entry_tags WHERE entry_id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .unwrap();

    if let Some(tags) = form.tags {
        for tag_name in tags.split(',').map(|s| s.trim().to_lowercase()).filter(|s| !s.is_empty()) {
            let tag_id: Option<(String,)> = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
                .bind(&tag_name)
                .fetch_optional(&state.db)
                .await
                .unwrap();

            let tag_id = match tag_id {
                Some((id,)) => id,
                None => {
                    let new_id = uuid::Uuid::new_v4().to_string();
                    sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
                        .bind(&new_id)
                        .bind(&tag_name)
                        .bind(&now)
                        .execute(&state.db)
                        .await
                        .unwrap();
                    new_id
                }
            };

            sqlx::query("INSERT OR IGNORE INTO entry_tags (entry_id, tag_id) VALUES (?, ?)")
                .bind(&id)
                .bind(&tag_id)
                .execute(&state.db)
                .await
                .unwrap();
        }
    }

    Redirect::to("/").into_response()
}

async fn delete_entry(
    State(state): State<AppState>,
    AuthUser(_user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    sqlx::query("DELETE FROM entries WHERE id = ?")
        .bind(&id)
        .execute(&state.db)
        .await
        .unwrap();

    // htmx expects empty response to remove element
    ([("HX-Redirect", "/")], "")
}
```

Add to imports:

```rust
use axum::response::Redirect;
use axum::routing::delete;
```

Update the router function:

```rust
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
```

**Step 3: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles successfully

**Step 4: Commit**

```bash
git add -A
git commit -m "feat: add and edit entry forms"
```

---

## Task 9: Collections CRUD

**Files:**
- Create: `src/routes/collections.rs`
- Create: `templates/collections/list.html`
- Create: `templates/collections/form.html`
- Create: `templates/collections/show.html`
- Modify: `src/routes/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create templates/collections directory**

Run:
```bash
mkdir -p templates/collections
```

**Step 2: Create src/routes/collections.rs**

```rust
use askama::Template;
use axum::{
    extract::{Path, State},
    response::{Html, IntoResponse, Redirect},
    routing::{delete, get, post},
    Form, Router,
};
use serde::Deserialize;
use std::collections::HashMap;

use crate::auth::AuthUser;
use crate::models::{Collection, CollectionMember, User};
use crate::AppState;

#[derive(Template)]
#[template(path = "collections/list.html")]
struct CollectionListTemplate {
    collections: Vec<CollectionView>,
    current_date: String,
    user: Option<User>,
}

#[derive(Template)]
#[template(path = "collections/form.html")]
struct CollectionFormTemplate {
    collection: Option<Collection>,
    errors: HashMap<String, String>,
    current_date: String,
    user: Option<User>,
}

#[derive(Template)]
#[template(path = "collections/show.html")]
struct CollectionShowTemplate {
    collection: Collection,
    members: Vec<User>,
    is_owner: bool,
    current_date: String,
    user: Option<User>,
}

struct CollectionView {
    id: String,
    name: String,
    is_owner: bool,
    member_count: i64,
}

#[derive(Deserialize)]
pub struct CollectionForm {
    name: String,
}

#[derive(Deserialize)]
pub struct JoinForm {
    invite_code: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/collections", get(list_collections))
        .route("/collections/new", get(new_collection_form))
        .route("/collections", post(create_collection))
        .route("/collections/join", post(join_collection))
        .route("/collections/{id}", get(show_collection))
        .route("/collections/{id}/edit", get(edit_collection_form))
        .route("/collections/{id}", post(update_collection))
        .route("/collections/{id}", delete(delete_collection))
        .route("/collections/{id}/regenerate-invite", post(regenerate_invite))
        .route("/collections/{id}/members/{user_id}", delete(remove_member))
}

async fn list_collections(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> impl IntoResponse {
    let collections: Vec<(Collection, i64)> = sqlx::query_as(
        r#"
        SELECT c.*, COUNT(DISTINCT cm.user_id) + 1 as member_count
        FROM collections c
        LEFT JOIN collection_members cm ON cm.collection_id = c.id
        WHERE c.owner_id = ? OR c.id IN (SELECT collection_id FROM collection_members WHERE user_id = ?)
        GROUP BY c.id
        ORDER BY c.name
        "#
    )
    .bind(&user.id)
    .bind(&user.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let views: Vec<CollectionView> = collections
        .into_iter()
        .map(|(c, count)| CollectionView {
            is_owner: c.owner_id == user.id,
            id: c.id,
            name: c.name,
            member_count: count,
        })
        .collect();

    let template = CollectionListTemplate {
        collections: views,
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap())
}

async fn new_collection_form(AuthUser(user): AuthUser) -> impl IntoResponse {
    let template = CollectionFormTemplate {
        collection: None,
        errors: HashMap::new(),
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap())
}

async fn create_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<CollectionForm>,
) -> impl IntoResponse {
    let collection = Collection::new(user.id, form.name);

    sqlx::query(
        "INSERT INTO collections (id, owner_id, name, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&collection.id)
    .bind(&collection.owner_id)
    .bind(&collection.name)
    .bind(&collection.invite_code)
    .bind(&collection.created_at)
    .bind(&collection.updated_at)
    .execute(&state.db)
    .await
    .unwrap();

    Redirect::to("/collections")
}

async fn join_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Form(form): Form<JoinForm>,
) -> impl IntoResponse {
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE invite_code = ?"
    )
    .bind(&form.invite_code)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    if let Some(collection) = collection {
        let member = CollectionMember::new(collection.id, user.id);
        sqlx::query(
            "INSERT OR IGNORE INTO collection_members (collection_id, user_id, joined_at) VALUES (?, ?, ?)"
        )
        .bind(&member.collection_id)
        .bind(&member.user_id)
        .bind(&member.joined_at)
        .execute(&state.db)
        .await
        .unwrap();
    }

    Redirect::to("/collections")
}

async fn show_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let collection: Option<Collection> = sqlx::query_as("SELECT * FROM collections WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .unwrap();

    let Some(collection) = collection else {
        return Redirect::to("/collections").into_response();
    };

    let members: Vec<User> = sqlx::query_as(
        r#"
        SELECT u.* FROM users u
        JOIN collection_members cm ON cm.user_id = u.id
        WHERE cm.collection_id = ?
        "#
    )
    .bind(&id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let template = CollectionShowTemplate {
        is_owner: collection.owner_id == user.id,
        collection,
        members,
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap()).into_response()
}

async fn edit_collection_form(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE id = ? AND owner_id = ?"
    )
    .bind(&id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    let Some(collection) = collection else {
        return Redirect::to("/collections").into_response();
    };

    let template = CollectionFormTemplate {
        collection: Some(collection),
        errors: HashMap::new(),
        current_date: chrono::Local::now().format("%B %d, %Y").to_string(),
        user: Some(user),
    };
    Html(template.render().unwrap()).into_response()
}

async fn update_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
    Form(form): Form<CollectionForm>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE collections SET name = ?, updated_at = ? WHERE id = ? AND owner_id = ?")
        .bind(&form.name)
        .bind(&now)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .unwrap();

    Redirect::to("/collections")
}

async fn delete_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    sqlx::query("DELETE FROM collections WHERE id = ? AND owner_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .unwrap();

    ([("HX-Redirect", "/collections")], "")
}

async fn regenerate_invite(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let new_code = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query("UPDATE collections SET invite_code = ?, updated_at = ? WHERE id = ? AND owner_id = ?")
        .bind(&new_code)
        .bind(&now)
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await
        .unwrap();

    Redirect::to(&format!("/collections/{}", id))
}

async fn remove_member(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path((collection_id, member_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // Verify user is owner
    let collection: Option<Collection> = sqlx::query_as(
        "SELECT * FROM collections WHERE id = ? AND owner_id = ?"
    )
    .bind(&collection_id)
    .bind(&user.id)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    if collection.is_some() {
        sqlx::query("DELETE FROM collection_members WHERE collection_id = ? AND user_id = ?")
            .bind(&collection_id)
            .bind(&member_id)
            .execute(&state.db)
            .await
            .unwrap();
    }

    Redirect::to(&format!("/collections/{}", collection_id))
}
```

**Step 3: Create templates/collections/list.html**

```html
{% extends "base.html" %}

{% block title %}Collections - Interne{% endblock %}

{% block header_actions %}
<a href="/collections/new">+ New Collection</a>
{% endblock %}

{% block content %}
<h1 style="font-size: 1.25rem; margin-bottom: 1.5rem;">Collections</h1>

<div style="margin-bottom: 2rem;">
    <h2 style="font-size: 1rem; margin-bottom: 0.5rem;">Join a Collection</h2>
    <form method="post" action="/collections/join" style="display: flex; gap: 0.5rem;">
        <input type="text" name="invite_code" placeholder="Invite code" required style="flex: 1;">
        <button type="submit">Join</button>
    </form>
</div>

{% if collections.is_empty() %}
<p class="empty">No collections yet.</p>
{% else %}
<div class="entry-list">
    {% for collection in collections %}
    <div class="entry">
        <div class="entry-header">
            <div class="entry-title">
                <a href="/collections/{{ collection.id }}">{{ collection.name }}</a>
            </div>
            <div class="entry-actions">
                {% if collection.is_owner %}
                <a href="/collections/{{ collection.id }}/edit">Edit</a>
                {% endif %}
            </div>
        </div>
        <div class="entry-meta">
            {{ collection.member_count }} member{% if collection.member_count != 1 %}s{% endif %}
            {% if collection.is_owner %}&middot; Owner{% endif %}
        </div>
    </div>
    {% endfor %}
</div>
{% endif %}
{% endblock %}
```

**Step 4: Create templates/collections/form.html**

```html
{% extends "base.html" %}

{% block title %}{% if collection.is_some() %}Edit{% else %}New{% endif %} Collection - Interne{% endblock %}

{% block content %}
<div style="max-width: 400px;">
    <h1 style="font-size: 1.25rem; margin-bottom: 1.5rem;">
        {% if collection.is_some() %}Edit Collection{% else %}New Collection{% endif %}
    </h1>

    <form method="post" action="{% if let Some(c) = collection %}/collections/{{ c.id }}{% else %}/collections{% endif %}">
        <div class="form-group">
            <label for="name">Name</label>
            <input
                type="text"
                id="name"
                name="name"
                required
                autofocus
                value="{% if let Some(c) = collection %}{{ c.name }}{% endif %}"
            >
            <div class="error-message">{% if let Some(err) = errors.get("name") %}{{ err }}{% endif %}</div>
        </div>

        <div style="display: flex; gap: 1rem; align-items: center;">
            <button type="submit">Save</button>
            <a href="/collections">Cancel</a>
            {% if collection.is_some() %}
            <button
                type="button"
                class="link-button"
                style="margin-left: auto; color: red;"
                hx-delete="/collections/{{ collection.as_ref().unwrap().id }}"
                hx-confirm="Delete this collection? All entries will become private."
            >
                Delete
            </button>
            {% endif %}
        </div>
    </form>
</div>
{% endblock %}
```

**Step 5: Create templates/collections/show.html**

```html
{% extends "base.html" %}

{% block title %}{{ collection.name }} - Interne{% endblock %}

{% block content %}
<div style="margin-bottom: 2rem;">
    <h1 style="font-size: 1.25rem; margin-bottom: 0.5rem;">{{ collection.name }}</h1>

    {% if is_owner %}
    <div style="margin-bottom: 1rem; padding: 1rem; border: 1px solid #eee;">
        <strong>Invite Code:</strong>
        <code style="background: #f5f5f5; padding: 0.25rem 0.5rem;">{{ collection.invite_code }}</code>
        <form method="post" action="/collections/{{ collection.id }}/regenerate-invite" style="display: inline; margin-left: 0.5rem;">
            <button type="submit" class="link-button">Regenerate</button>
        </form>
    </div>
    {% endif %}
</div>

<h2 style="font-size: 1rem; margin-bottom: 0.5rem;">Members</h2>

<div class="entry-list">
    {% for member in members %}
    <div class="entry">
        <div class="entry-header">
            <div class="entry-title">{{ member.name }}</div>
            {% if is_owner %}
            <div class="entry-actions">
                <button
                    class="link-button"
                    style="color: red;"
                    hx-delete="/collections/{{ collection.id }}/members/{{ member.id }}"
                    hx-confirm="Remove {{ member.name }} from this collection?"
                >
                    Remove
                </button>
            </div>
            {% endif %}
        </div>
        <div class="entry-meta">
            {% if let Some(email) = &member.email %}{{ email }}{% endif %}
        </div>
    </div>
    {% endfor %}
</div>

<p style="margin-top: 2rem;">
    <a href="/collections">&larr; Back to collections</a>
</p>
{% endblock %}
```

**Step 6: Update src/routes/mod.rs**

```rust
pub mod auth;
pub mod collections;
pub mod entries;
```

**Step 7: Update main.rs**

Update the app definition:

```rust
let app = Router::new()
    .route("/health", get(health))
    .merge(routes::auth::router())
    .merge(routes::entries::router())
    .merge(routes::collections::router())
    .nest_service("/static", ServeDir::new("static"))
    .layer(session_layer)
    .with_state(state);
```

**Step 8: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles successfully

**Step 9: Commit**

```bash
git add -A
git commit -m "feat: collections CRUD"
```

---

## Task 10: Export Route

**Files:**
- Create: `src/routes/export.rs`
- Modify: `src/routes/mod.rs`
- Modify: `src/main.rs`

**Step 1: Create src/routes/export.rs**

```rust
use axum::{
    extract::State,
    http::header,
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

    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CONTENT_DISPOSITION, &format!("attachment; filename=\"{}\"", filename)),
        ],
        Json(export),
    )
}
```

**Step 2: Update src/routes/mod.rs**

```rust
pub mod auth;
pub mod collections;
pub mod entries;
pub mod export;
```

**Step 3: Update main.rs**

Add to app:

```rust
let app = Router::new()
    .route("/health", get(health))
    .merge(routes::auth::router())
    .merge(routes::entries::router())
    .merge(routes::collections::router())
    .merge(routes::export::router())
    .nest_service("/static", ServeDir::new("static"))
    .layer(session_layer)
    .with_state(state);
```

**Step 4: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles successfully

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: export route"
```

---

## Task 11: Import CLI Command

**Files:**
- Create: `src/cli.rs`
- Modify: `src/main.rs`

**Step 1: Create src/cli.rs**

```rust
use serde::Deserialize;
use sqlx::SqlitePool;
use std::fs;
use uuid::Uuid;

#[derive(Deserialize)]
struct LegacyEntry {
    url: String,
    title: String,
    description: Option<String>,
    duration: String,
    interval: String,
    visited: Option<i64>,
    id: String,
    #[serde(rename = "createdAt")]
    created_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    #[serde(rename = "dismissedAt")]
    dismissed_at: Option<String>,
}

pub async fn import_data(pool: &SqlitePool, file_path: &str, user_id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string(file_path)?;
    let entries: Vec<LegacyEntry> = serde_json::from_str(&content)?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut imported = 0;

    for entry in entries {
        let id = Uuid::new_v4().to_string();
        let duration: i64 = entry.duration.parse().unwrap_or(1);
        let created_at = entry.created_at.unwrap_or_else(|| now.clone());
        let updated_at = entry.updated_at.unwrap_or_else(|| now.clone());

        sqlx::query(
            r#"
            INSERT INTO entries (id, user_id, url, title, description, duration, interval, dismissed_at, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#
        )
        .bind(&id)
        .bind(user_id)
        .bind(&entry.url)
        .bind(&entry.title)
        .bind(&entry.description)
        .bind(duration)
        .bind(&entry.interval)
        .bind(&entry.dismissed_at)
        .bind(&created_at)
        .bind(&updated_at)
        .execute(pool)
        .await?;

        // Create visit records for visited count
        if let Some(visited) = entry.visited {
            for _ in 0..visited {
                let visit_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO visits (id, entry_id, user_id, visited_at) VALUES (?, ?, ?, ?)"
                )
                .bind(&visit_id)
                .bind(&id)
                .bind(user_id)
                .bind(&now)
                .execute(pool)
                .await?;
            }
        }

        imported += 1;
    }

    println!("Imported {} entries", imported);
    Ok(())
}

pub async fn create_user(pool: &SqlitePool, name: &str, email: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let id = Uuid::new_v4().to_string();
    let invite_code = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO users (id, name, email, invite_code, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?)"
    )
    .bind(&id)
    .bind(name)
    .bind(email)
    .bind(&invite_code)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    println!("Created user:");
    println!("  ID: {}", id);
    println!("  Name: {}", name);
    println!("  Invite Code: {}", invite_code);

    Ok(())
}
```

**Step 2: Update main.rs with CLI parsing**

Replace the entire `src/main.rs`:

```rust
mod auth;
mod cli;
mod db;
mod models;
mod routes;

use axum::{routing::get, Router};
use sqlx::SqlitePool;
use std::env;
use std::net::SocketAddr;
use time::Duration;
use tokio::net::TcpListener;
use tower_http::services::ServeDir;
use tower_sessions::{Expiry, SessionManagerLayer};
use tower_sessions_sqlx_store::SqliteStore;

async fn health() -> &'static str {
    "ok"
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::init();
    dotenvy::dotenv().ok();

    let args: Vec<String> = env::args().collect();

    let database_url = env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:data/interne.db".to_string());

    let pool = db::init_pool(&database_url).await;

    // Handle CLI commands
    if args.len() > 1 {
        match args[1].as_str() {
            "import" => {
                if args.len() < 4 {
                    eprintln!("Usage: interne import <file.json> <user_id>");
                    std::process::exit(1);
                }
                cli::import_data(&pool, &args[2], &args[3]).await.unwrap();
                return;
            }
            "create-user" => {
                if args.len() < 3 {
                    eprintln!("Usage: interne create-user <name> [email]");
                    std::process::exit(1);
                }
                let email = args.get(3).map(|s| s.as_str());
                cli::create_user(&pool, &args[2], email).await.unwrap();
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
    let session_store = SqliteStore::new(pool.clone());
    session_store.migrate().await.expect("Failed to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::days(30)));

    let state = AppState { db: pool };

    let app = Router::new()
        .route("/health", get(health))
        .merge(routes::auth::router())
        .merge(routes::entries::router())
        .merge(routes::collections::router())
        .merge(routes::export::router())
        .nest_service("/static", ServeDir::new("static"))
        .layer(session_layer)
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
```

**Step 3: Verify compilation**

Run:
```bash
cargo build
```

Expected: Compiles successfully

**Step 4: Test CLI help**

Run:
```bash
cargo run -- help
```

Expected: Shows help text

**Step 5: Commit**

```bash
git add -A
git commit -m "feat: CLI for import and user creation"
```

---

## Task 12: Dockerfile

**Files:**
- Create: `Dockerfile`
- Create: `docker-compose.yml`
- Create: `.dockerignore`

**Step 1: Create Dockerfile**

```dockerfile
FROM rust:1.90 AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy actual source
COPY src ./src
COPY templates ./templates
COPY migrations ./migrations

# Build for release
RUN touch src/main.rs && cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/interne /usr/local/bin/interne
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/migrations /app/migrations
COPY static /app/static

WORKDIR /app

EXPOSE 3000

CMD ["interne"]
```

**Step 2: Create .dockerignore**

```
target/
.git/
.gitignore
data/
*.md
.env*
```

**Step 3: Create docker-compose.yml**

```yaml
services:
  interne:
    build: .
    ports:
      - "3000:3000"
    volumes:
      - ./data:/app/data
    environment:
      - DATABASE_URL=sqlite:/app/data/interne.db
      - RUST_LOG=info
    restart: unless-stopped
```

**Step 4: Create Cargo.lock (if not exists)**

Run:
```bash
cargo generate-lockfile
```

**Step 5: Verify Docker build**

Run:
```bash
docker build -t interne .
```

Expected: Builds successfully

**Step 6: Commit**

```bash
git add -A
git commit -m "feat: Dockerfile and docker-compose"
```

---

## Task 13: Final Testing & Polish

**Step 1: Run the full application**

```bash
cargo run &
sleep 3
```

**Step 2: Create a test user**

```bash
cargo run -- create-user "Test User" "test@example.com"
```

Note the invite code printed.

**Step 3: Test in browser**

1. Open http://localhost:3000
2. Should redirect to /login
3. Enter the invite code
4. Should redirect to /
5. Click "+ Add" and create an entry
6. Verify it appears in the list
7. Click "Mark Read" - entry should update
8. Toggle "View All" - should show unavailable entries
9. Edit an entry
10. Delete an entry
11. Create a collection
12. Export data

**Step 4: Test import**

```bash
cargo run -- import /Users/axelav/s/interne/INTERNE_DB_2026-02-12.json <user-id>
```

**Step 5: Stop server and commit any fixes**

```bash
pkill interne
git add -A
git commit -m "chore: final polish"
```

---

## Summary

After completing all tasks you will have:

- Rust/Axum web server
- SQLite database with full schema
- Session-based auth with invite codes
- Entry CRUD with tags
- Collection CRUD with member management
- htmx for partial page updates
- Brutalist CSS styling
- CLI for user creation and data import
- Docker deployment ready

To run in production:
1. Copy `docker-compose.yml` to your Hetzner box
2. Build and run: `docker-compose up -d`
3. Create your first user: `docker-compose exec interne interne create-user "Your Name"`
4. Configure reverse proxy to route traffic to port 3000
