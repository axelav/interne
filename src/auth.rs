use axum::{
    extract::FromRequestParts,
    http::request::Parts,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

use crate::AppState;
use crate::models::User;

const USER_ID_KEY: &str = "user_id";
const AUTH_VERSION_KEY: &str = "auth_version";

fn session_identity(user_id: Option<String>, auth_version: Option<i64>) -> Option<(String, i64)> {
    Some((user_id?, auth_version?))
}

pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthRedirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user_id: Option<String> = session.get(USER_ID_KEY).await.ok().flatten();
        let auth_version: Option<i64> = session.get(AUTH_VERSION_KEY).await.ok().flatten();

        let Some((user_id, auth_version)) = session_identity(user_id, auth_version) else {
            return Err(AuthRedirect);
        };

        let user: Option<User> =
            sqlx::query_as("SELECT * FROM users WHERE id = ? AND auth_version = ?")
                .bind(&user_id)
                .bind(auth_version)
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

pub async fn login_user(
    session: &Session,
    user: &User,
) -> Result<(), tower_sessions::session::Error> {
    session.insert(USER_ID_KEY, &user.id).await?;
    session.insert(AUTH_VERSION_KEY, user.auth_version).await
}

pub async fn logout_user(session: &Session) -> Result<(), tower_sessions::session::Error> {
    session.flush().await
}

#[cfg(test)]
mod tests {
    use super::session_identity;

    #[test]
    fn missing_auth_version_rejects_a_known_user_id() {
        let identity = session_identity(Some("known-user".into()), None);

        assert_eq!(identity, None);
    }
}
