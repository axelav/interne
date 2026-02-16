use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::models::User;

const USER_ID_KEY: &str = "user_id";

pub struct AuthUser(pub User);

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
{
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user: Option<User> = session.get(USER_ID_KEY).await.ok().flatten();

        user.map(AuthUser).ok_or(AuthRedirect)
    }
}

pub struct AuthRedirect;

impl IntoResponse for AuthRedirect {
    fn into_response(self) -> Response {
        Redirect::to("/login").into_response()
    }
}

pub async fn login_user(session: &Session, user: User) -> Result<(), tower_sessions::session::Error> {
    session.insert(USER_ID_KEY, user).await
}

pub async fn logout_user(session: &Session) -> Result<(), tower_sessions::session::Error> {
    session.flush().await
}
