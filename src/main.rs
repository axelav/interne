use std::env;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    dotenvy::dotenv().ok();

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
                if args.len() < 3 {
                    eprintln!("Usage: interne create-user <name> [email]");
                    std::process::exit(1);
                }
                let email = args.get(3).map(|s| s.as_str());
                if let Err(e) = interne::cli::create_user(&pool, &args[2], email).await {
                    eprintln!("Failed to create user: {}", e);
                    std::process::exit(1);
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
                println!("  create-user <name>  Create a new user");
                println!("  import <file> <id>  Import legacy JSON data");
                println!("  help                Show this help");
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
        env::var("SECURE_COOKIES").unwrap_or_else(|_| "true".to_string()) == "true";

    let app = interne::build_app(pool, secure).await;

    let addr = SocketAddr::from(([0, 0, 0, 0], 3000));
    let listener = TcpListener::bind(addr).await.unwrap();

    tracing::info!("listening on {}", addr);
    axum::serve(listener, app).await.unwrap();
}
