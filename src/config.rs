use std::fmt;

use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignupMode {
    Closed,
    Public,
}

#[derive(Clone, Debug)]
pub struct AuthConfig {
    pub public_base_url: Url,
    pub signup_mode: SignupMode,
}

pub struct GitHubCredentials {
    pub client_id: String,
    pub client_secret: String,
}

pub struct ServerAuthConfig {
    pub auth: AuthConfig,
    pub github: GitHubCredentials,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    MissingVariable(&'static str),
    InvalidPublicBaseUrl,
    InvalidSignupMode,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingVariable(name) => {
                write!(formatter, "missing required environment variable {name}")
            }
            Self::InvalidPublicBaseUrl => write!(
                formatter,
                "PUBLIC_BASE_URL must be a bare HTTPS origin or an HTTP literal loopback origin"
            ),
            Self::InvalidSignupMode => {
                write!(formatter, "GITHUB_SIGNUP_MODE must be closed or public")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

impl fmt::Debug for ServerAuthConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ServerAuthConfig")
            .field("auth", &self.auth)
            .field("github", &"<redacted>")
            .finish()
    }
}

impl AuthConfig {
    pub fn new(public_base_url: &str, signup_mode: SignupMode) -> Result<Self, ConfigError> {
        let original_public_base_url = public_base_url;
        let mut public_base_url =
            Url::parse(public_base_url).map_err(|_| ConfigError::InvalidPublicBaseUrl)?;

        let has_credentials =
            !public_base_url.username().is_empty() || public_base_url.password().is_some();
        let is_bare_origin = !has_credentials
            && public_base_url.host().is_some()
            && public_base_url.path() == "/"
            && public_base_url.query().is_none()
            && public_base_url.fragment().is_none();
        if !is_bare_origin {
            return Err(ConfigError::InvalidPublicBaseUrl);
        }

        let uses_allowed_transport = match public_base_url.scheme() {
            "https" => true,
            "http" => has_literal_loopback_authority(original_public_base_url),
            _ => false,
        };
        if !uses_allowed_transport {
            return Err(ConfigError::InvalidPublicBaseUrl);
        }

        public_base_url.set_path("/");
        Ok(Self {
            public_base_url,
            signup_mode,
        })
    }
}

fn has_literal_loopback_authority(original: &str) -> bool {
    let Some((scheme, remainder)) = original.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") {
        return false;
    }

    let authority = remainder.split(['/', '?', '#']).next().unwrap_or_default();
    authority == "127.0.0.1"
        || authority.starts_with("127.0.0.1:")
        || authority == "[::1]"
        || authority.starts_with("[::1]:")
}

impl ServerAuthConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    pub fn from_lookup<F>(mut lookup: F) -> Result<Self, ConfigError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let client_id =
            lookup("GITHUB_CLIENT_ID").ok_or(ConfigError::MissingVariable("GITHUB_CLIENT_ID"))?;
        let client_secret = lookup("GITHUB_CLIENT_SECRET")
            .ok_or(ConfigError::MissingVariable("GITHUB_CLIENT_SECRET"))?;
        let public_base_url =
            lookup("PUBLIC_BASE_URL").ok_or(ConfigError::MissingVariable("PUBLIC_BASE_URL"))?;
        let signup_mode = match lookup("GITHUB_SIGNUP_MODE").as_deref() {
            None | Some("closed") => SignupMode::Closed,
            Some("public") => SignupMode::Public,
            Some(_) => return Err(ConfigError::InvalidSignupMode),
        };

        Ok(Self {
            auth: AuthConfig::new(&public_base_url, signup_mode)?,
            github: GitHubCredentials {
                client_id,
                client_secret,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthConfig, ServerAuthConfig, SignupMode};

    #[test]
    fn signup_mode_defaults_closed_and_rejects_unknown_values() {
        let closed = ServerAuthConfig::from_lookup(|key| match key {
            "GITHUB_CLIENT_ID" => Some("client".into()),
            "GITHUB_CLIENT_SECRET" => Some("secret".into()),
            "PUBLIC_BASE_URL" => Some("https://interne.honkytonk.in".into()),
            _ => None,
        })
        .unwrap();
        assert_eq!(closed.auth.signup_mode, SignupMode::Closed);

        let error = ServerAuthConfig::from_lookup(|key| match key {
            "GITHUB_CLIENT_ID" => Some("client".into()),
            "GITHUB_CLIENT_SECRET" => Some("secret".into()),
            "PUBLIC_BASE_URL" => Some("https://interne.honkytonk.in".into()),
            "GITHUB_SIGNUP_MODE" => Some("sometimes".into()),
            _ => None,
        })
        .unwrap_err();
        assert!(error.to_string().contains("closed or public"));
    }

    #[test]
    fn public_base_url_requires_https_except_for_literal_loopback() {
        for invalid in [
            "http://interne.honkytonk.in",
            "http://localhost:3000",
            "https://interne.honkytonk.in/path",
        ] {
            let result = AuthConfig::new(invalid, SignupMode::Closed);
            assert!(result.is_err(), "{invalid} must be rejected");
        }
        assert!(AuthConfig::new("http://127.0.0.1:3000", SignupMode::Closed).is_ok());
    }

    #[test]
    fn public_base_url_rejects_components_outside_a_bare_origin() {
        for invalid in [
            "https://user@interne.honkytonk.in",
            "https://interne.honkytonk.in?mode=test",
            "https://interne.honkytonk.in#fragment",
            "ftp://interne.honkytonk.in",
        ] {
            assert!(
                AuthConfig::new(invalid, SignupMode::Closed).is_err(),
                "{invalid} must be rejected"
            );
        }
    }

    #[test]
    fn public_base_url_accepts_ipv6_loopback_and_normalizes_root_path() {
        let config = AuthConfig::new("http://[::1]:3000", SignupMode::Closed).unwrap();

        assert_eq!(config.public_base_url.as_str(), "http://[::1]:3000/");
    }

    #[test]
    fn http_requires_the_exact_loopback_host_literal() {
        for invalid in ["http://127.1:3000", "http://2130706433:3000"] {
            assert!(
                AuthConfig::new(invalid, SignupMode::Closed).is_err(),
                "{invalid} must be rejected"
            );
        }
    }

    #[test]
    fn configuration_names_each_missing_required_variable() {
        for missing in [
            "GITHUB_CLIENT_ID",
            "GITHUB_CLIENT_SECRET",
            "PUBLIC_BASE_URL",
        ] {
            let error = ServerAuthConfig::from_lookup(|key| {
                if key == missing {
                    None
                } else {
                    Some(
                        match key {
                            "GITHUB_CLIENT_ID" => "client",
                            "GITHUB_CLIENT_SECRET" => "secret",
                            "PUBLIC_BASE_URL" => "https://interne.honkytonk.in",
                            _ => return None,
                        }
                        .into(),
                    )
                }
            })
            .expect_err("a required variable is missing");

            assert!(error.to_string().contains(missing));
        }
    }

    #[test]
    fn signup_mode_accepts_public_exactly() {
        let config = ServerAuthConfig::from_lookup(|key| match key {
            "GITHUB_CLIENT_ID" => Some("client".into()),
            "GITHUB_CLIENT_SECRET" => Some("secret".into()),
            "PUBLIC_BASE_URL" => Some("https://interne.honkytonk.in".into()),
            "GITHUB_SIGNUP_MODE" => Some("public".into()),
            _ => None,
        })
        .unwrap();

        assert_eq!(config.auth.signup_mode, SignupMode::Public);
    }
}
