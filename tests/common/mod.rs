#![allow(dead_code)]

use axum::Router;
use axum::body::Body;
use axum::http::Request;
use axum::response::Response;
use http_body_util::BodyExt;
use interne::AuthServices;
use interne::config::{AuthConfig, SignupMode};
use interne::github::{GitHubError, GitHubProfile, GitHubProvider};
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::collections::HashMap;
use std::io::Write;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tower_sessions::{SessionStore, session::Id};
use tower_sessions_sqlx_store::SqliteStore;
use tracing::instrument::WithSubscriber;
use url::Url;

static NEXT_GITHUB_USER_ID: AtomicU64 = AtomicU64::new(1_000_000);

#[derive(Clone, Default)]
struct LogCapture(Arc<Mutex<Vec<u8>>>);

struct LogWriter(Arc<Mutex<Vec<u8>>>);

impl Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogCapture {
    type Writer = LogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogWriter(self.0.clone())
    }
}

impl LogCapture {
    fn contents(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

pub async fn capture_error_logs<F, T>(future: F) -> (T, String)
where
    F: Future<Output = T>,
{
    let capture = LogCapture::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_max_level(tracing::Level::ERROR)
        .with_writer(capture.clone())
        .finish();
    let output = future.with_subscriber(subscriber).await;
    (output, capture.contents())
}

#[derive(Clone, Default)]
pub struct FakeGitHubProvider {
    profiles: Arc<Mutex<HashMap<String, Result<GitHubProfile, GitHubError>>>>,
}

impl FakeGitHubProvider {
    pub fn profile_for_code(&self, code: &str, profile: GitHubProfile) {
        self.profiles
            .lock()
            .expect("Fake GitHub profile store should not be poisoned")
            .insert(code.to_string(), Ok(profile));
    }

    pub fn error_for_code(&self, code: &str, error: GitHubError) {
        self.profiles
            .lock()
            .expect("Fake GitHub profile store should not be poisoned")
            .insert(code.to_string(), Err(error));
    }
}

#[async_trait::async_trait]
impl GitHubProvider for FakeGitHubProvider {
    fn authorization_url(
        &self,
        callback_url: &Url,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Url, GitHubError> {
        let mut url = Url::parse("https://github.test/authorize")
            .map_err(|_| GitHubError::AuthorizationUrl)?;
        url.query_pairs_mut()
            .append_pair("redirect_uri", callback_url.as_str())
            .append_pair("state", state)
            .append_pair("code_challenge", pkce_challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url)
    }

    async fn exchange_code(
        &self,
        _callback_url: &Url,
        code: &str,
        _pkce_verifier: &str,
    ) -> Result<GitHubProfile, GitHubError> {
        self.profiles
            .lock()
            .map_err(|_| GitHubError::TokenExchange)?
            .get(code)
            .cloned()
            .unwrap_or(Err(GitHubError::TokenExchange))
    }
}

pub struct TestApp {
    pub router: Router,
    pub db: SqlitePool,
    pub github: FakeGitHubProvider,
}

impl TestApp {
    pub async fn new() -> Self {
        Self::with_signup_mode(SignupMode::Closed).await
    }

    pub async fn with_signup_mode(signup_mode: SignupMode) -> Self {
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

        let github = FakeGitHubProvider::default();
        let auth = AuthServices {
            config: AuthConfig::new("https://interne.test", signup_mode).unwrap(),
            github: Arc::new(github.clone()),
        };
        let router = interne::build_app(pool.clone(), false, auth).await;

        Self {
            router,
            db: pool,
            github,
        }
    }

    /// Send a request through the app and return the response.
    pub async fn request(&self, req: Request<Body>) -> Response {
        tower::ServiceExt::oneshot(self.router.clone(), req)
            .await
            .unwrap()
    }

    /// Create a GitHub-linked user and return (user_id, github_user_id).
    pub async fn create_user(&self, name: &str) -> (String, String) {
        let github_user_id = NEXT_GITHUB_USER_ID
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let github_login = format!("test-{github_user_id}");
        let user_id = self
            .create_github_user(name, &github_user_id, &github_login)
            .await;

        (user_id, github_user_id)
    }

    pub async fn create_github_user(
        &self,
        name: &str,
        github_user_id: &str,
        github_login: &str,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO users (id, name, github_user_id, github_login, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(name)
        .bind(github_user_id)
        .bind(github_login)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("Failed to create test user");

        id
    }

    pub async fn create_legacy_user(&self, name: &str) -> (String, String) {
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
        .expect("Failed to create legacy test user");

        (id, invite_code)
    }

    pub async fn begin_github_login(&self) -> (String, Url) {
        self.begin_github_login_with_cookie(None).await
    }

    pub async fn begin_github_login_with_cookie(&self, cookie: Option<&str>) -> (String, Url) {
        let response = self.get("/auth/github", cookie).await;
        assert!(
            response.status().is_redirection(),
            "GitHub login start should redirect, got {}",
            response.status()
        );
        let authorization_url = Url::parse(
            response
                .headers()
                .get("location")
                .expect("GitHub login start should have a Location header")
                .to_str()
                .unwrap(),
        )
        .expect("GitHub authorization redirect should be a valid URL");
        let session_cookie = response
            .headers()
            .get("set-cookie")
            .map(cookie_from_response)
            .or_else(|| cookie.map(str::to_owned))
            .expect("GitHub login start should establish or preserve a session cookie");

        (session_cookie, authorization_url)
    }

    pub async fn legacy_login(&self, invite_code: &str) -> String {
        let response = self
            .post_form("/login", &format!("invite_code={invite_code}"), None)
            .await;
        assert_redirect(&response, "/auth/connect");
        cookie_from_response(
            response
                .headers()
                .get("set-cookie")
                .expect("Legacy login should establish a migration session"),
        )
    }

    pub async fn begin_github_link(&self, cookie: &str) -> Url {
        let response = self
            .post_form("/auth/github/connect", "", Some(cookie))
            .await;
        assert!(
            response.status().is_redirection(),
            "GitHub link start should redirect, got {}",
            response.status()
        );
        Url::parse(
            response
                .headers()
                .get("location")
                .expect("GitHub link start should have a Location header")
                .to_str()
                .unwrap(),
        )
        .expect("GitHub authorization redirect should be a valid URL")
    }

    pub async fn github_callback_response(
        &self,
        github_user_id: &str,
        github_login: &str,
        name: Option<&str>,
    ) -> Response {
        let (cookie, authorization_url) = self.begin_github_login().await;
        let state = authorization_url
            .query_pairs()
            .find_map(|(key, value)| (key == "state").then(|| value.into_owned()))
            .expect("GitHub authorization URL should contain state");
        let code = format!("code-{github_user_id}");
        self.github.profile_for_code(
            &code,
            GitHubProfile {
                user_id: github_user_id.to_owned(),
                login: github_login.to_owned(),
                name: name.map(str::to_owned),
            },
        );

        self.get(
            &format!("/auth/github/callback?code={code}&state={state}"),
            Some(&cookie),
        )
        .await
    }

    pub async fn github_login(
        &self,
        github_user_id: &str,
        github_login: &str,
        name: Option<&str>,
    ) -> String {
        let response = self
            .github_callback_response(github_user_id, github_login, name)
            .await;
        assert_redirect(&response, "/");
        cookie_from_response(
            response
                .headers()
                .get("set-cookie")
                .expect("GitHub callback should set a post-cycle session cookie"),
        )
    }

    /// Log in as the given GitHub identity and return the session cookie string.
    pub async fn login(&self, github_user_id: &str) -> String {
        let (github_login, name): (String, String) =
            sqlx::query_as("SELECT github_login, name FROM users WHERE github_user_id = ?")
                .bind(github_user_id)
                .fetch_one(&self.db)
                .await
                .expect("GitHub-linked test user should exist");

        self.github_login(github_user_id, &github_login, Some(&name))
            .await
    }

    pub async fn expire_oauth_attempt(&self, cookie: &str) {
        self.expire_session_value(cookie, "oauth_attempt").await;
    }

    pub async fn expire_migration_session(&self, cookie: &str) {
        self.expire_session_value(cookie, "migration_session").await;
    }

    pub async fn expire_pending_connection(&self, cookie: &str) {
        self.expire_session_value(cookie, "pending_connection")
            .await;
    }

    pub async fn retarget_migration_session(&self, cookie: &str, user_id: &str) {
        let (_, encoded_id) = cookie
            .split_once('=')
            .expect("Session cookie should contain an ID");
        let session_id =
            Id::from_str(encoded_id).expect("Session cookie should contain a valid ID");
        let store = SqliteStore::new(self.db.clone());
        let mut record = store
            .load(&session_id)
            .await
            .expect("Session should load")
            .expect("Migration session should exist");
        record
            .data
            .get_mut("migration_session")
            .and_then(serde_json::Value::as_object_mut)
            .expect("Migration session should be stored in the session")
            .insert("user_id".into(), serde_json::json!(user_id));
        store.save(&record).await.expect("Session should save");
    }

    async fn expire_session_value(&self, cookie: &str, key: &str) {
        let (_, encoded_id) = cookie
            .split_once('=')
            .expect("Session cookie should contain an ID");
        let session_id =
            Id::from_str(encoded_id).expect("Session cookie should contain a valid ID");
        let store = SqliteStore::new(self.db.clone());
        let mut record = store
            .load(&session_id)
            .await
            .expect("Session should load")
            .expect("OAuth session should exist");
        record
            .data
            .get_mut(key)
            .and_then(serde_json::Value::as_object_mut)
            .expect("Expiring session value should be stored in the session")
            .insert("expires_at".into(), serde_json::json!(0));
        store.save(&record).await.expect("Session should save");
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

pub fn cookie_from_response(value: &axum::http::HeaderValue) -> String {
    value
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
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

#[cfg(test)]
mod tests {
    use interne::github::{GitHubError, GitHubProfile, GitHubProvider};
    use url::Url;

    use super::FakeGitHubProvider;

    #[tokio::test]
    async fn fake_github_returns_the_profile_registered_for_a_code() {
        let fake = FakeGitHubProvider::default();
        let expected = GitHubProfile {
            user_id: "123".into(),
            login: "axelav".into(),
            name: Some("Axel".into()),
        };
        fake.profile_for_code("good-code", expected.clone());

        let actual = fake
            .exchange_code(
                &Url::parse("https://interne.test/auth/github/callback").unwrap(),
                "good-code",
                "verifier",
            )
            .await
            .unwrap();
        assert_eq!(actual, expected);
    }

    #[tokio::test]
    async fn fake_github_returns_the_error_registered_for_a_code() {
        let fake = FakeGitHubProvider::default();
        fake.error_for_code("failed-code", GitHubError::ProfileFetch);

        let error = fake
            .exchange_code(
                &Url::parse("https://interne.test/auth/github/callback").unwrap(),
                "failed-code",
                "verifier",
            )
            .await
            .unwrap_err();
        assert_eq!(error, GitHubError::ProfileFetch);
    }
}
