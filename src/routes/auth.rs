use askama::Template;
use axum::{
    Form, Router,
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::AppState;
use crate::auth::{
    OAuthAttempt, OAuthPurpose, login_user, logout_user, store_oauth_attempt, take_oauth_attempt,
};
use crate::config::SignupMode;
use crate::error::AppError;
use crate::models::User;

const GITHUB_SIGN_IN_ERROR: &str =
    "We couldn’t complete GitHub sign-in. Please return to login and try again.";
const CLOSED_SIGNUP_ERROR: &str =
    "Access isn’t open yet. Email webmaster@honkytonk.in for an invite.";

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
    legacy_login_available: bool,
    static_hash: &'static str,
    user: Option<User>,
}

#[derive(Template)]
#[template(path = "auth_error.html")]
struct AuthErrorTemplate<'a> {
    message: &'a str,
    static_hash: &'static str,
    user: Option<User>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    invite_code: String,
}

#[derive(Deserialize)]
struct GitHubCallbackQuery {
    code: String,
    state: String,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page))
        .route("/login", post(login_submit))
        .route("/auth/github", get(github_login_start))
        .route("/auth/github/callback", get(github_callback))
        .route("/logout", post(logout))
}

async fn login_page(State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let template = LoginTemplate {
        error: None,
        legacy_login_available: legacy_login_available(&state).await?,
        static_hash: crate::STATIC_HASH,
        user: None,
    };
    Ok(Html(template.render()?))
}

async fn login_submit(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Result<impl IntoResponse, AppError> {
    let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE invite_code = ?")
        .bind(&form.invite_code)
        .fetch_optional(&state.db)
        .await?;

    match user {
        Some(user) => {
            session.cycle_id().await?;
            login_user(&session, &user).await?;
            Ok(Redirect::to("/").into_response())
        }
        None => {
            let template = LoginTemplate {
                error: Some("Invalid invite code".to_string()),
                legacy_login_available: legacy_login_available(&state).await?,
                static_hash: crate::STATIC_HASH,
                user: None,
            };
            Ok(Html(template.render()?).into_response())
        }
    }
}

async fn github_login_start(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    let attempt = OAuthAttempt::new(OAuthPurpose::Login, chrono::Utc::now());
    store_oauth_attempt(&session, &attempt).await?;
    let callback_url = github_callback_url(&state);
    let authorization_url = match state.auth.github.authorization_url(
        &callback_url,
        &attempt.state,
        &attempt.pkce_challenge(),
    ) {
        Ok(url) => url,
        Err(_) => return render_auth_error(GITHUB_SIGN_IN_ERROR),
    };

    Ok(Redirect::to(authorization_url.as_str()).into_response())
}

async fn github_callback(
    State(state): State<AppState>,
    session: Session,
    Query(query): Query<GitHubCallbackQuery>,
) -> Result<impl IntoResponse, AppError> {
    let Some(attempt) = take_oauth_attempt(&session).await? else {
        return render_auth_error(GITHUB_SIGN_IN_ERROR);
    };
    let is_valid_attempt = attempt.purpose == OAuthPurpose::Login
        && attempt.state == query.state
        && attempt.expires_at > chrono::Utc::now().timestamp();
    if !is_valid_attempt {
        return render_auth_error(GITHUB_SIGN_IN_ERROR);
    }

    let profile = match state
        .auth
        .github
        .exchange_code(
            &github_callback_url(&state),
            &query.code,
            &attempt.pkce_verifier,
        )
        .await
    {
        Ok(profile) => profile,
        Err(_) => return render_auth_error(GITHUB_SIGN_IN_ERROR),
    };

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE users SET github_login = ?, updated_at = ? WHERE github_user_id = ?")
        .bind(&profile.login)
        .bind(&now)
        .bind(&profile.user_id)
        .execute(&state.db)
        .await?;

    let mut user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE github_user_id = ?")
        .bind(&profile.user_id)
        .fetch_optional(&state.db)
        .await?;

    if user.is_none() {
        if state.auth.config.signup_mode == SignupMode::Closed {
            return render_auth_error(CLOSED_SIGNUP_ERROR);
        }

        let user_id = uuid::Uuid::new_v4().to_string();
        let display_name = profile
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&profile.login);
        sqlx::query(
            "INSERT INTO users (\
                 id, name, email, invite_code, github_user_id, github_login, created_at, updated_at\
             ) VALUES (?, ?, NULL, NULL, ?, ?, ?, ?) \
             ON CONFLICT(github_user_id) DO NOTHING",
        )
        .bind(&user_id)
        .bind(display_name)
        .bind(&profile.user_id)
        .bind(&profile.login)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;

        user = sqlx::query_as("SELECT * FROM users WHERE github_user_id = ?")
            .bind(&profile.user_id)
            .fetch_optional(&state.db)
            .await?;
    }

    let Some(user) = user else {
        return render_auth_error(GITHUB_SIGN_IN_ERROR);
    };
    session.cycle_id().await?;
    login_user(&session, &user).await?;
    Ok(Redirect::to("/").into_response())
}

async fn legacy_login_available(state: &AppState) -> Result<bool, AppError> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS(\
             SELECT 1 FROM users \
             WHERE invite_code IS NOT NULL AND github_user_id IS NULL\
         )",
    )
    .fetch_one(&state.db)
    .await?)
}

fn github_callback_url(state: &AppState) -> url::Url {
    state
        .auth
        .config
        .public_base_url
        .join("auth/github/callback")
        .expect("validated public base URL should accept an OAuth callback path")
}

fn render_auth_error(message: &str) -> Result<axum::response::Response, AppError> {
    let template = AuthErrorTemplate {
        message,
        static_hash: crate::STATIC_HASH,
        user: None,
    };
    Ok(Html(template.render()?).into_response())
}

async fn logout(session: Session) -> Result<impl IntoResponse, AppError> {
    logout_user(&session).await?;
    Ok(Redirect::to("/login"))
}
