mod common;

use chrono::{Duration, Utc};
use interne::connection_tokens::{
    ConnectionPurpose, ConnectionTokenError, connection_url, issue_invitation, reset_auth,
    validate_token,
};
use std::process::Command;
use std::str::FromStr;
use url::Url;

use common::TestApp;

#[tokio::test]
async fn invitation_is_hashed_single_use_and_expires_in_four_hours() {
    let app = TestApp::new().await;
    let now = Utc::now();
    let issued = issue_invitation(&app.db, "Guest", now).await.unwrap();

    assert_eq!(issued.expires_at, now + Duration::hours(4));
    let stored: String =
        sqlx::query_scalar("SELECT token_hash FROM auth_connection_tokens WHERE user_id = ?")
            .bind(&issued.user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_ne!(stored, issued.plaintext_token);

    let claim = validate_token(&app.db, &issued.plaintext_token, now)
        .await
        .unwrap();
    assert_eq!(claim.user_id, issued.user_id);
    assert_eq!(claim.purpose, ConnectionPurpose::Invite);
}

#[tokio::test]
async fn new_invitation_supersedes_an_older_token() {
    let app = TestApp::new().await;
    let first_now = Utc::now();
    let invitation = issue_invitation(&app.db, "Guest", first_now).await.unwrap();

    let replacement_now = first_now + Duration::minutes(5);
    let replacement = reset_auth(&app.db, &invitation.user_id, replacement_now)
        .await
        .unwrap();

    assert_eq!(replacement.user_id, invitation.user_id);
    let consumed_at: Option<String> = sqlx::query_scalar(
        "SELECT consumed_at FROM auth_connection_tokens WHERE user_id = ? AND purpose = 'invite'",
    )
    .bind(&invitation.user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(
        consumed_at.as_deref(),
        Some(replacement_now.to_rfc3339().as_str())
    );
    assert!(
        validate_token(&app.db, &invitation.plaintext_token, replacement_now)
            .await
            .is_err()
    );
    assert_eq!(
        validate_token(&app.db, &replacement.plaintext_token, replacement_now)
            .await
            .unwrap()
            .purpose,
        ConnectionPurpose::Recovery
    );
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let app = TestApp::new().await;
    let now = Utc::now();
    let issued = issue_invitation(&app.db, "Guest", now).await.unwrap();

    let error = validate_token(&app.db, &issued.plaintext_token, now + Duration::hours(4))
        .await
        .unwrap_err();

    assert!(matches!(error, ConnectionTokenError::InvalidToken));
}

#[tokio::test]
async fn malformed_token_is_rejected_like_an_expired_token() {
    let app = TestApp::new().await;
    let now = Utc::now();
    let issued = issue_invitation(&app.db, "Guest", now).await.unwrap();
    let expired = validate_token(&app.db, &issued.plaintext_token, now + Duration::hours(4))
        .await
        .unwrap_err();

    for malformed in ["short", "!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!"] {
        let error = validate_token(&app.db, malformed, now).await.unwrap_err();
        assert_eq!(error.to_string(), expired.to_string());
        assert_eq!(format!("{error:?}"), format!("{expired:?}"));
    }
}

#[tokio::test]
async fn reset_auth_disconnects_github_and_increments_auth_version() {
    let app = TestApp::new().await;
    let user_id = app.create_github_user("Guest", "12345", "octoguest").await;
    sqlx::query("UPDATE users SET invite_code = 'legacy-code' WHERE id = ?")
        .bind(&user_id)
        .execute(&app.db)
        .await
        .unwrap();
    let now = Utc::now();

    let issued = reset_auth(&app.db, &user_id, now).await.unwrap();

    let user: (Option<String>, Option<String>, Option<String>, i64, String) = sqlx::query_as(
        "SELECT github_user_id, github_login, invite_code, auth_version, updated_at FROM users WHERE id = ?",
    )
    .bind(&user_id)
    .fetch_one(&app.db)
    .await
    .unwrap();
    assert_eq!(user, (None, None, None, 2, now.to_rfc3339()));
    assert_eq!(issued.user_id, user_id);
    assert_eq!(issued.expires_at, now + Duration::hours(4));
    assert_eq!(
        validate_token(&app.db, &issued.plaintext_token, now)
            .await
            .unwrap()
            .purpose,
        ConnectionPurpose::Recovery
    );
}

#[tokio::test]
async fn reset_auth_rejects_an_unknown_user() {
    let app = TestApp::new().await;

    let error = match reset_auth(&app.db, "missing-user", Utc::now()).await {
        Ok(_) => panic!("an unknown user must not receive a recovery token"),
        Err(error) => error,
    };

    assert!(matches!(error, ConnectionTokenError::UserNotFound));
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM auth_connection_tokens")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn connection_url_uses_public_base_and_percent_encoding() {
    let base = Url::parse("https://interne.test/").unwrap();

    let url = connection_url(&base, "a/b? c");

    assert_eq!(
        url.as_str(),
        "https://interne.test/recover?token=a%2Fb%3F+c"
    );
}

#[tokio::test]
async fn plaintext_token_is_never_written_to_sqlite() {
    let app = TestApp::new().await;
    let issued = issue_invitation(&app.db, "Guest", Utc::now())
        .await
        .unwrap();

    let persisted_text: (String, String, String, String, String, Option<String>) =
        sqlx::query_as(
            "SELECT id, user_id, token_hash, purpose, expires_at, consumed_at FROM auth_connection_tokens WHERE user_id = ?",
        )
        .bind(&issued.user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    for value in [
        persisted_text.0.as_str(),
        persisted_text.1.as_str(),
        persisted_text.2.as_str(),
        persisted_text.3.as_str(),
        persisted_text.4.as_str(),
        persisted_text.5.as_deref().unwrap_or_default(),
    ] {
        assert!(!value.contains(&issued.plaintext_token));
    }
}

#[tokio::test]
async fn invitation_rolls_back_user_creation_when_token_insert_fails() {
    let app = TestApp::new().await;
    sqlx::query(
        "CREATE TRIGGER reject_connection_tokens BEFORE INSERT ON auth_connection_tokens BEGIN SELECT RAISE(ABORT, 'rejected'); END",
    )
    .execute(&app.db)
    .await
    .unwrap();

    assert!(
        issue_invitation(&app.db, "Rolled Back", Utc::now())
            .await
            .is_err()
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE name = 'Rolled Back'")
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn reset_auth_rolls_back_disconnection_when_token_insert_fails() {
    let app = TestApp::new().await;
    let user_id = app.create_github_user("Guest", "12345", "octoguest").await;
    sqlx::query(
        "CREATE TRIGGER reject_connection_tokens BEFORE INSERT ON auth_connection_tokens BEGIN SELECT RAISE(ABORT, 'rejected'); END",
    )
    .execute(&app.db)
    .await
    .unwrap();

    assert!(reset_auth(&app.db, &user_id, Utc::now()).await.is_err());

    let user: (Option<String>, Option<String>, i64) =
        sqlx::query_as("SELECT github_user_id, github_login, auth_version FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(&app.db)
            .await
            .unwrap();
    assert_eq!(user, (Some("12345".into()), Some("octoguest".into()), 1));
}

struct CliDatabase {
    path: std::path::PathBuf,
}

impl CliDatabase {
    fn new() -> Self {
        Self {
            path: std::env::temp_dir().join(format!("interne-cli-{}.db", uuid::Uuid::new_v4())),
        }
    }

    fn url(&self) -> String {
        format!("sqlite:{}", self.path.display())
    }
}

impl Drop for CliDatabase {
    fn drop(&mut self) {
        for path in [
            self.path.clone(),
            self.path.with_extension("db-shm"),
            self.path.with_extension("db-wal"),
        ] {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn run_cli(database: &CliDatabase, arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_interne"))
        .args(arguments)
        .env_clear()
        .env("DATABASE_URL", database.url())
        .env("PUBLIC_BASE_URL", "https://interne.test")
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap()
}

#[tokio::test]
async fn invitation_and_reset_commands_do_not_require_github_credentials() {
    let database = CliDatabase::new();
    let invite = run_cli(&database, &["invite-user", "CLI Guest"]);
    assert!(
        invite.status.success(),
        "{}",
        String::from_utf8_lossy(&invite.stderr)
    );
    let stdout = String::from_utf8(invite.stdout).unwrap();
    assert_eq!(stdout.matches("/recover?token=").count(), 1);
    let user_id = stdout
        .lines()
        .find_map(|line| line.strip_prefix("User ID: "))
        .expect("invite command prints the user ID");

    let reset = run_cli(&database, &["reset-auth", user_id]);
    assert!(
        reset.status.success(),
        "{}",
        String::from_utf8_lossy(&reset.stderr)
    );
    let stdout = String::from_utf8(reset.stdout).unwrap();
    assert_eq!(stdout.matches("/recover?token=").count(), 1);
}

#[tokio::test]
async fn deprecated_create_user_alias_ignores_email_and_issues_an_invitation() {
    let database = CliDatabase::new();

    let output = run_cli(
        &database,
        &["create-user", "Legacy Guest", "ignored@example.com"],
    );

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(stdout.matches("/recover?token=").count(), 1);
    assert!(stderr.contains("deprecated"));
    assert!(stderr.contains("email is ignored"));

    let options = sqlx::sqlite::SqliteConnectOptions::from_str(&database.url()).unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let email: Option<String> =
        sqlx::query_scalar("SELECT email FROM users WHERE name = 'Legacy Guest'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(email, None);
}

#[test]
fn cli_help_names_connection_arguments_and_four_hour_expiry() {
    let database = CliDatabase::new();

    let output = run_cli(&database, &["help"]);

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("invite-user <name>"));
    assert!(stdout.contains("reset-auth <user-id>"));
    assert!(stdout.contains("four hours"));
}
