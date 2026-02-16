use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::models::User;
use crate::AppState;

const USER_ID_KEY: &str = "user_id";

pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user_id: Option<String> = session.get(USER_ID_KEY).await.ok().flatten();

        let Some(user_id) = user_id else {
            return Err(AuthRedirect);
        };

        let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AuthRedirect)?;

        user.map(AuthUser).ok_or(AuthRedirect)
    }
}

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::to("/login").into_response()
    }
}

pub async fn login_user(session: &Session, user: &User) -> Result<(), tower_sessions::session::Error> {
    session.insert(USER_ID_KEY, &user.id).await
}

pub async fn logout_user(session: &Session) -> Result<(), tower_sessions::session::Error> {
    session.flush().await
}
