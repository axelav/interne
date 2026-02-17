# Interne

Spaced repetition for websites. Track URLs you want to revisit periodically, mark them read, see them again when they're due.

## Stack

- **Rust + Axum** — web framework
- **SQLite** via sqlx — async database access
- **Askama** — type-safe Jinja2-style HTML templates
- **htmx** — partial page updates without custom JS
- **Docker** — multi-stage build for deployment

## Project Structure

```
src/
├── main.rs              # server + CLI entrypoint
├── lib.rs               # app builder (shared by server + tests)
├── auth.rs              # session auth, AuthUser extractor
├── cli.rs               # import and create-user commands
├── db.rs                # connection pool + migrations
├── error.rs             # AppError type for route handlers
├── models/
│   ├── entry.rs         # Entry, Interval enum
│   ├── collection.rs    # Collection, CollectionMember
│   ├── user.rs          # User
│   └── visit.rs         # Visit
└── routes/
    ├── auth.rs          # login/logout
    ├── entries.rs       # CRUD, visit, availability logic
    ├── collections.rs   # CRUD, join/leave, member management
    ├── tags.rs          # tag cloud + per-tag entry views
    └── export.rs        # JSON export

templates/               # Askama HTML templates
static/                  # CSS + htmx
migrations/              # SQLite schema
tests/                   # integration tests (TestApp + in-memory SQLite)
build.rs                 # static asset cache-busting hash
```

## Development

```bash
# prerequisites: rust toolchain
cargo run
```

Server starts at [http://localhost:3000](http://localhost:3000).

Create a user to log in:

```bash
cargo run -- create-user "Your Name"
# prints an invite code — use it at /login
```

Run the test suite:

```bash
cargo test
```

## CLI

```bash
interne                                  # start the web server
interne create-user <name> [email]       # create a user, prints invite code + ID
interne import <file.json> <user-id>     # import entries from legacy JSON
interne help                             # show usage
```

## Deployment

```bash
docker compose up -d
```

Uses a multi-stage Docker build. SQLite database is stored in `./data/` via a volume mount. Configure a reverse proxy to route traffic to port 3000.

## Environment

| Variable         | Default                  | Description                                    |
|------------------|--------------------------|------------------------------------------------|
| `DATABASE_URL`   | `sqlite:data/interne.db` | SQLite database path                           |
| `SECURE_COOKIES` | `true`                   | Set to `false` for local HTTP dev (no HTTPS)   |
| `RUST_LOG`       | —                        | Log level filter (e.g. `info`, `debug`)        |

## Data Model

- **users** — invite-code auth, no passwords
- **entries** — URLs with title, description, duration/interval for spaced repetition
- **visits** — full history of entry views per user
- **collections** — shared groups of entries with invite codes
- **collection_members** — join table for collection membership
- **tags** / **entry_tags** — tagging system for entries
