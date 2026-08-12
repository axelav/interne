use std::{fmt, time::Duration};

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
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

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
    token_url: Url,
    profile_url: Url,
}

impl GitHubOAuthClient {
    pub fn new(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
    ) -> Result<Self, GitHubError> {
        let token_url = Url::parse(TOKEN_URL).map_err(|_| GitHubError::ClientConfiguration)?;
        let profile_url = Url::parse(PROFILE_URL).map_err(|_| GitHubError::ClientConfiguration)?;
        Self::build(
            client_id,
            client_secret,
            token_url,
            profile_url,
            CONNECT_TIMEOUT,
            REQUEST_TIMEOUT,
        )
    }

    fn build(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        token_url: Url,
        profile_url: Url,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self, GitHubError> {
        let http = Client::builder()
            .redirect(Policy::none())
            .connect_timeout(connect_timeout)
            .timeout(request_timeout)
            .build()
            .map_err(|_| GitHubError::ClientConfiguration)?;

        Ok(Self {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
            http,
            token_url,
            profile_url,
        })
    }

    #[cfg(test)]
    fn new_with_endpoints(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        token_url: Url,
        profile_url: Url,
    ) -> Result<Self, GitHubError> {
        Self::build(
            client_id,
            client_secret,
            token_url,
            profile_url,
            CONNECT_TIMEOUT,
            REQUEST_TIMEOUT,
        )
    }

    #[cfg(test)]
    fn new_with_endpoints_and_timeouts(
        client_id: impl Into<String>,
        client_secret: impl Into<String>,
        token_url: Url,
        profile_url: Url,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> Result<Self, GitHubError> {
        Self::build(
            client_id,
            client_secret,
            token_url,
            profile_url,
            connect_timeout,
            request_timeout,
        )
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
            .post(self.token_url.clone())
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
            .get(self.profile_url.clone())
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
    use std::{collections::HashMap, time::Duration};

    use super::{GitHubError, GitHubOAuthClient, GitHubProvider, pkce_challenge};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
        time::timeout,
    };
    use url::Url;

    #[derive(Debug)]
    struct CapturedRequest {
        method: String,
        target: String,
        headers: HashMap<String, String>,
        body: String,
    }

    async fn local_server(responses: Vec<String>) -> (Url, JoinHandle<Vec<CapturedRequest>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let task = tokio::spawn(async move {
            let mut captured = Vec::new();
            for response in responses {
                let Ok(Ok((mut socket, _))) =
                    timeout(Duration::from_millis(500), listener.accept()).await
                else {
                    break;
                };
                captured.push(read_request(&mut socket).await);
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            captured
        });
        (base_url, task)
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> CapturedRequest {
        let mut bytes = Vec::new();
        let header_end = loop {
            let mut chunk = [0; 1024];
            let read = socket.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0, "request ended before headers completed");
            bytes.extend_from_slice(&chunk[..read]);
            if let Some(index) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                break index + 4;
            }
        };

        let header_text = std::str::from_utf8(&bytes[..header_end]).unwrap();
        let mut lines = header_text.split("\r\n");
        let mut request_line = lines.next().unwrap().split_whitespace();
        let method = request_line.next().unwrap().to_string();
        let target = request_line.next().unwrap().to_string();
        let headers: HashMap<_, _> = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        let content_length = headers
            .get("content-length")
            .map(|value| value.parse::<usize>().unwrap())
            .unwrap_or(0);
        while bytes.len() < header_end + content_length {
            let mut chunk = [0; 1024];
            let read = socket.read(&mut chunk).await.unwrap();
            assert_ne!(read, 0, "request ended before body completed");
            bytes.extend_from_slice(&chunk[..read]);
        }

        CapturedRequest {
            method,
            target,
            headers,
            body: String::from_utf8(bytes[header_end..header_end + content_length].to_vec())
                .unwrap(),
        }
    }

    fn response(status: &str, extra_headers: &[(&str, &str)], body: &str) -> String {
        let mut response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in extra_headers {
            response.push_str(&format!("{name}: {value}\r\n"));
        }
        response.push_str("\r\n");
        response.push_str(body);
        response
    }

    fn local_client(base_url: &Url) -> GitHubOAuthClient {
        GitHubOAuthClient::new_with_endpoints(
            "client-id",
            "client-secret",
            base_url.join("token").unwrap(),
            base_url.join("user").unwrap(),
        )
        .unwrap()
    }

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

    #[tokio::test]
    async fn exchange_code_sends_the_required_requests_and_converts_numeric_id() {
        let (base_url, server) = local_server(vec![
            response(
                "200 OK",
                &[("Content-Type", "application/json")],
                r#"{"access_token":"temporary-token"}"#,
            ),
            response(
                "200 OK",
                &[("Content-Type", "application/json")],
                r#"{"id":12345678901234567890,"login":"octocat","name":"The Octocat"}"#,
            ),
        ])
        .await;
        let callback = Url::parse("https://interne.test/auth/github/callback").unwrap();

        let profile = local_client(&base_url)
            .exchange_code(&callback, "authorization-code", "pkce-verifier")
            .await
            .unwrap();
        let requests = server.await.unwrap();

        assert_eq!(
            profile,
            super::GitHubProfile {
                user_id: "12345678901234567890".into(),
                login: "octocat".into(),
                name: Some("The Octocat".into()),
            }
        );
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].target, "/token");
        assert_eq!(
            requests[0].headers.get("accept").unwrap(),
            "application/json"
        );
        let form: HashMap<_, _> = url::form_urlencoded::parse(requests[0].body.as_bytes())
            .into_owned()
            .collect();
        assert_eq!(form.len(), 5);
        assert_eq!(form.get("client_id").unwrap(), "client-id");
        assert_eq!(form.get("client_secret").unwrap(), "client-secret");
        assert_eq!(form.get("code").unwrap(), "authorization-code");
        assert_eq!(form.get("redirect_uri").unwrap(), callback.as_str());
        assert_eq!(form.get("code_verifier").unwrap(), "pkce-verifier");

        assert_eq!(requests[1].method, "GET");
        assert_eq!(requests[1].target, "/user");
        assert_eq!(
            requests[1].headers.get("accept").unwrap(),
            "application/vnd.github+json"
        );
        assert_eq!(
            requests[1].headers.get("authorization").unwrap(),
            "Bearer temporary-token"
        );
        assert_eq!(requests[1].headers.get("user-agent").unwrap(), "interne");
        assert_eq!(
            requests[1].headers.get("x-github-api-version").unwrap(),
            "2022-11-28"
        );
    }

    #[tokio::test]
    async fn token_exchange_refuses_redirects() {
        let (base_url, server) = local_server(vec![
            response("302 Found", &[("Location", "/redirected")], ""),
            response(
                "200 OK",
                &[("Content-Type", "application/json")],
                r#"{"access_token":"redirected-token"}"#,
            ),
        ])
        .await;

        let error = local_client(&base_url)
            .exchange_code(
                &Url::parse("https://interne.test/auth/github/callback").unwrap(),
                "code",
                "verifier",
            )
            .await
            .unwrap_err();
        let requests = server.await.unwrap();

        assert_eq!(error, GitHubError::TokenExchange);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].target, "/token");
    }

    #[tokio::test]
    async fn stalled_token_exchange_returns_safe_stage_error_within_request_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base_url = Url::parse(&format!("http://{}/", listener.local_addr().unwrap())).unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _request = read_request(&mut socket).await;
            tokio::time::sleep(Duration::from_secs(2)).await;
        });
        let client = GitHubOAuthClient::new_with_endpoints_and_timeouts(
            "client-id",
            "client-secret",
            base_url.join("token").unwrap(),
            base_url.join("user").unwrap(),
            Duration::from_millis(100),
            Duration::from_millis(100),
        )
        .unwrap();

        let result = timeout(
            Duration::from_secs(1),
            client.exchange_code(
                &Url::parse("https://interne.test/auth/github/callback").unwrap(),
                "secret-code",
                "secret-verifier",
            ),
        )
        .await
        .expect("the configured request timeout must bound a stalled response");
        server.abort();

        assert_eq!(result.unwrap_err(), GitHubError::TokenExchange);
    }

    #[tokio::test]
    async fn token_status_and_json_failures_are_safe_stage_errors() {
        for response in [
            response("502 Bad Gateway", &[], "hostile-token-status-body"),
            response(
                "200 OK",
                &[("Content-Type", "application/json")],
                "hostile-token-json-body",
            ),
        ] {
            let (base_url, server) = local_server(vec![response]).await;
            let error = local_client(&base_url)
                .exchange_code(
                    &Url::parse("https://interne.test/auth/github/callback").unwrap(),
                    "secret-code",
                    "secret-verifier",
                )
                .await
                .unwrap_err();
            server.await.unwrap();

            assert_eq!(error, GitHubError::TokenExchange);
            let rendered = format!("{error} {error:?}");
            for secret in ["hostile", "secret-code", "secret-verifier"] {
                assert!(!rendered.contains(secret));
            }
        }
    }

    #[tokio::test]
    async fn profile_status_and_json_failures_are_safe_stage_errors() {
        for profile_response in [
            response("502 Bad Gateway", &[], "hostile-profile-status-body"),
            response(
                "200 OK",
                &[("Content-Type", "application/json")],
                "hostile-profile-json-body",
            ),
        ] {
            let (base_url, server) = local_server(vec![
                response(
                    "200 OK",
                    &[("Content-Type", "application/json")],
                    r#"{"access_token":"secret-temporary-token"}"#,
                ),
                profile_response,
            ])
            .await;
            let error = local_client(&base_url)
                .exchange_code(
                    &Url::parse("https://interne.test/auth/github/callback").unwrap(),
                    "secret-code",
                    "secret-verifier",
                )
                .await
                .unwrap_err();
            server.await.unwrap();

            assert_eq!(error, GitHubError::ProfileFetch);
            let rendered = format!("{error} {error:?}");
            for secret in [
                "hostile",
                "secret-temporary-token",
                "secret-code",
                "secret-verifier",
            ] {
                assert!(!rendered.contains(secret));
            }
        }
    }
}
