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

async fn login_page() -> impl IntoResponse {
    let template = LoginTemplate {
        error: None,

        user: None,
    };
    Html(template.render().unwrap())
}

async fn login_submit(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> impl IntoResponse {
    let user: Option<User> = sqlx::query_as(
        "SELECT * FROM users WHERE invite_code = ?"
    )
    .bind(&form.invite_code)
    .fetch_optional(&state.db)
    .await
    .unwrap();

    match user {
        Some(user) => {
            login_user(&session, user).await.unwrap();
            Redirect::to("/").into_response()
        }
        None => {
            let template = LoginTemplate {
                error: Some("Invalid invite code".to_string()),
        
                user: None,
            };
            Html(template.render().unwrap()).into_response()
        }
    }
}

async fn logout(session: Session) -> impl IntoResponse {
    logout_user(&session).await.unwrap();
    Redirect::to("/login")
}
