# Interne Rebuild Design

## Overview

Fresh rebuild of Interne - a spaced repetition system for websites. Track URLs you want to revisit periodically, mark them read, see them again when they're due.

## Goals

- Multi-user with private entries and shared collections
- Sync across devices (refresh-based now, polling/SSE later)
- Simpler, more brutalist UI
- Server-rendered with minimal client JS
- Lightweight deployment

## Stack

- **Rust + Axum** - web framework
- **SQLite** via sqlx - async, compile-time checked queries
- **Askama** - type-safe templates
- **htmx** - partial page updates without custom JS
- **Docker** - multi-stage build for small image (~20MB)

## Project Structure

```
interne/
├── src/
│   ├── main.rs
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── entries.rs
│   │   ├── collections.rs
│   │   └── auth.rs
│   ├── models/
│   ├── templates/
│   └── db.rs
├── templates/
│   ├── base.html
│   ├── entries/
│   └── collections/
├── static/
│   └── style.css
├── migrations/
├── Cargo.toml
├── Dockerfile
└── .env
```

## Data Model

### users

| Column      | Type     | Notes                     |
|-------------|----------|---------------------------|
| id          | TEXT     | UUID, primary key         |
| name        | TEXT     | Display name              |
| email       | TEXT     | Optional, unique if set   |
| invite_code | TEXT     | Unique, used for login    |
| created_at  | DATETIME |                           |
| updated_at  | DATETIME |                           |

### entries

| Column        | Type     | Notes                              |
|---------------|----------|------------------------------------|
| id            | TEXT     | UUID, primary key                  |
| user_id       | TEXT     | FK → users (creator)               |
| collection_id | TEXT     | FK → collections, nullable = private |
| url           | TEXT     | Required                           |
| title         | TEXT     | Required                           |
| description   | TEXT     | Optional                           |
| duration      | INTEGER  | e.g., 2                            |
| interval      | TEXT     | hours/days/weeks/months/years      |
| dismissed_at  | DATETIME | Last marked read                   |
| created_at    | DATETIME |                                    |
| updated_at    | DATETIME | For sync - bumped on field changes |

### visits

| Column     | Type     | Notes            |
|------------|----------|------------------|
| id         | TEXT     | UUID, primary key|
| entry_id   | TEXT     | FK → entries     |
| user_id    | TEXT     | FK → users       |
| visited_at | DATETIME |                  |

Replaces the old `visited` counter. Full history of clicks.

### collections

| Column      | Type     | Notes              |
|-------------|----------|--------------------|
| id          | TEXT     | UUID, primary key  |
| owner_id    | TEXT     | FK → users         |
| name        | TEXT     |                    |
| invite_code | TEXT     | For joining        |
| created_at  | DATETIME |                    |
| updated_at  | DATETIME |                    |

### collection_members

| Column        | Type     | Notes          |
|---------------|----------|----------------|
| collection_id | TEXT     | FK → collections |
| user_id       | TEXT     | FK → users     |
| joined_at     | DATETIME |                |

### tags

| Column     | Type     | Notes              |
|------------|----------|--------------------|
| id         | TEXT     | UUID, primary key  |
| name       | TEXT     | Unique, lowercase  |
| created_at | DATETIME |                    |

### entry_tags

| Column   | Type | Notes          |
|----------|------|----------------|
| entry_id | TEXT | FK → entries   |
| tag_id   | TEXT | FK → tags      |

Index on `entry_tags(tag_id)` for fast tag queries.

## Authentication

**Invite codes.** No email/password flow initially.

1. Admin creates user in DB with unique invite code
2. User visits `/login`, enters code
3. Server validates, sets session cookie
4. Session cookie: 30 days sliding expiration (refreshed on each request)

Upgrade path: add `password_hash` column later for email/password auth.

## Collections

- Entries with `collection_id = NULL` are private to the creator
- Entries with `collection_id` set are visible to all collection members
- Collection owner generates invite codes
- Members join via `/collections/join` with the code
- Owner can remove members

## Routes

### Auth

- `GET /login` - invite code form
- `POST /login` - validate code, set session
- `POST /logout` - clear session

### Entries

- `GET /` - available entries (due now)
- `GET /all` - all entries including not-yet-due
- `GET /entries/new` - add form
- `POST /entries` - create
- `GET /entries/:id/edit` - edit form
- `PUT /entries/:id` - update
- `DELETE /entries/:id` - delete
- `POST /entries/:id/visit` - log visit, update dismissed_at

### Collections

- `GET /collections` - list your collections
- `GET /collections/new` - create form
- `POST /collections` - create
- `GET /collections/:id` - view collection entries
- `GET /collections/:id/edit` - settings
- `PUT /collections/:id` - update
- `DELETE /collections/:id` - delete (owner only)
- `GET /collections/:id/invite` - show/generate invite code
- `POST /collections/join` - join via code

### Data

- `GET /export` - export your data as JSON

## UI Design

### Layout

- Single column, centered, max-width ~700px
- Brutalist: high contrast, minimal decoration, visible borders
- System font stack
- Rotated left-side toggle for "View All / View Available"

### Entry List

```
┌─────────────────────────────────────────────┐
│  Interne              [+ Add]    Feb 15     │
├─────────────────────────────────────────────┤
│                                             │
│  ┌───────────────────────────────────────┐  │
│  │ London Review of Books →              │  │
│  │ Never viewed         Mark Read · Edit │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  ┌───────────────────────────────────────┐  │
│  │ YouTube →                             │  │
│  │ 3 days ago           Mark Read · Edit │  │
│  └───────────────────────────────────────┘  │
│                                             │
│V ┌───────────────────────────────────────┐  │
│i │ Pitchfork →              (muted)      │  │
│e │ Available in 2 weeks              Edit│  │
│w └───────────────────────────────────────┘  │
│                                             │
│A                                            │
│l                                            │
│l                                            │
└─────────────────────────────────────────────┘
```

### Add/Edit Form

Fields in tab order:
1. URL - `<input type="url" required autofocus>`
2. Title - `<input type="text" required>`
3. Description - `<textarea>` optional
4. Duration - `<input type="number" min="1" required>`
5. Interval - `<select>` hours/days/weeks/months/years
6. Tags - `<input type="text">` comma-separated
7. Collection - `<select>` Private or collection name
8. Submit button

HTML5 validation, no JS. Error messages have reserved space (no layout shift).

## htmx Interactions

### Mark Read

```html
<button hx-post="/entries/123/visit"
        hx-swap="outerHTML"
        hx-target="closest .entry">
  Mark Read
</button>
```

### Delete

```html
<button hx-delete="/entries/123"
        hx-confirm="Are you sure?"
        hx-swap="outerHTML swap:0.2s"
        hx-target="closest .entry">
  Delete
</button>
```

### Filter Toggle

```html
<a hx-get="/entries?filter=available"
   hx-target="#entry-list"
   hx-push-url="true">
  View Available
</a>
```

Forms use normal POST, full page response. On success redirect, on error re-render with validation messages.

## Sync Strategy

**Now:** Refresh-based. Changes sync when you reload or revisit.

**Future:** `updated_at` timestamps on entries enable:
- Polling: `GET /entries?since=<timestamp>` returns changed entries
- SSE: Server pushes changes to connected clients

No schema changes needed for the upgrade.

## Deployment

### Dockerfile

```dockerfile
FROM rust:1.90 AS builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/interne /usr/local/bin/
EXPOSE 3000
CMD ["interne"]
```

### Docker Compose

```yaml
interne:
  build: ./interne
  ports:
    - "3000:3000"
  volumes:
    - ./data/interne:/data
  environment:
    - DATABASE_URL=/data/interne.db
    - SESSION_SECRET=${INTERNE_SESSION_SECRET}
```

Reverse proxy routes domain to localhost:3000.

### Backups

Cron job copies SQLite file. Single file, simple.

## Migration

CLI command to import existing data:

```bash
interne import INTERNE_DB_2026-02-12.json --user <user-id>
```

Creates entries from the JSON, associates with specified user.
