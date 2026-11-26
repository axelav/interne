use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use interne::AuthServices;
use interne::config::ServerAuthConfig;
use interne::github::GitHubOAuthClient;
use interne::suggestions::LinkSuggestions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let log_filter =
        parse_log_filter(env::var("RUST_LOG").ok().as_deref()).unwrap_or_else(|error| {
            eprintln!("Logging configuration error: {error}");
            std::process::exit(1);
        });
    install_logging(
        tracing_subscriber::fmt()
            .with_env_filter(log_filter)
            .finish(),
    );

    let args: Vec<String> = env::args().collect();

    let database_url =
        env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:data/interne.db".to_string());

    let pool = interne::db::init_pool(&database_url).await;

    // Handle CLI commands
    if args.len() > 1 {
        match args[1].as_str() {
            "import" => {
                if args.len() < 4 {
                    eprintln!("Usage: interne import <file.json> <user_id>");
                    std::process::exit(1);
                }
                if let Err(e) = interne::cli::import_data(&pool, &args[2], &args[3]).await {
                    eprintln!("Import failed: {}", e);
                    std::process::exit(1);
                }
                return;
            }
            "create-user" => {
                if !(3..=4).contains(&args.len()) {
                    eprintln!("Usage: interne create-user <name> [email]");
                    std::process::exit(1);
                }
                eprintln!(
                    "Warning: create-user is deprecated and will be removed; email is ignored."
                );
                let public_base_url = connection_base_url_or_exit();
                match interne::cli::invite_user(&pool, &args[2], &public_base_url).await {
                    Ok((user_id, url)) => {
                        println!("User ID: {user_id}");
                        println!("Invitation URL: {url}");
                    }
                    Err(error) => {
                        eprintln!("Failed to invite user: {error}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            "invite-user" => {
                if args.len() != 3 {
                    eprintln!("Usage: interne invite-user <name>");
                    std::process::exit(1);
                }
                let public_base_url = connection_base_url_or_exit();
                match interne::cli::invite_user(&pool, &args[2], &public_base_url).await {
                    Ok((user_id, url)) => {
                        println!("User ID: {user_id}");
                        println!("Invitation URL: {url}");
                    }
                    Err(error) => {
                        eprintln!("Failed to invite user: {error}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            "reset-auth" => {
                if args.len() != 3 {
                    eprintln!("Usage: interne reset-auth <user-id>");
                    std::process::exit(1);
                }
                let public_base_url = connection_base_url_or_exit();
                match interne::cli::reset_user_auth(&pool, &args[2], &public_base_url).await {
                    Ok(url) => println!("Recovery URL: {url}"),
                    Err(error) => {
                        eprintln!("Failed to reset user authentication: {error}");
                        std::process::exit(1);
                    }
                }
                return;
            }
            "help" | "--help" | "-h" => {
                println!("Interne - Spaced repetition for websites");
                println!();
                println!("Usage: interne [command]");
                println!();
                println!("Commands:");
                println!("  (none)              Start the web server");
                println!(
                    "  invite-user <name>       Create an invitation URL (expires in four hours)"
                );
                println!(
                    "  reset-auth <user-id>     Reset auth and create a recovery URL (expires in four hours)"
                );
                println!("  create-user <name> [email]  Deprecated alias for invite-user");
                println!("  import <file> <id>       Import legacy JSON data");
                println!("  help                     Show this help");
                return;
            }
            cmd => {
                eprintln!("Unknown command: {}", cmd);
                eprintln!("Run 'interne help' for usage");
                std::process::exit(1);
            }
        }
    }

    // Start web server
    let secure =
        parse_secure_cookies(env::var("SECURE_COOKIES").ok().as_deref()).unwrap_or_else(|error| {
            eprintln!("Server configuration error: {error}");
            std::process::exit(1);
        });

    let server_auth = ServerAuthConfig::from_env().unwrap_or_else(|error| {
        eprintln!("Authentication configuration error: {error}");
        std::process::exit(1);
    });
    let github = GitHubOAuthClient::new(
        server_auth.github.client_id,
        server_auth.github.client_secret,
    )
    .unwrap_or_else(|error| {
        eprintln!("GitHub client configuration error: {error}");
        std::process::exit(1);
    });
    let auth = AuthServices {
        config: server_auth.auth,
        github: Arc::new(github),
    };
    let suggestions = env::var("OPENAI_API_KEY")
        .ok()
        .filter(|key| !key.trim().is_empty())
        .and_then(|key| match LinkSuggestions::new(key.trim()) {
            Ok(service) => Some(service),
            Err(message) => {
                tracing::warn!(message, "link suggestions unavailable");
                None
            }
        });
    let app = interne::build_app(pool, secure, auth, suggestions).await;

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}

fn connection_base_url_or_exit() -> url::Url {
    interne::cli::connection_base_url_from_lookup(|key| env::var(key).ok()).unwrap_or_else(
        |error| {
            eprintln!("Authentication configuration error: {error}");
            std::process::exit(1);
        },
    )
}

fn parse_secure_cookies(value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(_) => Err("SECURE_COOKIES must be exactly true or false".to_string()),
    }
}

fn parse_log_filter(value: Option<&str>) -> Result<EnvFilter, String> {
    EnvFilter::try_new(value.unwrap_or("info"))
        .map_err(|_| "RUST_LOG must be a valid tracing filter directive".to_string())
}

fn install_logging(subscriber: impl tracing::Subscriber + Send + Sync + 'static) {
    use tracing_log::{AsLog, LogTracer};

    tracing::subscriber::set_global_default(subscriber).expect("install tracing subscriber");
    // html5ever logs raw page tokens before excerpt filtering. Do not allow
    // RUST_LOG directives to include those records in application diagnostics.
    LogTracer::builder()
        .ignore_crate("html5ever")
        .with_max_level(tracing::level_filters::LevelFilter::current().as_log())
        .init()
        .expect("install log bridge");
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use tracing_subscriber::fmt::{MakeWriter, writer::MutexGuardWriter};

    use super::{install_logging, parse_log_filter, parse_secure_cookies};

    #[derive(Default, Clone)]
    struct LogCapture(Arc<Mutex<Vec<u8>>>);

    impl<'a> MakeWriter<'a> for LogCapture {
        type Writer = MutexGuardWriter<'a, Vec<u8>>;

        fn make_writer(&'a self) -> Self::Writer {
            self.0.make_writer()
        }
    }

    #[test]
    fn debug_logging_does_not_record_page_contents() {
        let capture = LogCapture::default();
        install_logging(
            tracing_subscriber::fmt()
                .with_env_filter(
                    parse_log_filter(Some("trace,html5ever::tree_builder=trace")).unwrap(),
                )
                .with_writer(capture.clone())
                .finish(),
        );
        tracing::debug!("unrelated diagnostic");
        tracing_log::log::debug!(target: "fixture", "bridged diagnostic");
        scraper::Html::parse_document("<p hidden>unlisted marker</p>");

        let bytes = capture.0.lock().unwrap();
        let log = std::str::from_utf8(&bytes).unwrap();
        assert!(log.contains("unrelated diagnostic"));
        assert!(log.contains("bridged diagnostic"));
        assert!(!log.contains("html5ever::"), "{log}");
        assert!(!log.contains("unlisted marker"), "{log}");
    }

    #[test]
    fn secure_cookies_default_true_and_accept_only_exact_booleans() {
        assert_eq!(parse_secure_cookies(None), Ok(true));
        assert_eq!(parse_secure_cookies(Some("true")), Ok(true));
        assert_eq!(parse_secure_cookies(Some("false")), Ok(false));

        for invalid in ["TRUE", "False", " true", "false ", "yes", ""] {
            let error = parse_secure_cookies(Some(invalid)).unwrap_err();
            assert!(error.contains("SECURE_COOKIES"));
            assert!(error.contains("true or false"));
        }
    }

    #[test]
    fn rust_log_defaults_to_info_and_rejects_invalid_filters() {
        assert_eq!(parse_log_filter(None).unwrap().to_string(), "info");
        let configured = parse_log_filter(Some("interne=debug,tower_http=warn"))
            .unwrap()
            .to_string();
        assert!(configured.contains("interne=debug"));
        assert!(configured.contains("tower_http=warn"));

        let error = parse_log_filter(Some("interne=[debug")).unwrap_err();
        assert!(error.contains("RUST_LOG"));
        assert!(!error.contains("client-secret"));
    }
}
