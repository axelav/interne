use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PageReadError {
    InvalidUrl,
    RestrictedDestination,
    Timeout,
    Connection,
    Redirect,
    HttpStatus(u16),
    UnsupportedEncoding,
    NotHtml,
    BodyTooLarge,
    BodyRead,
    ParserUnavailable,
    MarkupLimit,
    DomItems,
    DomBytes,
    DomOperations,
    AttributeMerges,
}

impl std::fmt::Display for PageReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for PageReadError {}

impl PageReadError {
    fn response(self) -> Response {
        use PageReadError::*;
        let complexity = "This page is too complex to read for suggestions.";
        let (reason, message) = match self {
            InvalidUrl => ("invalid_url", "Enter a valid public HTTP or HTTPS URL."),
            RestrictedDestination => (
                "restricted_destination",
                "Only public web pages can be read.",
            ),
            Timeout => (
                "timeout",
                "The page took too long to respond. Try again later.",
            ),
            Connection => (
                "connection",
                "Could not connect to the website. Try again later.",
            ),
            Redirect => (
                "redirect",
                "Could not follow the website's redirect. Try the final page URL.",
            ),
            HttpStatus(status) => (
                "http_status",
                match status {
                    401 => "The website requires sign-in. Try a publicly accessible page.",
                    403 => "The website denied access to this page.",
                    404 | 410 => "The website could not find this page. Check the URL.",
                    429 => "The website is limiting requests. Try again later.",
                    500..=599 => "The website reported a server error. Try again later.",
                    _ => "The website did not return a successful response. Check the URL.",
                },
            ),
            UnsupportedEncoding => (
                "unsupported_encoding",
                "The page uses an unsupported encoding.",
            ),
            NotHtml => ("not_html", "This URL does not return an HTML page."),
            BodyTooLarge => (
                "body_too_large",
                "This page is too large to read for suggestions.",
            ),
            BodyRead => (
                "body_read",
                "Could not finish reading the page. Try again later.",
            ),
            ParserUnavailable => (
                "parser_unavailable",
                "The page reader is unavailable. Try again later.",
            ),
            MarkupLimit => ("markup_limit", complexity),
            DomItems => ("dom_items", complexity),
            DomBytes => ("dom_bytes", complexity),
            DomOperations => ("dom_operations", complexity),
            AttributeMerges => ("attribute_merges", complexity),
        };
        let upstream_status = match self {
            HttpStatus(status) => Some(status),
            _ => None,
        };
        tracing::warn!(reason, upstream_status, "Suggestion page read failed");
        (StatusCode::UNPROCESSABLE_ENTITY, message).into_response()
    }
}

#[derive(Debug)]
pub enum AppError {
    Database(sqlx::Error),
    Template(askama::Error),
    Session(tower_sessions::session::Error),
    NotFound,
    SuggestionsUnavailable,
    UnreadablePage(PageReadError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found").into_response(),
            AppError::SuggestionsUnavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "Suggestions are unavailable. You can still fill in the form and save.",
            )
                .into_response(),
            AppError::UnreadablePage(error) => error.response(),
            AppError::Database(e) => {
                tracing::error!("Database error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
            AppError::Template(e) => {
                tracing::error!("Template error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
            AppError::Session(e) => {
                tracing::error!("Session error: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Database(e)
    }
}

impl From<askama::Error> for AppError {
    fn from(e: askama::Error) -> Self {
        AppError::Template(e)
    }
}

impl From<tower_sessions::session::Error> for AppError {
    fn from(e: tower_sessions::session::Error) -> Self {
        AppError::Session(e)
    }
}
