use std::fmt;

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use reqwest::{
    Client,
    header::{ACCEPT, USER_AGENT},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use sha2::{Digest, Sha256};
use url::Url;

const AUTHORIZE_URL: &str = "https://github.com/login/oauth/authorize";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const PROFILE_URL: &str = "https://api.github.com/user";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubProfile {
    pub user_id: String,
    pub login: String,
    pub name: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitHubError {
    ClientConfiguration,
    AuthorizationUrl,
    TokenExchange,
    ProfileFetch,
}

impl fmt::Display for GitHubError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stage = match self {
            Self::ClientConfiguration => "GitHub client configuration failed",
            Self::AuthorizationUrl => "GitHub authorization URL construction failed",
            Self::TokenExchange => "GitHub token exchange failed",
            Self::ProfileFetch => "GitHub profile fetch failed",
        };
        formatter.write_str(stage)
    }
}

impl std::error::Error for GitHubError {}

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

#[derive(Clone)]
pub struct GitHubOAuthClient {
    client_id: String,
    client_secret: String,
    http: Client,
}

impl GitHubOAuthClient {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Result<Self, GitHubError> {
        let http = Client::builder()
            .redirect(Policy::none())
            .build()
            .map_err(|_| GitHubError::ClientConfiguration)?;

        Ok(Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            http,
        })
    }
}

#[derive(Deserialize)]
struct AccessTokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct GitHubUserResponse {
    id: Number,
    login: String,
    name: Option<String>,
}

#[async_trait]
impl GitHubProvider for GitHubOAuthClient {
    fn authorization_url(
        &self,
        callback_url: &Url,
        state: &str,
        pkce_challenge: &str,
    ) -> Result<Url, GitHubError> {
        let mut url = Url::parse(AUTHORIZE_URL).map_err(|_| GitHubError::AuthorizationUrl)?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", callback_url.as_str())
            .append_pair("state", state)
            .append_pair("code_challenge", pkce_challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url)
    }

    async fn exchange_code(
        &self,
        callback_url: &Url,
        code: &str,
        pkce_verifier: &str,
    ) -> Result<GitHubProfile, GitHubError> {
        let token_response = self
            .http
            .post(TOKEN_URL)
            .header(ACCEPT, "application/json")
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("client_secret", self.client_secret.as_str()),
                ("code", code),
                ("redirect_uri", callback_url.as_str()),
                ("code_verifier", pkce_verifier),
            ])
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| GitHubError::TokenExchange)?
            .json::<AccessTokenResponse>()
            .await
            .map_err(|_| GitHubError::TokenExchange)?;

        let github_user = self
            .http
            .get(PROFILE_URL)
            .header(ACCEPT, "application/vnd.github+json")
            .bearer_auth(&token_response.access_token)
            .header(USER_AGENT, "interne")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()
            .await
            .and_then(reqwest::Response::error_for_status)
            .map_err(|_| GitHubError::ProfileFetch)?
            .json::<GitHubUserResponse>()
            .await
            .map_err(|_| GitHubError::ProfileFetch)?;

        Ok(GitHubProfile {
            user_id: github_user.id.to_string(),
            login: github_user.login,
            name: github_user.name,
        })
    }
}

pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{GitHubError, GitHubOAuthClient, GitHubProvider, pkce_challenge};
    use url::Url;

    #[test]
    fn authorization_url_has_identity_only_parameters() {
        let client = GitHubOAuthClient::new("client-id", "secret").unwrap();
        let callback = Url::parse("https://interne.honkytonk.in/auth/github/callback").unwrap();
        let url = client
            .authorization_url(&callback, "state-value", "challenge-value")
            .unwrap();
        let query: HashMap<_, _> = url.query_pairs().into_owned().collect();

        assert_eq!(
            query.get("client_id").map(String::as_str),
            Some("client-id")
        );
        assert_eq!(
            query.get("redirect_uri").map(String::as_str),
            Some(callback.as_str())
        );
        assert_eq!(query.get("state").map(String::as_str), Some("state-value"));
        assert_eq!(
            query.get("code_challenge").map(String::as_str),
            Some("challenge-value")
        );
        assert_eq!(
            query.get("code_challenge_method").map(String::as_str),
            Some("S256")
        );
        assert!(!query.contains_key("scope"));
    }

    #[test]
    fn pkce_challenge_is_base64url_sha256() {
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn provider_errors_identify_only_the_failed_stage() {
        assert_eq!(
            GitHubError::TokenExchange.to_string(),
            "GitHub token exchange failed"
        );
        assert_eq!(format!("{:?}", GitHubError::ProfileFetch), "ProfileFetch");
    }
}
