use askama::Template;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use tower_sessions::Session;

use crate::auth::{login_user, logout_user};
use crate::error::AppError;
use crate::models::User;
use crate::AppState;

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,

    user: Option<User>,
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

async fn login_page() -> Result<impl IntoResponse, AppError> {
    let template = LoginTemplate {
        error: None,

        user: None,
    };
    Ok(Html(template.render()?))
}

async fn login_submit(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Result<impl IntoResponse, AppError> {
    let user: Option<User> = sqlx::query_as(
        "SELECT * FROM users WHERE invite_code = ?"
    )
    .bind(&form.invite_code)
    .fetch_optional(&state.db)
    .await?;

    match user {
        Some(user) => {
            login_user(&session, user).await?;
            Ok(Redirect::to("/").into_response())
        }
        None => {
            let template = LoginTemplate {
                error: Some("Invalid invite code".to_string()),

                user: None,
            };
            Ok(Html(template.render()?).into_response())
        }
    }
}

async fn logout(session: Session) -> Result<impl IntoResponse, AppError> {
    logout_user(&session).await?;
    Ok(Redirect::to("/login"))
}
