use std::env;
use std::net::SocketAddr;
use std::sync::Arc;

use interne::AuthServices;
use interne::config::ServerAuthConfig;
use interne::github::GitHubOAuthClient;
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
    tracing_subscriber::fmt().with_env_filter(log_filter).init();

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
    let app = interne::build_app(pool, secure, auth).await;

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

#[cfg(test)]
mod tests {
    use super::{parse_log_filter, parse_secure_cookies};

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
