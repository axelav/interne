# Interne

Spaced repetition for websites. Track URLs you want to revisit periodically, mark them read, see them again when they're due.

New links default to a one-week revisit interval. Choose a shorter or longer interval in the Add Link form.

When adding or editing a link, type in any comma-separated segment of the Tags
field to find matching tags from your own links. Tags already in another
segment are not suggested, and an exact match appears first. Click a suggestion
or use ↑/↓ and Enter to select it. Escape closes the list without changing the tags.

## Stack

- **Rust 1.94 + Axum** — language and web framework
- **SQLite** via sqlx 0.9 — async database access
- **Askama** — type-safe Jinja2-style HTML templates
- **htmx** — partial page updates without custom JS
- **Docker** — multi-stage build for deployment

## Suggested link details

On Add Link, **Suggest details** fetches a public HTML page. It sends a short
text excerpt and tag names from your own saved links to OpenAI to draft a brief
bookmark note and up to four tags. It prefers labels you already use and can
suggest a new one, even if you have no tags yet. Interne omits hidden elements,
closed panel bodies, datalists, struck-out or deleted text, fallback notices,
canvas, SVG graphics, and MathML formulas. In a named disclosure group, only
the first initially open panel contributes its body. Embedded resources are
not fetched; their contents are omitted without joining adjacent text. Interne
reads static shadow-root content but skips hosts that need slot assignment. It uses
image alternative text, native dropdown labels, and listbox rows. A one-row
multiple dropdown contributes a label only when exactly one option is selected.
Ruby text keeps the base spelling without its annotations; superscripts and subscripts
are marked explicitly.
If a script cannot fit, the excerpt drops the incomplete expression and marks
the cutoff with an ellipsis. Interne does not run page scripts or apply CSS.
Pasting or editing the URL does not fetch it.
Interne uses the page’s declared title and keeps the original link target,
even after a redirect.
You can edit or clear any proposed field. Only **Save** creates the link and
applies its tags. If the HTML exceeds the markup limit, Interne looks for a
title and meta description near the start of the document head. It offers
whichever it can read without fetching the page again, sending anything to
OpenAI, or suggesting tags. A refused request, timeout, or oversized download
still fails. You can always fill in the form and save manually.
When reading fails, the server logs a warning with the
failure category and, when a site refuses the request, its HTTP status. The
warning does not include the URL, page content, or your tags.

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
    ├── tags.rs          # tag cloud, per-tag entry views, and tag suggestions
    └── export.rs        # JSON export

templates/               # Askama HTML templates
static/                  # CSS, htmx, and tag autocomplete
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

Copy `.env.example` to `.env`, replace both GitHub credential placeholders, and
make these explicit local-only overrides:

```dotenv
PUBLIC_BASE_URL=http://127.0.0.1:3000
GITHUB_SIGNUP_MODE=closed
SECURE_COOKIES=false
```

`SECURE_COOKIES=false` is a local-only override for HTTP. Do not use it when the
site is served over HTTPS; `.env.example` and Compose default it to `true`. Start
the server with `cargo run`, then open
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
credential belongs in this repository. Compose refuses to render without the
client ID, client secret, and public base URL; secure cookies default to `true`.
The multi-stage image stores SQLite in the mounted `./data/` directory. Configure
the reverse proxy for port 3000. Never set `SECURE_COOKIES=false` in production.

Outbound GitHub HTTPS uses reqwest's embedded WebPKI roots, not the operating
system CA store. The eMudhra incident fixes are locked at `rustls-webpki 0.103.16`
and `webpki-roots 1.0.10`; rebuild and redeploy after updating these dependencies.
Updating `ca-certificates` alone does not fix an older application binary.

## Environment

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `sqlite:data/interne.db` | SQLite database path |
| `GITHUB_CLIENT_ID` | required | GitHub OAuth app client ID |
| `GITHUB_CLIENT_SECRET` | required | GitHub OAuth app client secret; never commit it |
| `PUBLIC_BASE_URL` | required | Public origin used to build callbacks and connection URLs |
| `OPENAI_API_KEY` | unset | Optional instance-wide key for Add Link descriptions. Unset or blank disables suggestions. |
| `GITHUB_SIGNUP_MODE` | `closed` | `closed` rejects unknown identities; `public` creates users |
| `SECURE_COOKIES` | `true` | Set to `false` only for local HTTP development |
| `RUST_LOG` | `info` | Tracing filter, such as `info` or `interne=debug`; invalid filters stop startup. HTML parser logs are suppressed so diagnostics don't include raw page contents. |

In closed mode, an unknown GitHub user sees exactly: “Access isn’t open yet. Email
webmaster@honkytonk.in for an invite.” Create their account and URL with
`interne invite-user "Name"`. To open registration, set
`GITHUB_SIGNUP_MODE=public` and restart; no schema or code change is needed.

To enable suggestions, set `OPENAI_API_KEY` in the server environment (or local
`.env`) and restart Interne. For Compose, supply it through the deployment’s
existing secret mechanism as an environment variable. The key stays on the
server. Interne sends the excerpt and up to 100 of the reader’s most-used short
tag names to OpenAI’s Responses API using `gpt-4.1-mini` with response storage
disabled. It does not send the pasted URL as a separate field, other saved
links, visit history, or tag names used only by other readers.

Every signed-in reader can use the shared OpenAI key through suggestions. Before
enabling suggestions with public signup, set request limits for
`POST /entries/suggest` at the reverse proxy and configure OpenAI project usage
alerts and rate limits. Neither public signup nor suggestions is enabled by
default.

## Production rollout

Use the [GitHub authentication production rollout runbook](docs/github-auth-production-rollout.md)
for the gated deployment, backup, proxy-log verification, real-provider smoke
test, and rollback procedure.

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
