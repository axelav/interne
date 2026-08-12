# GitHub Authentication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace routine invite-code login with GitHub OAuth while preserving existing accounts, supporting closed invitations and later public signup, and retaining an SSH-only recovery path.

**Architecture:** Add a mockable GitHub provider at the application boundary, keep OAuth and connection state in server-side sessions, and identify users by GitHub's stable numeric ID. A dedicated connection-token module owns four-hour invitation and recovery credentials; protected routes continue to depend only on `AuthUser`.

**Tech Stack:** Rust 1.90, Axum 0.8, SQLite via sqlx 0.8, Askama 0.15, tower-sessions 0.15, reqwest 0.12 with rustls, SHA-256, PKCE S256, GitHub OAuth web flow

## Global Constraints

- Production base URL is exactly `https://interne.honkytonk.in`.
- Production callback URL is exactly `https://interne.honkytonk.in/auth/github/callback`.
- Non-production `PUBLIC_BASE_URL` values may use HTTP only with literal loopback hosts `127.0.0.1` or `[::1]`; `localhost` is rejected.
- GitHub OAuth requests use an empty permission scope, random `state`, and PKCE S256.
- GitHub access tokens exist only long enough to fetch `/user` and are never persisted.
- GitHub's numeric user ID, stored as an opaque decimal string, is authoritative; usernames are display metadata.
- `GITHUB_SIGNUP_MODE` accepts only `closed` or `public` and defaults to `closed`.
- Unknown GitHub identities in closed mode see: “Access isn’t open yet. Email webmaster@honkytonk.in for an invite.”
- Invitation and recovery tokens are random, stored only as SHA-256 hashes, single-use, and expire after exactly four hours.
- OAuth attempts and pending confirmations expire after ten minutes; restricted legacy sessions expire after thirty minutes.
- Linking GitHub nulls the legacy invite code; accounts have no self-service unlink operation.
- `reset-auth` disconnects GitHub, revokes every existing session through `auth_version`, and issues a recovery URL.
- Public signup stores no GitHub email and uses display name with username fallback.
- Preserve all existing user IDs and related entries, visits, collections, memberships, and tags.
- Every implementation commit is signed and uses conventional commit format.

---

## File Map

- `migrations/003_github_auth.sql`: safely rebuild `users` with nullable legacy credentials and add connection-token storage.
- `src/models/user.rs`: represent GitHub identity and session generation on users.
- `src/config.rs`: parse and validate server auth configuration without exposing the client secret.
- `src/github.rs`: own GitHub authorization URLs, token exchange, and `/user` lookup behind a testable trait.
- `src/auth.rs`: own full-session, migration-session, OAuth-attempt, and pending-confirmation state.
- `src/connection_tokens.rs`: issue, validate, invalidate, and atomically consume invitation/recovery tokens.
- `src/routes/auth.rs`: coordinate login, callback, connection confirmation, recovery, and logout routes.
- `src/lib.rs`: inject auth configuration and GitHub provider into `AppState`.
- `src/main.rs`: wire production configuration and expose `invite-user` and `reset-auth` commands.
- `src/cli.rs`: retain legacy import logic and provide thin user-facing wrappers over connection-token operations.
- `templates/login.html`: make GitHub the primary login action and conditionally expose legacy migration login.
- `templates/connect_github.html`: explain the required legacy-to-GitHub migration step.
- `templates/github_confirm.html`: confirm the GitHub username before linking.
- `templates/auth_error.html`: render safe, actionable authentication errors.
- `static/style.css`: style OAuth actions and authentication status pages using existing design tokens.
- `tests/common/mod.rs`: provide a fake GitHub provider and OAuth-based test login helper.
- `tests/migrations.rs`: prove the parent-table rebuild preserves related records.
- `tests/auth.rs`: cover session generations and legacy migration behavior.
- `tests/github_auth.rs`: cover OAuth login, linking, signup, confirmation, and provider failures.
- `tests/connection_tokens.rs`: cover invitations, recovery, expiry, replay, and session revocation.
- `.env.example`, `docker-compose.yml`, `README.md`: document and expose required configuration and operations.

---

### Task 1: Migrate users and add authentication storage

**Files:**
- Create: `migrations/003_github_auth.sql`
- Create: `tests/migrations.rs`
- Modify: `src/models/user.rs`

**Interfaces:**
- Produces: `User { invite_code: Option<String>, github_user_id: Option<String>, github_login: Option<String>, auth_version: i64 }`
- Produces: `auth_connection_tokens(id, user_id, token_hash, purpose, created_at, expires_at, consumed_at)`
- Consumes: existing schema from `migrations/001_initial.sql` and timestamp normalization from `migrations/002_timestamps.sql`

- [ ] **Step 1: Write a migration-preservation test**

Create `tests/migrations.rs`. Build the old schema on one in-memory connection, insert a user plus one referencing row in each affected table, execute the new script, and verify both data and foreign keys:

```rust
use sqlx::{Connection, Row, SqliteConnection};

#[tokio::test]
async fn github_auth_migration_preserves_users_and_related_data() {
    let mut db = SqliteConnection::connect("sqlite::memory:").await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/001_initial.sql"))
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/002_timestamps.sql"))
        .execute(&mut db)
        .await
        .unwrap();

    sqlx::query("INSERT INTO users (id, name, invite_code) VALUES ('u1', 'Axel', 'legacy')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entries (id, user_id, url, title, duration, interval) VALUES ('e1', 'u1', 'https://example.com', 'Example', 1, 'days')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO visits (id, entry_id, user_id) VALUES ('v1', 'e1', 'u1')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO collections (id, owner_id, name, invite_code) VALUES ('c1', 'u1', 'Reading', 'collection')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO collection_members (collection_id, user_id) VALUES ('c1', 'u1')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tags (id, name) VALUES ('t1', 'rust')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_tags (entry_id, tag_id) VALUES ('e1', 't1')")
        .execute(&mut db)
        .await
        .unwrap();

    sqlx::raw_sql(include_str!("../migrations/003_github_auth.sql"))
        .execute(&mut db)
        .await
        .unwrap();

    let user = sqlx::query("SELECT invite_code, github_user_id, auth_version FROM users WHERE id = 'u1'")
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert_eq!(user.get::<Option<String>, _>("invite_code").as_deref(), Some("legacy"));
    assert_eq!(user.get::<Option<String>, _>("github_user_id"), None);
    assert_eq!(user.get::<i64, _>("auth_version"), 1);

    for table in ["entries", "visits", "collections", "collection_members", "tags", "entry_tags"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut db)
            .await
            .unwrap();
        assert_eq!(count, 1, "{table} rows must survive the users rebuild");
    }

    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut db)
        .await
        .unwrap();
    assert!(violations.is_empty());
}
```

- [ ] **Step 2: Run the migration test and verify red**

Run: `cargo test --test migrations github_auth_migration_preserves_users_and_related_data -- --exact`

Expected: FAIL because `migrations/003_github_auth.sql` does not exist.

- [ ] **Step 3: Add the no-transaction SQLite migration**

Use SQLite's documented create-copy-drop-rename procedure. `-- no-transaction` is required because `PRAGMA foreign_keys=OFF` must execute before `BEGIN`:

```sql
-- no-transaction
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

CREATE TABLE new_users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    invite_code TEXT UNIQUE,
    github_user_id TEXT UNIQUE,
    github_login TEXT,
    auth_version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO new_users (
    id, name, email, invite_code, github_user_id, github_login,
    auth_version, created_at, updated_at
)
SELECT
    id, name, email, invite_code, NULL, NULL,
    1, created_at, updated_at
FROM users;

DROP TABLE users;
ALTER TABLE new_users RENAME TO users;

CREATE TABLE auth_connection_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    purpose TEXT NOT NULL CHECK (purpose IN ('invite', 'recovery')),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);

CREATE INDEX idx_auth_connection_tokens_user_id
    ON auth_connection_tokens(user_id);
CREATE INDEX idx_auth_connection_tokens_active
    ON auth_connection_tokens(token_hash, expires_at, consumed_at);

COMMIT;
PRAGMA foreign_keys = ON;
```

- [ ] **Step 4: Update the Rust user model**

Change `User` fields to match the migrated table exactly:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub invite_code: Option<String>,
    pub github_user_id: Option<String>,
    pub github_login: Option<String>,
    pub auth_version: i64,
    pub created_at: String,
    pub updated_at: String,
}
```

- [ ] **Step 5: Run migration and regression tests**

Run: `cargo test --test migrations`

Expected: PASS.

Run: `cargo test`

Expected: all existing tests PASS without changing existing invite-login behavior yet.

- [ ] **Step 6: Commit the schema checkpoint**

```bash
git add migrations/003_github_auth.sql src/models/user.rs tests/migrations.rs
git commit -S -m "feat(auth): add GitHub identity schema"
```

---

### Task 2: Add validated configuration and the GitHub provider boundary

**Files:**
- Create: `src/config.rs`
- Create: `src/github.rs`
- Modify: `src/lib.rs`
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`

**Interfaces:**
- Produces: `SignupMode::{Closed, Public}`
- Produces: `AuthConfig { public_base_url: Url, signup_mode: SignupMode }`
- Produces: `ServerAuthConfig { auth: AuthConfig, github: GitHubCredentials }`
- Produces: `GitHubProvider::authorization_url(...)` and `GitHubProvider::exchange_code(...)`
- Produces: `GitHubProfile { user_id: String, login: String, name: Option<String> }`

- [ ] **Step 1: Write configuration parsing tests**

Add unit tests in `src/config.rs` against a closure-based parser so tests never mutate process-global environment:

```rust
#[test]
fn signup_mode_defaults_closed_and_rejects_unknown_values() {
    let closed = ServerAuthConfig::from_lookup(|key| match key {
        "GITHUB_CLIENT_ID" => Some("client".into()),
        "GITHUB_CLIENT_SECRET" => Some("secret".into()),
        "PUBLIC_BASE_URL" => Some("https://interne.honkytonk.in".into()),
        _ => None,
    }).unwrap();
    assert_eq!(closed.auth.signup_mode, SignupMode::Closed);

    let error = ServerAuthConfig::from_lookup(|key| match key {
        "GITHUB_CLIENT_ID" => Some("client".into()),
        "GITHUB_CLIENT_SECRET" => Some("secret".into()),
        "PUBLIC_BASE_URL" => Some("https://interne.honkytonk.in".into()),
        "GITHUB_SIGNUP_MODE" => Some("sometimes".into()),
        _ => None,
    }).unwrap_err();
    assert!(error.to_string().contains("closed or public"));
}

#[test]
fn public_base_url_requires_https_except_for_literal_loopback() {
    for invalid in [
        "http://interne.honkytonk.in",
        "http://localhost:3000",
        "https://interne.honkytonk.in/path",
    ] {
        let result = AuthConfig::new(invalid, SignupMode::Closed);
        assert!(result.is_err(), "{invalid} must be rejected");
    }
    assert!(AuthConfig::new("http://127.0.0.1:3000", SignupMode::Closed).is_ok());
}
```

- [ ] **Step 2: Write GitHub authorization URL and PKCE tests**

Add unit tests in `src/github.rs`:

```rust
#[test]
fn authorization_url_has_identity_only_parameters() {
    let client = GitHubOAuthClient::new("client-id", "secret").unwrap();
    let callback = Url::parse("https://interne.honkytonk.in/auth/github/callback").unwrap();
    let url = client.authorization_url(&callback, "state-value", "challenge-value").unwrap();
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();

    assert_eq!(query.get("client_id").map(String::as_str), Some("client-id"));
    assert_eq!(query.get("redirect_uri").map(String::as_str), Some(callback.as_str()));
    assert_eq!(query.get("state").map(String::as_str), Some("state-value"));
    assert_eq!(query.get("code_challenge").map(String::as_str), Some("challenge-value"));
    assert_eq!(query.get("code_challenge_method").map(String::as_str), Some("S256"));
    assert!(!query.contains_key("scope"));
}

#[test]
fn pkce_challenge_is_base64url_sha256() {
    assert_eq!(
        pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
        "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
    );
}
```

- [ ] **Step 3: Run focused tests and verify red**

Run: `cargo test config::tests github::tests`

Expected: FAIL because the modules and dependencies do not exist.

- [ ] **Step 4: Add the HTTP and cryptography dependencies**

Add:

```toml
async-trait = "0.1"
base64 = "0.22"
rand = "0.9"
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
sha2 = "0.10"
```

- [ ] **Step 5: Implement configuration types**

Use this public shape in `src/config.rs`:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignupMode { Closed, Public }

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub public_base_url: Url,
    pub signup_mode: SignupMode,
}

pub struct GitHubCredentials {
    pub client_id: String,
    pub client_secret: String,
}

pub struct ServerAuthConfig {
    pub auth: AuthConfig,
    pub github: GitHubCredentials,
}
```

`AuthConfig::new` must reject credentials in the URL, query strings, fragments, and non-root paths. Require HTTPS except for HTTP URLs whose host is the literal `127.0.0.1` or `[::1]`; reject `localhost`. Normalize the root path to `/`. `ServerAuthConfig::from_lookup` returns named errors for each missing variable and accepts exactly `closed` or `public`. `from_env` delegates to `from_lookup(|key| std::env::var(key).ok())`. Do not derive `Debug` for credentials or server config.

- [ ] **Step 6: Implement the GitHub adapter**

Use this provider seam in `src/github.rs`:

```rust
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubProfile {
    pub user_id: String,
    pub login: String,
    pub name: Option<String>,
}

#[async_trait]
pub trait GitHubProvider: Send + Sync {
    fn authorization_url(
        &self,
        callback_url: &Url,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Url, GitHubError>;

    async fn exchange_code(
        &self,
        callback_url: &Url,
        code: &str,
        pkce_verifier: &str,
    ) -> Result<GitHubProfile, GitHubError>;
}

impl GitHubOAuthClient {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Result<Self, GitHubError>;
}
```

`GitHubOAuthClient` owns the client ID, client secret, and a `reqwest::Client` configured with redirects disabled. `exchange_code` must:

```text
POST https://github.com/login/oauth/access_token
Accept: application/json
body: client_id, client_secret, code, redirect_uri, code_verifier

GET https://api.github.com/user
Accept: application/vnd.github+json
Authorization: Bearer <temporary token>
User-Agent: interne
X-GitHub-Api-Version: 2022-11-28
```

Deserialize the numeric API `id` to `serde_json::Number` and convert it to its decimal string. Return typed errors that name only the failed stage; never include response bodies, secrets, authorization codes, or tokens in `Display` or `Debug` output.

- [ ] **Step 7: Export modules and run tests**

Add `pub mod config;` and `pub mod github;` to `src/lib.rs`.

Run: `cargo test config::tests github::tests`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

- [ ] **Step 8: Commit the provider boundary**

```bash
git add Cargo.toml Cargo.lock src/config.rs src/github.rs src/lib.rs
git commit -S -m "feat(auth): add GitHub OAuth provider"
```

---

### Task 3: Inject authentication services and a fake provider

**Files:**
- Modify: `src/lib.rs`
- Modify: `src/main.rs`
- Modify: `tests/common/mod.rs`

**Interfaces:**
- Consumes: `AuthConfig`, `GitHubCredentials`, `GitHubProvider`, `GitHubOAuthClient`
- Produces: `AuthServices { config: AuthConfig, github: Arc<dyn GitHubProvider> }`
- Produces: `build_app(pool, secure_cookies, auth_services) -> Router`
- Produces: `FakeGitHubProvider::profile_for_code(code, profile)` for integration tests

- [ ] **Step 1: Write a fake-provider contract test**

Add the fake to `tests/common/mod.rs`, then add this test under `#[cfg(test)]` in that module:

```rust
#[tokio::test]
async fn fake_github_returns_the_profile_registered_for_a_code() {
    let fake = FakeGitHubProvider::default();
    let expected = GitHubProfile {
        user_id: "123".into(),
        login: "axelav".into(),
        name: Some("Axel".into()),
    };
    fake.profile_for_code("good-code", expected.clone());

    let actual = fake.exchange_code(
        &Url::parse("https://interne.test/auth/github/callback").unwrap(),
        "good-code",
        "verifier",
    ).await.unwrap();
    assert_eq!(actual, expected);
}
```

- [ ] **Step 2: Run the contract test and verify red**

Run: `cargo test fake_github_returns_the_profile_registered_for_a_code`

Expected: FAIL because the fake and injectable application services do not exist.

- [ ] **Step 3: Add the injectable application boundary**

In `src/lib.rs`:

```rust
#[derive(Clone)]
pub struct AuthServices {
    pub config: AuthConfig,
    pub github: Arc<dyn GitHubProvider>,
}

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub auth: AuthServices,
}

pub async fn build_app(
    pool: SqlitePool,
    secure_cookies: bool,
    auth: AuthServices,
) -> Router
```

Preserve the existing session store, static service, cache headers, trace layer, and route merges. Construct `AppState { db: pool, auth }`.

- [ ] **Step 4: Wire production startup**

In `src/main.rs`, parse CLI commands before OAuth configuration. Only the server path requires GitHub credentials:

```rust
let server_auth = ServerAuthConfig::from_env().unwrap_or_else(|error| {
    eprintln!("Authentication configuration error: {error}");
    std::process::exit(1);
});
let github = GitHubOAuthClient::new(
    server_auth.github.client_id,
    server_auth.github.client_secret,
).unwrap_or_else(|error| {
    eprintln!("GitHub client configuration error: {error}");
    std::process::exit(1);
});
let auth = AuthServices {
    config: server_auth.auth,
    github: Arc::new(github),
};
let app = interne::build_app(pool, secure, auth).await;
```

- [ ] **Step 5: Implement the test fake and test app configuration**

`FakeGitHubProvider` stores `HashMap<String, Result<GitHubProfile, GitHubError>>` behind `Arc<Mutex<_>>`. Its authorization URL is `https://github.test/authorize` with the supplied state and PKCE challenge. Its code exchange returns the registered result or a safe `GitHubError::TokenExchange`.

Give `TestApp` these fields and constructors:

```rust
pub struct TestApp {
    pub router: Router,
    pub db: SqlitePool,
    pub github: FakeGitHubProvider,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_signup_mode(SignupMode::Closed).await
    }

    pub async fn with_signup_mode(signup_mode: SignupMode) -> Self
}
```

Use `AuthConfig::new("https://interne.test", signup_mode)` and inject the fake into `build_app`.

- [ ] **Step 6: Update existing direct `build_app` callers**

Run: `rg -n "build_app\(" src tests`

Expected callers: `src/main.rs` and `tests/common/mod.rs`. Update both; do not add environment-variable reads to tests.

- [ ] **Step 7: Run contract and regression tests**

Run: `cargo test fake_github_returns_the_profile_registered_for_a_code`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

- [ ] **Step 8: Commit dependency injection**

```bash
git add src/lib.rs src/main.rs tests/common/mod.rs
git commit -S -m "refactor(auth): inject GitHub provider"
```

---

### Task 4: Version authenticated sessions

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/routes/auth.rs`
- Modify: `tests/auth.rs`

**Interfaces:**
- Consumes: `User.auth_version`
- Produces: full sessions containing `user_id` and `auth_version`
- Preserves: `AuthUser(pub User)` as the only protected-route interface

- [ ] **Step 1: Write session-generation tests**

Add to `tests/auth.rs`:

```rust
#[tokio::test]
async fn changing_auth_version_revokes_an_existing_session() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    sqlx::query("UPDATE users SET auth_version = auth_version + 1 WHERE id = ?")
        .bind(&user_id)
        .execute(&app.db)
        .await
        .unwrap();

    let response = app.get("/", Some(&cookie)).await;
    assert_redirect(&response, "/login");
}

#[tokio::test]
async fn missing_auth_version_rejects_a_known_user_id() {
    let identity = session_identity(Some("known-user".into()), None);
    assert_eq!(identity, None);
}
```

Put `missing_auth_version_rejects_a_known_user_id` in the `src/auth.rs` unit-test module. `session_identity(Option<String>, Option<i64>) -> Option<(String, i64)>` is a private pure helper used by `AuthUser`, so the test distinguishes a pre-migration session from a wholly unauthenticated request.

- [ ] **Step 2: Run the revocation test and verify red**

Run: `cargo test --test auth changing_auth_version_revokes_an_existing_session -- --exact`

Expected: FAIL because `AuthUser` currently checks only `user_id`.

- [ ] **Step 3: Store and validate the generation**

In `src/auth.rs`:

```rust
const USER_ID_KEY: &str = "user_id";
const AUTH_VERSION_KEY: &str = "auth_version";

pub async fn login_user(
    session: &Session,
    user: &User,
) -> Result<(), tower_sessions::session::Error> {
    session.insert(USER_ID_KEY, &user.id).await?;
    session.insert(AUTH_VERSION_KEY, user.auth_version).await
}
```

`AuthUser` reads both keys and selects with:

```sql
SELECT * FROM users WHERE id = ? AND auth_version = ?
```

If either key is absent or mismatched, return `AuthRedirect`. Keep `logout_user` as `session.flush()`.

- [ ] **Step 4: Cycle the session before every full login**

Retain `session.cycle_id().await?` in the login route immediately before `login_user`. Do not cycle inside `login_user`, because later linking transactions need to decide when old restricted state is cleared.

- [ ] **Step 5: Run auth and full tests**

Run: `cargo test --test auth`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

- [ ] **Step 6: Commit session revocation**

```bash
git add src/auth.rs src/routes/auth.rs tests/auth.rs
git commit -S -m "feat(auth): version authenticated sessions"
```

---

### Task 5: Implement normal GitHub login and public signup

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/routes/auth.rs`
- Modify: `templates/login.html`
- Create: `templates/auth_error.html`
- Modify: `static/style.css`
- Modify: `tests/common/mod.rs`
- Create: `tests/github_auth.rs`
- Modify: existing integration tests that use `TestApp::login`

**Interfaces:**
- Consumes: `AuthServices`, `GitHubProvider`, `SignupMode`, versioned `login_user`
- Produces: `OAuthAttempt { state, pkce_verifier, purpose, expires_at }`
- Produces: `OAuthPurpose::Login`
- Produces routes: `GET /auth/github`, `GET /auth/github/callback`
- Produces: OAuth-based `TestApp::login(github_user_id)` helper

- [ ] **Step 1: Write closed-login, public-signup, and callback-security tests**

Create `tests/github_auth.rs` with helpers that begin OAuth, capture the session cookie and `state` from the fake authorization URL, register a fake profile for a callback code, and call the callback. Cover these behaviors with named tests:

```rust
#[tokio::test]
async fn linked_github_identity_logs_into_existing_user() {
    let app = TestApp::new().await;
    let user_id = app.create_github_user("Axel", "100", "axelav").await;
    let cookie = app.github_login("100", "axelav", Some("Axel")).await;

    let response = app.get("/", Some(&cookie)).await;
    assert_eq!(response.status(), StatusCode::OK);
    let authenticated_id: String = sqlx::query_scalar("SELECT id FROM users WHERE github_user_id = '100'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(authenticated_id, user_id);
}

#[tokio::test]
async fn unknown_identity_is_rejected_in_closed_mode() {
    let app = TestApp::new().await;
    let response = app.github_callback_response("200", "visitor", None).await;
    let body = body_string(response).await;
    assert!(body.contains("Access isn’t open yet"));
    assert!(body.contains("webmaster@honkytonk.in"));
}

#[tokio::test]
async fn unknown_identity_creates_account_in_public_mode() {
    let app = TestApp::with_signup_mode(SignupMode::Public).await;
    let response = app.github_callback_response("200", "visitor", None).await;
    assert_redirect(&response, "/");
    let name: String = sqlx::query_scalar("SELECT name FROM users WHERE github_user_id = '200'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(name, "visitor");
    let email: Option<String> = sqlx::query_scalar("SELECT email FROM users WHERE github_user_id = '200'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(email, None);
}
```

Also add exact tests named:

```text
oauth_callback_rejects_mismatched_state
oauth_callback_rejects_replayed_state
oauth_callback_rejects_expired_attempt
github_login_refreshes_display_username
provider_failure_shows_safe_error
authorization_redirect_has_pkce_and_no_scope
```

- [ ] **Step 2: Run the new integration test file and verify red**

Run: `cargo test --test github_auth`

Expected: FAIL because the routes and OAuth session state do not exist.

- [ ] **Step 3: Add OAuth flow state to `src/auth.rs`**

Use serializable session types:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OAuthPurpose {
    Login,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthAttempt {
    pub state: String,
    pub pkce_verifier: String,
    pub purpose: OAuthPurpose,
    pub expires_at: i64,
}
```

`OAuthAttempt::new(OAuthPurpose::Login, now)` generates 32 random bytes for both state and verifier, base64url-encodes without padding, and sets `expires_at = now.timestamp() + 600`. Add `pkce_challenge()` using SHA-256. `take_oauth_attempt` removes the attempt from the session before validation so a callback cannot be replayed.

- [ ] **Step 4: Add login and callback routes**

In `src/routes/auth.rs`, add:

```rust
.route("/auth/github", get(github_login_start))
.route("/auth/github/callback", get(github_callback))
```

`github_login_start` stores an `OAuthAttempt`, builds the callback from `PUBLIC_BASE_URL`, and redirects to the provider URL.

`github_callback` must, in order:

1. remove and validate the session attempt, state, purpose, and ten-minute expiry;
2. exchange the code and fetch the GitHub profile through the provider;
3. update and load a user by `github_user_id`;
4. in closed mode, render the exact webmaster invitation copy if no user exists;
5. in public mode, insert a user with `invite_code = NULL`, no email, and display-name fallback, using `ON CONFLICT(github_user_id) DO NOTHING` before selecting the winner;
6. cycle the session ID, write the full versioned session, and redirect `/`.

Do not include the provider's raw error in HTML or logs.

- [ ] **Step 5: Make GitHub the primary login action**

Update `LoginTemplate` with `legacy_login_available: bool`. Query:

```sql
SELECT EXISTS(
    SELECT 1 FROM users
    WHERE invite_code IS NOT NULL AND github_user_id IS NULL
)
```

Render a primary **Continue with GitHub** link to `/auth/github`. Keep the existing invite form only when `legacy_login_available` is true. Add `auth_error.html` extending `base.html` with a safe `message` and a link back to `/login`.

- [ ] **Step 6: Convert the shared test login to fake GitHub OAuth**

Add `create_github_user`, `begin_github_login`, `github_callback_response`, and `github_login` to `TestApp`. `github_login` must perform the actual start and callback requests, carry the OAuth-attempt cookie, and return the post-cycle cookie.

Change `TestApp::create_user` to insert a linked GitHub user and return `(user_id, github_user_id)`. Keep the return shape so feature tests need only rename local variables when useful. Add `create_legacy_user` for legacy-specific tests. Change `TestApp::login` to delegate to `github_login` using the supplied GitHub ID.

- [ ] **Step 7: Update existing tests to use the new helper semantics**

Run: `rg -n "invite_code = app.create_user|app.login\(&invite_code\)" tests`

Replace misleading local names with `github_user_id` in each touched test. Do not alter collection invitation tests; collection invite codes are unrelated and remain unchanged.

- [ ] **Step 8: Run OAuth, auth, and full tests**

Run: `cargo test --test github_auth`

Expected: PASS.

Run: `cargo test --test auth`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

- [ ] **Step 9: Commit normal GitHub login**

```bash
git add src/auth.rs src/routes/auth.rs templates/login.html templates/auth_error.html static/style.css tests
git commit -S -m "feat(auth): sign in with GitHub"
```

---

### Task 6: Turn legacy invite login into a one-time GitHub bridge

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/routes/auth.rs`
- Create: `templates/connect_github.html`
- Create: `templates/github_confirm.html`
- Modify: `tests/auth.rs`
- Modify: `tests/github_auth.rs`

**Interfaces:**
- Extends: `OAuthPurpose::Link { user_id: String }`
- Produces: `MigrationSession { user_id, expires_at }`
- Produces: `PendingConnection { user_id, github_profile, proof, expires_at }`
- Produces routes: `GET /auth/connect`, `POST /auth/github/connect`, `GET/POST /auth/github/confirm`
- Consumes: legacy `users.invite_code`

- [ ] **Step 1: Replace legacy-login expectations with bridge tests**

Change the valid-invite test and add restricted-session coverage:

```rust
#[tokio::test]
async fn valid_legacy_code_redirects_to_github_connection() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Test User").await;
    let response = app.post_form("/login", &format!("invite_code={invite_code}"), None).await;
    assert_redirect(&response, "/auth/connect");
}

#[tokio::test]
async fn legacy_session_cannot_access_application_data() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_legacy_user("Test User").await;
    let cookie = app.legacy_login(&invite_code).await;
    let response = app.get("/", Some(&cookie)).await;
    assert_redirect(&response, "/login");
}
```

Add GitHub-flow tests named:

```text
legacy_user_confirms_github_and_keeps_the_same_user_id
link_confirmation_nulls_legacy_invite_code
link_confirmation_rejects_github_id_owned_by_another_user
link_callback_requires_a_live_migration_session
expired_migration_session_cannot_start_linking
pending_confirmation_expires_after_ten_minutes
```

- [ ] **Step 2: Run focused tests and verify red**

Run: `cargo test --test auth valid_legacy_code_redirects_to_github_connection -- --exact`

Expected: FAIL because valid invite codes still create full sessions.

- [ ] **Step 3: Add restricted and pending session types**

In `src/auth.rs`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MigrationSession {
    pub user_id: String,
    pub expires_at: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum ConnectionProof {
    LegacyInvite,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingConnection {
    pub user_id: String,
    pub github_profile: GitHubProfile,
    pub proof: ConnectionProof,
    pub expires_at: i64,
}
```

Provide session helpers that insert, read, and remove these values. Migration sessions expire at `now + 1_800`; pending connections expire at `now + 600`. Neither helper may populate the full-session keys used by `AuthUser`.

- [ ] **Step 4: Restrict legacy login**

Change the invite query to:

```sql
SELECT * FROM users
WHERE invite_code = ? AND github_user_id IS NULL
```

On success, flush the prior session, cycle its ID, store only `MigrationSession`, and redirect `/auth/connect`. Invalid, null, and already-linked credentials all return the existing generic “Invalid invite code” message.

- [ ] **Step 5: Add connect, callback, and confirmation handling**

`GET /auth/connect` validates the migration session and renders an explanation plus a POST form. `POST /auth/github/connect` creates `OAuthPurpose::Link { user_id }` only when the live migration session targets the same user.

Extend the callback: a successful link-purpose exchange stores `PendingConnection` and redirects `/auth/github/confirm`; it does not mutate the user yet.

The confirmation POST opens a transaction and executes:

```sql
UPDATE users
SET github_user_id = ?,
    github_login = ?,
    invite_code = NULL,
    auth_version = auth_version + 1,
    updated_at = ?
WHERE id = ?
  AND invite_code IS NOT NULL
  AND github_user_id IS NULL
```

Reject zero updated rows. Convert a unique `github_user_id` violation into the safe duplicate-identity error. Mark all active connection tokens for the target user consumed, commit, reload the user, flush and cycle the session, write the full session with the incremented generation, and redirect `/`.

- [ ] **Step 6: Add connection and confirmation templates**

`connect_github.html` must explain that GitHub will replace the legacy code. `github_confirm.html` must display only the escaped GitHub username and POST without accepting identity fields from the browser.

- [ ] **Step 7: Run legacy, OAuth, and regression tests**

Run: `cargo test --test auth`

Expected: PASS.

Run: `cargo test --test github_auth`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

- [ ] **Step 8: Commit the one-time bridge**

```bash
git add src/auth.rs src/routes/auth.rs templates/connect_github.html templates/github_confirm.html tests/auth.rs tests/github_auth.rs
git commit -S -m "feat(auth): connect legacy users to GitHub"
```

---

### Task 7: Add four-hour invitation and recovery commands

**Files:**
- Create: `src/connection_tokens.rs`
- Modify: `src/lib.rs`
- Modify: `src/cli.rs`
- Modify: `src/main.rs`
- Create: `tests/connection_tokens.rs`

**Interfaces:**
- Produces: `ConnectionPurpose::{Invite, Recovery}`
- Produces: `IssuedConnection { user_id, plaintext_token, expires_at }`
- Produces: `ConnectionClaim { token_id, user_id, purpose }`
- Produces: `issue_invitation`, `reset_auth`, `validate_token`, `consume_and_link`, `connection_url`
- Consumes: `AuthConfig.public_base_url`, `GitHubProfile`, and the migrated token table

- [ ] **Step 1: Write token-service tests**

Create `tests/connection_tokens.rs` with a migrated in-memory database. Add exact tests:

```rust
#[tokio::test]
async fn invitation_is_hashed_single_use_and_expires_in_four_hours() {
    let app = TestApp::new().await;
    let now = Utc::now();
    let issued = issue_invitation(&app.db, "Guest", now).await.unwrap();

    assert_eq!(issued.expires_at, now + chrono::Duration::hours(4));
    let stored: String = sqlx::query_scalar(
        "SELECT token_hash FROM auth_connection_tokens WHERE user_id = ?"
    ).bind(&issued.user_id).fetch_one(&app.db).await.unwrap();
    assert_ne!(stored, issued.plaintext_token);

    let claim = validate_token(&app.db, &issued.plaintext_token, now).await.unwrap();
    assert_eq!(claim.user_id, issued.user_id);
}
```

Add tests named:

```text
new_invitation_supersedes_an_older_token
expired_token_is_rejected
malformed_token_is_rejected_like_an_expired_token
reset_auth_disconnects_github_and_increments_auth_version
reset_auth_rejects_an_unknown_user
connection_url_uses_public_base_and_percent_encoding
plaintext_token_is_never_written_to_sqlite
```

- [ ] **Step 2: Run token tests and verify red**

Run: `cargo test --test connection_tokens`

Expected: FAIL because `connection_tokens` does not exist.

- [ ] **Step 3: Implement token generation, hashing, validation, and URLs**

In `src/connection_tokens.rs`, expose:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionPurpose { Invite, Recovery }

pub struct IssuedConnection {
    pub user_id: String,
    pub plaintext_token: String,
    pub expires_at: DateTime<Utc>,
}

pub struct ConnectionClaim {
    pub token_id: String,
    pub user_id: String,
    pub purpose: ConnectionPurpose,
}

pub async fn issue_invitation(
    pool: &SqlitePool,
    name: &str,
    now: DateTime<Utc>,
) -> Result<IssuedConnection, ConnectionTokenError>;

pub async fn reset_auth(
    pool: &SqlitePool,
    user_id: &str,
    now: DateTime<Utc>,
) -> Result<IssuedConnection, ConnectionTokenError>;

pub async fn validate_token(
    pool: &SqlitePool,
    plaintext: &str,
    now: DateTime<Utc>,
) -> Result<ConnectionClaim, ConnectionTokenError>;

pub fn connection_url(base: &Url, plaintext: &str) -> Url;
```

Generate 32 random bytes and base64url-encode without padding. Store `base64url(SHA-256(plaintext))`. Reject plaintext outside the exact generated length before querying. In one transaction, invalidate existing tokens with `consumed_at = now`, then insert the replacement expiring at `now + Duration::hours(4)`.

`issue_invitation` creates a UUID user with `invite_code = NULL`, no email, and no GitHub identity. `reset_auth` requires an existing user, clears both GitHub fields, nulls the invite code, increments `auth_version`, updates `updated_at`, and then issues a recovery token in the same transaction.

- [ ] **Step 4: Add CLI-facing wrappers**

In `src/cli.rs` add functions that call the service with `Utc::now()` and return printable records rather than printing secrets internally:

```rust
pub async fn invite_user(
    pool: &SqlitePool,
    name: &str,
    public_base_url: &Url,
) -> Result<(String, Url), Box<dyn std::error::Error>>;

pub async fn reset_user_auth(
    pool: &SqlitePool,
    user_id: &str,
    public_base_url: &Url,
) -> Result<Url, Box<dyn std::error::Error>>;
```

The main binary prints the URL exactly once. Never log it through `tracing`.

- [ ] **Step 5: Add commands and deprecate `create-user`**

Parse `PUBLIC_BASE_URL` for URL-generating CLI commands without requiring GitHub credentials. Add:

```text
interne invite-user <name>
interne reset-auth <user-id>
```

Keep `create-user <name> [email]` as a deprecated alias for `invite-user <name>`. Print a warning that email is ignored and the command will be removed. Update `help` with exact argument names and four-hour expiry.

- [ ] **Step 6: Run token, CLI-unit, and full tests**

Run: `cargo test --test connection_tokens`

Expected: PASS.

Run: `cargo test cli::tests`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

- [ ] **Step 7: Commit CLI recovery primitives**

```bash
git add src/connection_tokens.rs src/lib.rs src/cli.rs src/main.rs tests/connection_tokens.rs
git commit -S -m "feat(auth): issue invitation and recovery links"
```

---

### Task 8: Complete invitation and recovery in the browser

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/connection_tokens.rs`
- Modify: `src/routes/auth.rs`
- Modify: `src/lib.rs`
- Modify: `templates/github_confirm.html`
- Modify: `templates/auth_error.html`
- Modify: `tests/connection_tokens.rs`
- Modify: `tests/github_auth.rs`

**Interfaces:**
- Extends: `OAuthPurpose::ConnectionToken { token_id, user_id, purpose }`
- Extends: `ConnectionProof::Token { token_id, purpose }`
- Produces routes: `GET /recover`, `GET /auth/github/recover`
- Produces: `consume_and_link(pool, claim, profile, now) -> User`

- [ ] **Step 1: Write complete recovery-flow tests**

Add tests that exercise HTTP start, OAuth callback, confirmation, and replay:

```rust
#[tokio::test]
async fn recovery_url_connects_confirmed_github_identity_and_logs_in() {
    let app = TestApp::new().await;
    let (user_id, github_id) = app.create_user("Axel").await;
    let old_cookie = app.login(&github_id).await;
    let issued = reset_auth(&app.db, &user_id, Utc::now()).await.unwrap();

    let response = app.get(
        &format!("/recover?token={}", issued.plaintext_token),
        None,
    ).await;
    assert_redirect(&response, "/auth/github/recover");
    assert_eq!(response.headers()["referrer-policy"], "no-referrer");
    assert!(!response.headers()["location"].to_str().unwrap().contains(&issued.plaintext_token));

    let new_cookie = app.finish_recovery_oauth(response, "900", "axelav").await;
    assert_eq!(app.get("/", Some(&new_cookie)).await.status(), StatusCode::OK);
    assert_redirect(&app.get("/", Some(&old_cookie)).await, "/login");
}
```

Add exact tests named:

```text
invitation_url_connects_the_precreated_user
recovery_token_is_not_consumed_before_confirmation
successful_confirmation_consumes_token
used_recovery_url_cannot_be_replayed
superseded_recovery_url_is_rejected
expired_recovery_url_is_rejected
token_confirmation_rechecks_expiration
token_confirmation_rejects_duplicate_github_identity
token_query_is_removed_before_redirecting_to_github
request_log_path_excludes_recovery_query_string
```

- [ ] **Step 2: Run recovery tests and verify red**

Run: `cargo test --test connection_tokens recovery_url_connects_confirmed_github_identity_and_logs_in -- --exact`

Expected: FAIL because `/recover` is not routed.

- [ ] **Step 3: Add token-backed session purpose**

Extend the serializable enums:

```rust
pub enum OAuthPurpose {
    Login,
    Link { user_id: String },
    ConnectionToken {
        token_id: String,
        user_id: String,
        purpose: ConnectionPurpose,
    },
}

pub enum ConnectionProof {
    LegacyInvite,
    Token {
        token_id: String,
        purpose: ConnectionPurpose,
    },
}
```

Derive `Serialize` and `Deserialize` for `ConnectionPurpose`. Never store the plaintext token in session state.

- [ ] **Step 4: Add the two-step clean recovery redirect**

`GET /recover?token=...` must:

1. hash and validate the token at the current time;
2. store the `ConnectionClaim` in the server-side session;
3. return `Referrer-Policy: no-referrer`;
4. redirect to `/auth/github/recover`, with no token in the location.

`GET /auth/github/recover` removes the stored claim, revalidates the database token record, creates a token-purpose OAuth attempt, and redirects to GitHub. This clean intermediate URL ensures the token is absent from the GitHub request's referrer and from subsequent browser history entries.

- [ ] **Step 5: Extend callback and confirmation**

The callback stores a `PendingConnection` carrying only token ID, target user ID, purpose, fetched profile, and ten-minute expiration.

Add to `src/connection_tokens.rs`:

```rust
pub async fn consume_and_link(
    pool: &SqlitePool,
    claim: &ConnectionClaim,
    profile: &GitHubProfile,
    now: DateTime<Utc>,
) -> Result<User, ConnectionTokenError>;
```

In one transaction, reselect the token by ID and target user, require `consumed_at IS NULL` and `expires_at > now`, reject a GitHub ID owned by another user, update the target user's GitHub fields, null the legacy invite, increment `auth_version`, mark every active token for the target consumed, and reload the user. Require exactly one target token row to transition from unconsumed to consumed. On success, flush and cycle the session and call `login_user` with the new generation.

- [ ] **Step 6: Sanitize request tracing**

Replace the default trace span's URI field with path-only data for all routes. Add `fn trace_path(uri: &Uri) -> &str { uri.path() }` and test it with `/recover?token=secret` before wiring it into:

```rust
.make_span_with(|request: &axum::http::Request<_>| {
    tracing::info_span!(
        "http_request",
        method = %request.method(),
        path = %request.uri().path(),
    )
})
```

Keep the existing request and response log levels. The pure helper test must assert that `/recover?token=secret` is represented as `/recover`.

- [ ] **Step 7: Render purpose-specific confirmation and safe errors**

Use “Accept invitation” for invite tokens, “Recover account” for recovery tokens, and “Connect GitHub” for legacy migration. All invalid/expired/used/superseded token conditions render the same message and link back to login. Never render a token, token hash, OAuth code, provider body, or database error.

- [ ] **Step 8: Run recovery, OAuth, and full tests**

Run: `cargo test --test connection_tokens`

Expected: PASS.

Run: `cargo test --test github_auth`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

- [ ] **Step 9: Commit complete invitation and recovery flows**

```bash
git add src/auth.rs src/connection_tokens.rs src/routes/auth.rs src/lib.rs templates/github_confirm.html templates/auth_error.html tests/connection_tokens.rs tests/github_auth.rs
git commit -S -m "feat(auth): recover accounts through GitHub"
```

---

### Task 9: Document configuration, rollout, and legacy cleanup

**Files:**
- Modify: `.env.example`
- Modify: `docker-compose.yml`
- Modify: `README.md`
- Modify: `static/style.css`

**Interfaces:**
- Documents: production OAuth registration, server configuration, closed/public modes, invitations, recovery, and legacy cleanup
- Preserves: no secrets committed to Git

- [ ] **Step 1: Write documentation acceptance checks**

Run these before editing and confirm at least one fails:

```bash
rg -n "GITHUB_CLIENT_ID|GITHUB_CLIENT_SECRET|PUBLIC_BASE_URL|GITHUB_SIGNUP_MODE" .env.example README.md docker-compose.yml
rg -n "invite-user|reset-auth|webmaster@honkytonk.in" README.md
```

Expected: required configuration and commands are incomplete or absent.

- [ ] **Step 2: Update example environment and local Compose**

Add non-secret placeholders:

```dotenv
GITHUB_CLIENT_ID=replace-with-github-oauth-client-id
GITHUB_CLIENT_SECRET=replace-with-github-oauth-client-secret
PUBLIC_BASE_URL=https://interne.example.com
GITHUB_SIGNUP_MODE=closed
```

Expose the same variable names in `docker-compose.yml`; use Compose substitution for credentials so no real secret enters the file. Keep `SECURE_COOKIES=false` as an explicitly documented local-only override when testing over HTTP.

- [ ] **Step 3: Update README setup and operations**

Document:

```text
Production homepage: https://interne.honkytonk.in
Production callback: https://interne.honkytonk.in/auth/github/callback
Scopes: leave blank
Closed invitation: interne invite-user "Name"
Emergency recovery: interne reset-auth <user-id>
Open signup: set GITHUB_SIGNUP_MODE=public and restart
```

Explain that invitation/recovery URLs last four hours, work once, and must be sent manually. Explain that `reset-auth` immediately logs out every session and allows the holder of the URL to attach a different GitHub identity. Document the exact closed-mode contact copy. For local OAuth testing, document a separate GitHub OAuth app using `http://127.0.0.1:3000/auth/github/callback`; GitHub OAuth apps accept only one configured callback URL.

Add a rollout checklist: back up SQLite, configure the OAuth app, deploy in closed mode, use the existing invite code once to connect `axelav`, log out, verify the same entries after GitHub login, and retain `reset-auth` for emergencies.

- [ ] **Step 4: Finish auth-page styling**

Use existing `--black`, `--gray-*`, `--border`, and `--radius` tokens. Verify `.oauth-button`, `.auth-divider`, `.auth-status`, and confirmation forms work at 320px width and do not introduce external icons, fonts, or JavaScript.

- [ ] **Step 5: Run documentation checks**

Run:

```bash
rg -n "GITHUB_CLIENT_ID|GITHUB_CLIENT_SECRET|PUBLIC_BASE_URL|GITHUB_SIGNUP_MODE" .env.example README.md docker-compose.yml
rg -n "invite-user|reset-auth|webmaster@honkytonk.in" README.md
```

Expected: every required term is present in the appropriate setup or operations section.

- [ ] **Step 6: Run final automated verification**

Run: `cargo fmt --check`

Expected: PASS.

Run: `cargo clippy --all-targets --all-features -- -D warnings`

Expected: PASS.

Run: `cargo test`

Expected: all tests PASS.

Run: `git diff --check`

Expected: no output.

- [ ] **Step 7: Perform local browser smoke test**

With test OAuth credentials and a callback registered for the local environment, verify:

```text
1. /login shows Continue with GitHub first.
2. A linked account reaches its existing entries.
3. An unknown account in closed mode sees the webmaster invitation message.
4. A legacy code cannot open entries and can connect only after confirmation.
5. A generated invitation URL loses its token before leaving Interne.
6. A used URL fails safely.
7. Logout redirects to /login.
```

- [ ] **Step 8: Commit documentation and final styling**

```bash
git add .env.example docker-compose.yml README.md static/style.css
git commit -S -m "docs(auth): document GitHub authentication"
```

- [ ] **Step 9: Review the complete branch**

Run: `git log --oneline main..HEAD`

Expected: the signed, incremental commits from Tasks 1–9 in dependency order.

Run: `git status --short --branch`

Expected: clean `feat/github-auth` worktree.

Invoke `superpowers:requesting-code-review` and review the complete diff against `docs/superpowers/specs/2026-08-11-github-auth-design.md` before pushing.

## Out-of-repository deployment prerequisite

Before production deployment, update the `interne` service in the `honkytonk-infra` repository—on its own worktree and signed commit—to pass `GITHUB_CLIENT_ID`, `GITHUB_CLIENT_SECRET`, `PUBLIC_BASE_URL=https://interne.honkytonk.in`, and `GITHUB_SIGNUP_MODE=closed`. Store the client secret in that deployment's existing secret/environment mechanism; never commit it. This repository's branch does not modify a sibling repository.

## Future Work

- [ ] Remove the legacy invite-code route, conditional form, Rust model field, SQLite column, and deprecated `create-user` alias after every existing user has connected GitHub.

## Implementation References

- [GitHub OAuth web application flow](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps)
- [GitHub OAuth app registration](https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/creating-an-oauth-app)
- [SQLite generalized ALTER TABLE procedure](https://sqlite.org/lang_altertable.html#making_other_kinds_of_table_schema_changes)
- [SQLite foreign-key behavior for DROP TABLE](https://www.sqlite.org/foreignkeys.html#fk_schemacommands)
