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
