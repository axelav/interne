# Interne

Spaced repetition for websites. Track URLs you want to revisit periodically, mark them read, see them again when they're due.

## Stack

- **Rust 1.94 + Axum** — language and web framework
- **SQLite** via sqlx 0.9 — async database access
- **Askama** — type-safe Jinja2-style HTML templates
- **htmx** — partial page updates without custom JS
- **Docker** — multi-stage build for deployment

## Project Structure

```
src/
├── main.rs              # server + CLI entrypoint
├── lib.rs               # app builder (shared by server + tests)
├── auth.rs              # session auth, AuthUser extractor
├── cli.rs               # import, invitation, and recovery commands
├── db.rs                # connection pool + migrations
├── error.rs             # AppError type for route handlers
├── models/
│   ├── entry.rs         # Entry, Interval enum
│   ├── collection.rs    # Collection, CollectionMember
│   ├── user.rs          # User
│   └── visit.rs         # Visit
└── routes/
    ├── auth.rs          # GitHub OAuth, legacy linking, and logout
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

The server requires Rust 1.94 and a GitHub OAuth app. GitHub OAuth apps accept only
one configured callback URL, so create a separate app for local development with:

- Homepage URL: `http://127.0.0.1:3000`
- Authorization callback URL: `http://127.0.0.1:3000/auth/github/callback`
- Scopes: leave blank

Copy `.env.example` to `.env`, replace both GitHub credential placeholders, and set:

```dotenv
PUBLIC_BASE_URL=http://127.0.0.1:3000
GITHUB_SIGNUP_MODE=closed
SECURE_COOKIES=false
```

`SECURE_COOKIES=false` is a local-only override for HTTP. Do not use it when the
site is served over HTTPS. Start the server with `cargo run`, then open
[http://127.0.0.1:3000](http://127.0.0.1:3000). A bare `cargo run` without
`GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, and `PUBLIC_BASE_URL` exits with a
configuration error.

Run the test suite:

```bash
cargo test
```

## CLI

```bash
interne                                   # start the web server
interne invite-user <name>                # create a user and invitation URL
interne reset-auth <user-id>              # reset auth and create a recovery URL
interne create-user <name> [email]        # deprecated alias for invite-user
interne import <file.json> <user-id>      # import entries from legacy JSON
interne help                              # show usage
```

`invite-user` and `reset-auth` print URLs that expire after four hours, work once,
and must be sent to the intended person manually. Treat either URL as a secret.
`reset-auth` immediately logs out every session for that user and lets whoever
holds the recovery URL attach a different GitHub identity. Keep this command
available for emergency recovery. The deprecated `create-user` alias ignores its
optional email argument and creates the same invitation as `invite-user`.

The invitation and recovery commands require `PUBLIC_BASE_URL` but do not need the
GitHub client credentials. The web server requires the complete OAuth
configuration.

## Deployment

Register the production GitHub OAuth app with exactly these values:

- Homepage URL: `https://interne.honkytonk.in`
- Authorization callback URL: `https://interne.honkytonk.in/auth/github/callback`
- Scopes: leave blank

Set the OAuth variables below in the deployment environment, store
`GITHUB_CLIENT_SECRET` in the deployment's existing secret mechanism, and then run
`docker compose up -d`. Compose passes the variables through substitution; no real
credential belongs in this repository. The multi-stage image stores SQLite in the
mounted `./data/` directory. Configure the reverse proxy for port 3000 and use
secure cookies in production.

## Environment

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `sqlite:data/interne.db` | SQLite database path |
| `GITHUB_CLIENT_ID` | required | GitHub OAuth app client ID |
| `GITHUB_CLIENT_SECRET` | required | GitHub OAuth app client secret; never commit it |
| `PUBLIC_BASE_URL` | required | Public origin used to build callbacks and connection URLs |
| `GITHUB_SIGNUP_MODE` | `closed` | `closed` rejects unknown identities; `public` creates users |
| `SECURE_COOKIES` | `true` | Set to `false` only for local HTTP development |
| `RUST_LOG` | — | Log level filter, such as `info` or `debug` |

In closed mode, an unknown GitHub user sees exactly: “Access isn’t open yet. Email
webmaster@honkytonk.in for an invite.” Create their account and URL with
`interne invite-user "Name"`. To open registration, set
`GITHUB_SIGNUP_MODE=public` and restart; no schema or code change is needed.

## Production rollout

1. Back up the SQLite database.
2. Configure the production OAuth app with the exact homepage and callback above,
   leaving scopes blank.
3. Pass `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`,
   `PUBLIC_BASE_URL=https://interne.honkytonk.in`, and
   `GITHUB_SIGNUP_MODE=closed` to the deployed service. The sibling
   `honkytonk-infra` repository must be updated separately; do not commit the
   client secret there.
4. Deploy the application and migrations in closed mode.
5. Use the existing invite code once to connect the GitHub account `axelav`.
6. Log out, sign in with GitHub, and verify that the same existing entries appear.
7. Retain `interne reset-auth <user-id>` for emergencies. Verify that the command
   is available without resetting the production account during rollout.

## Legacy cleanup

Invite-code login and `create-user` remain temporarily for migration. After every
existing user has connected GitHub, a follow-up change can remove the legacy
invite-code route and conditional form, the Rust model field, the SQLite column,
and the deprecated `create-user` alias. Invitation and recovery continue through
single-use connection URLs.

## Future Work

- [ ] Remove the legacy invite-code route, UI, Rust model field, SQLite column, and deprecated `create-user` alias after all existing users have connected GitHub.

## Data Model

- **users** — GitHub identity plus a temporary nullable legacy invite code; no passwords
- **entries** — URLs with title, description, duration/interval for spaced repetition
- **visits** — full history of entry views per user
- **collections** — shared groups of entries with invite codes
- **collection_members** — join table for collection membership
- **tags** / **entry_tags** — tagging system for entries
