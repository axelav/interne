use std::collections::HashSet;

use askama::Template;
use axum::{
    Form, Router,
    extract::{RawQuery, State},
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::AppState;
use crate::auth::{
    ConnectionProof, MigrationSession, OAuthAttempt, OAuthPurpose, PendingConnection,
    get_migration_session, get_pending_connection, login_user, logout_user, store_connection_claim,
    store_migration_session, store_oauth_attempt, store_pending_connection, take_connection_claim,
    take_migration_session, take_oauth_attempt, take_pending_connection,
};
use crate::config::SignupMode;
use crate::connection_tokens::{
    ConnectionClaim, ConnectionPurpose, ConnectionTokenError, consume_and_link, validate_claim,
    validate_token,
};
use crate::error::AppError;
use crate::models::User;

const GITHUB_SIGN_IN_ERROR: &str =
    "We couldn’t complete GitHub sign-in. Please return to login and try again.";
const GITHUB_LINK_ERROR: &str =
    "We couldn’t connect this GitHub account. Please return to login and try again.";
const GITHUB_IDENTITY_IN_USE_ERROR: &str = "That GitHub account is already connected to another Interne account. Please return to login and try another account.";
const CONNECTION_TOKEN_ERROR: &str =
    "This invitation or recovery link is invalid or expired. Please request a new link.";
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

#[derive(Template)]
#[template(path = "connect_github.html")]
struct ConnectGitHubTemplate {
    static_hash: &'static str,
    user: Option<User>,
}

#[derive(Template)]
#[template(path = "github_confirm.html")]
struct GitHubConfirmTemplate<'a> {
    github_login: &'a str,
    action_label: &'a str,
    static_hash: &'static str,
    user: Option<User>,
}

#[derive(Deserialize)]
pub struct LoginForm {
    invite_code: String,
}

struct GitHubCallbackQuery {
    code: Option<String>,
    state: Option<String>,
    provider_error: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/recover", get(recovery_entry))
        .route("/login", get(login_page))
        .route("/login", post(login_submit))
        .route("/auth/github", get(github_login_start))
        .route("/auth/github/recover", get(github_recovery_start))
        .route("/auth/connect", get(connect_github_page))
        .route("/auth/github/connect", post(github_link_start))
        .route("/auth/github/callback", get(github_callback))
        .route(
            "/auth/github/confirm",
            get(github_confirm_page).post(github_confirm_submit),
        )
        .route("/logout", post(logout))
}

async fn recovery_entry(
    State(state): State<AppState>,
    session: Session,
    RawQuery(raw_query): RawQuery,
) -> Result<impl IntoResponse, AppError> {
    let Some(plaintext_token) = parse_recovery_token(raw_query.as_deref()) else {
        return render_auth_error(CONNECTION_TOKEN_ERROR);
    };
    let claim = match validate_token(&state.db, &plaintext_token, chrono::Utc::now()).await {
        Ok(claim) => claim,
        Err(_) => return render_auth_error(CONNECTION_TOKEN_ERROR),
    };
    store_connection_claim(&session, &claim).await?;
    Ok(Redirect::to("/auth/github/recover").into_response())
}

async fn github_recovery_start(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    let Some(claim) = take_connection_claim(&session).await? else {
        return render_auth_error(CONNECTION_TOKEN_ERROR);
    };
    if validate_claim(&state.db, &claim, chrono::Utc::now())
        .await
        .is_err()
    {
        return render_auth_error(CONNECTION_TOKEN_ERROR);
    }

    let attempt = OAuthAttempt::new(
        OAuthPurpose::ConnectionToken {
            token_id: claim.token_id,
            user_id: claim.user_id,
            purpose: claim.purpose,
        },
        chrono::Utc::now(),
    );
    store_oauth_attempt(&session, &attempt).await?;
    let callback_url = github_callback_url(&state);
    let authorization_url = match state.auth.github.authorization_url(
        &callback_url,
        &attempt.state,
        &attempt.pkce_challenge(),
    ) {
        Ok(url) => url,
        Err(_) => return render_auth_error(CONNECTION_TOKEN_ERROR),
    };

    Ok(Redirect::to(authorization_url.as_str()).into_response())
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
    let user: Option<User> =
        sqlx::query_as("SELECT * FROM users WHERE invite_code = ? AND github_user_id IS NULL")
            .bind(&form.invite_code)
            .fetch_optional(&state.db)
            .await?;

    match user {
        Some(user) => {
            session.flush().await?;
            session.cycle_id().await?;
            let migration = MigrationSession::new(user.id, chrono::Utc::now());
            store_migration_session(&session, &migration).await?;
            Ok(Redirect::to("/auth/connect").into_response())
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

async fn connect_github_page(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    let Some(_) = live_migration_session(&state, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };

    let template = ConnectGitHubTemplate {
        static_hash: crate::STATIC_HASH,
        user: None,
    };
    Ok(Html(template.render()?).into_response())
}

async fn github_link_start(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    let Some(migration) = live_migration_session(&state, &session).await? else {
        return Ok(Redirect::to("/login").into_response());
    };

    let attempt = OAuthAttempt::new(
        OAuthPurpose::Link {
            user_id: migration.user_id,
        },
        chrono::Utc::now(),
    );
    store_oauth_attempt(&session, &attempt).await?;
    let callback_url = github_callback_url(&state);
    let authorization_url = match state.auth.github.authorization_url(
        &callback_url,
        &attempt.state,
        &attempt.pkce_challenge(),
    ) {
        Ok(url) => url,
        Err(_) => return render_auth_error(GITHUB_LINK_ERROR),
    };

    Ok(Redirect::to(authorization_url.as_str()).into_response())
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
    RawQuery(raw_query): RawQuery,
) -> Result<impl IntoResponse, AppError> {
    let Some(attempt) = take_oauth_attempt(&session).await? else {
        return render_auth_error(GITHUB_SIGN_IN_ERROR);
    };
    let callback_error = match &attempt.purpose {
        OAuthPurpose::Login => GITHUB_SIGN_IN_ERROR,
        OAuthPurpose::Link { .. } => GITHUB_LINK_ERROR,
        OAuthPurpose::ConnectionToken { .. } => CONNECTION_TOKEN_ERROR,
    };
    let Some(query) = parse_github_callback_query(raw_query.as_deref()) else {
        return render_auth_error(callback_error);
    };
    if query.provider_error {
        return render_auth_error(callback_error);
    }
    let (Some(code), Some(callback_state)) = (query.code.as_deref(), query.state.as_deref()) else {
        return render_auth_error(callback_error);
    };
    let is_valid_attempt = !code.is_empty()
        && attempt.state == callback_state
        && attempt.expires_at > chrono::Utc::now().timestamp();
    if !is_valid_attempt {
        return render_auth_error(callback_error);
    }
    if let OAuthPurpose::Link { user_id } = &attempt.purpose {
        let Some(migration) = live_migration_session(&state, &session).await? else {
            return render_auth_error(callback_error);
        };
        if migration.user_id != *user_id {
            return render_auth_error(callback_error);
        }
    }
    if let OAuthPurpose::ConnectionToken {
        token_id,
        user_id,
        purpose,
    } = &attempt.purpose
    {
        let claim = ConnectionClaim {
            token_id: token_id.clone(),
            user_id: user_id.clone(),
            purpose: *purpose,
        };
        if validate_claim(&state.db, &claim, chrono::Utc::now())
            .await
            .is_err()
        {
            return render_auth_error(callback_error);
        }
    }

    let profile = match state
        .auth
        .github
        .exchange_code(&github_callback_url(&state), code, &attempt.pkce_verifier)
        .await
    {
        Ok(profile) => profile,
        Err(_) => return render_auth_error(callback_error),
    };

    match attempt.purpose {
        OAuthPurpose::Login => complete_github_login(&state, &session, profile).await,
        OAuthPurpose::Link { user_id } => {
            let Some(migration) = live_migration_session(&state, &session).await? else {
                return render_auth_error(GITHUB_LINK_ERROR);
            };
            if migration.user_id != user_id {
                return render_auth_error(GITHUB_LINK_ERROR);
            }

            let pending = PendingConnection::new(
                user_id,
                profile,
                ConnectionProof::LegacyInvite,
                chrono::Utc::now(),
            );
            store_pending_connection(&session, &pending).await?;
            Ok(Redirect::to("/auth/github/confirm").into_response())
        }
        OAuthPurpose::ConnectionToken {
            token_id,
            user_id,
            purpose,
        } => {
            let pending = PendingConnection::new(
                user_id,
                profile,
                ConnectionProof::Token { token_id, purpose },
                chrono::Utc::now(),
            );
            store_pending_connection(&session, &pending).await?;
            Ok(Redirect::to("/auth/github/confirm").into_response())
        }
    }
}

async fn complete_github_login(
    state: &AppState,
    session: &Session,
    profile: crate::github::GitHubProfile,
) -> Result<axum::response::Response, AppError> {
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
    session.flush().await?;
    session.cycle_id().await?;
    login_user(session, &user).await?;
    Ok(Redirect::to("/").into_response())
}

async fn github_confirm_page(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    let Some(pending) = get_pending_connection(&session).await? else {
        return render_auth_error(GITHUB_LINK_ERROR);
    };
    if pending.expires_at <= chrono::Utc::now().timestamp()
        || !pending_connection_is_live(&state, &session, &pending).await?
    {
        take_pending_connection(&session).await?;
        return render_auth_error(connection_error(&pending.proof));
    }

    let template = GitHubConfirmTemplate {
        github_login: &pending.github_profile.login,
        action_label: confirmation_action(&pending.proof),
        static_hash: crate::STATIC_HASH,
        user: None,
    };
    Ok(Html(template.render()?).into_response())
}

async fn github_confirm_submit(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    let Some(pending) = take_pending_connection(&session).await? else {
        return render_auth_error(GITHUB_LINK_ERROR);
    };
    if pending.expires_at <= chrono::Utc::now().timestamp()
        || !pending_connection_is_live(&state, &session, &pending).await?
    {
        return render_auth_error(connection_error(&pending.proof));
    }
    let user = match &pending.proof {
        ConnectionProof::LegacyInvite => match complete_legacy_connection(&state, &pending).await {
            Ok(user) => user,
            Err(ConnectionTokenError::GitHubIdentityInUse) => {
                return render_auth_error(GITHUB_IDENTITY_IN_USE_ERROR);
            }
            Err(ConnectionTokenError::InvalidToken | ConnectionTokenError::UserNotFound) => {
                return render_auth_error(GITHUB_LINK_ERROR);
            }
            Err(ConnectionTokenError::Database(error)) => return Err(error.into()),
        },
        ConnectionProof::Token { token_id, purpose } => {
            let claim = ConnectionClaim {
                token_id: token_id.clone(),
                user_id: pending.user_id.clone(),
                purpose: *purpose,
            };
            match consume_and_link(
                &state.db,
                &claim,
                &pending.github_profile,
                chrono::Utc::now(),
            )
            .await
            {
                Ok(user) => user,
                Err(ConnectionTokenError::GitHubIdentityInUse) => {
                    return render_auth_error(GITHUB_IDENTITY_IN_USE_ERROR);
                }
                Err(_) => return render_auth_error(CONNECTION_TOKEN_ERROR),
            }
        }
    };
    session.flush().await?;
    session.cycle_id().await?;
    login_user(&session, &user).await?;
    Ok(Redirect::to("/").into_response())
}

async fn complete_legacy_connection(
    state: &AppState,
    pending: &PendingConnection,
) -> Result<User, ConnectionTokenError> {
    let now = crate::connection_tokens::database_timestamp(chrono::Utc::now());
    let mut transaction = state.db.begin().await?;
    let update_result = sqlx::query(
        "UPDATE users \
         SET github_user_id = ?, github_login = ?, invite_code = NULL, \
             auth_version = auth_version + 1, updated_at = ? \
         WHERE id = ? AND invite_code IS NOT NULL AND github_user_id IS NULL",
    )
    .bind(&pending.github_profile.user_id)
    .bind(&pending.github_profile.login)
    .bind(&now)
    .bind(&pending.user_id)
    .execute(&mut *transaction)
    .await;

    let update = match update_result {
        Ok(update) => update,
        Err(error) if is_unique_violation(&error) => {
            return Err(ConnectionTokenError::GitHubIdentityInUse);
        }
        Err(error) => return Err(error.into()),
    };
    if update.rows_affected() != 1 {
        return Err(ConnectionTokenError::InvalidToken);
    }

    sqlx::query(
        "UPDATE auth_connection_tokens SET consumed_at = ? \
         WHERE user_id = ? AND consumed_at IS NULL AND expires_at > ?",
    )
    .bind(&now)
    .bind(&pending.user_id)
    .bind(&now)
    .execute(&mut *transaction)
    .await?;
    let user: User = sqlx::query_as("SELECT * FROM users WHERE id = ?")
        .bind(&pending.user_id)
        .fetch_one(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(user)
}

async fn live_migration_session(
    state: &AppState,
    session: &Session,
) -> Result<Option<MigrationSession>, AppError> {
    let Some(migration) = get_migration_session(session).await? else {
        return Ok(None);
    };
    let user_is_linkable: i64 = sqlx::query_scalar(
        "SELECT EXISTS(\
             SELECT 1 FROM users \
             WHERE id = ? AND invite_code IS NOT NULL AND github_user_id IS NULL\
         )",
    )
    .bind(&migration.user_id)
    .fetch_one(&state.db)
    .await?;
    if migration.expires_at <= chrono::Utc::now().timestamp() || user_is_linkable == 0 {
        take_migration_session(session).await?;
        return Ok(None);
    }

    Ok(Some(migration))
}

async fn pending_connection_is_live(
    state: &AppState,
    session: &Session,
    pending: &PendingConnection,
) -> Result<bool, AppError> {
    match &pending.proof {
        ConnectionProof::LegacyInvite => {
            let migration = live_migration_session(state, session).await?;
            Ok(migration.is_some_and(|migration| migration.user_id == pending.user_id))
        }
        ConnectionProof::Token { token_id, purpose } => {
            let claim = ConnectionClaim {
                token_id: token_id.clone(),
                user_id: pending.user_id.clone(),
                purpose: *purpose,
            };
            Ok(validate_claim(&state.db, &claim, chrono::Utc::now())
                .await
                .is_ok())
        }
    }
}

fn connection_error(proof: &ConnectionProof) -> &'static str {
    match proof {
        ConnectionProof::LegacyInvite => GITHUB_LINK_ERROR,
        ConnectionProof::Token { .. } => CONNECTION_TOKEN_ERROR,
    }
}

fn confirmation_action(proof: &ConnectionProof) -> &'static str {
    match proof {
        ConnectionProof::LegacyInvite => "Connect GitHub",
        ConnectionProof::Token {
            purpose: ConnectionPurpose::Invite,
            ..
        } => "Accept invitation",
        ConnectionProof::Token {
            purpose: ConnectionPurpose::Recovery,
            ..
        } => "Recover account",
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|error| error.is_unique_violation())
}

fn parse_github_callback_query(raw_query: Option<&str>) -> Option<GitHubCallbackQuery> {
    let raw_query = raw_query?;
    if !has_valid_percent_encoding(raw_query) {
        return None;
    }

    let mut seen_keys = HashSet::new();
    let mut query = GitHubCallbackQuery {
        code: None,
        state: None,
        provider_error: false,
    };
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}')
            || value.contains('\u{fffd}')
            || !seen_keys.insert(key.to_string())
        {
            return None;
        }
        match key.as_ref() {
            "code" => query.code = Some(value.into_owned()),
            "state" => query.state = Some(value.into_owned()),
            "error" => query.provider_error = true,
            _ => {}
        }
    }

    Some(query)
}

fn has_valid_percent_encoding(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn parse_recovery_token(raw_query: Option<&str>) -> Option<String> {
    let raw_query = raw_query?;
    if !has_valid_percent_encoding(raw_query) {
        return None;
    }

    let mut token = None;
    let mut seen_keys = HashSet::new();
    for (key, value) in url::form_urlencoded::parse(raw_query.as_bytes()) {
        if key.contains('\u{fffd}')
            || value.contains('\u{fffd}')
            || !seen_keys.insert(key.to_string())
        {
            return None;
        }
        if key == "token" {
            token = Some(value.into_owned());
        }
    }
    token
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
