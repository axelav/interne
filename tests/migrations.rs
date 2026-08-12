use sqlx::{AssertSqlSafe, Connection, Row, SqliteConnection};

#[tokio::test]
async fn github_auth_migration_preserves_users_and_related_data() {
    let mut db = SqliteConnection::connect("sqlite::memory:").await.unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/001_initial.sql"))
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::raw_sql(include_str!("../migrations/002_timestamps.sql"))
        .execute(&mut db)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO users (id, name, invite_code, created_at, updated_at) VALUES ('u1', 'Axel', 'legacy', '2025-01-02T03:04:05+00:00', '2026-06-07T08:09:10+00:00')",
    )
    .execute(&mut db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO entries (id, user_id, url, title, duration, interval) VALUES ('e1', 'u1', 'https://example.com', 'Example', 1, 'days')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO visits (id, entry_id, user_id) VALUES ('v1', 'e1', 'u1')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO collections (id, owner_id, name, invite_code) VALUES ('c1', 'u1', 'Reading', 'collection')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO collection_members (collection_id, user_id) VALUES ('c1', 'u1')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO tags (id, name) VALUES ('t1', 'rust')")
        .execute(&mut db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO entry_tags (entry_id, tag_id) VALUES ('e1', 't1')")
        .execute(&mut db)
        .await
        .unwrap();

    sqlx::raw_sql(include_str!("../migrations/003_github_auth.sql"))
        .execute(&mut db)
        .await
        .unwrap();

    let user = sqlx::query(
        "SELECT invite_code, github_user_id, auth_version, created_at, updated_at FROM users WHERE id = 'u1'",
    )
    .fetch_one(&mut db)
    .await
    .unwrap();
    assert_eq!(
        user.get::<Option<String>, _>("invite_code").as_deref(),
        Some("legacy")
    );
    assert_eq!(user.get::<Option<String>, _>("github_user_id"), None);
    assert_eq!(user.get::<i64, _>("auth_version"), 1);
    assert_eq!(
        user.get::<String, _>("created_at"),
        "2025-01-02T03:04:05+00:00"
    );
    assert_eq!(
        user.get::<String, _>("updated_at"),
        "2026-06-07T08:09:10+00:00"
    );

    for table in [
        "entries",
        "visits",
        "collections",
        "collection_members",
        "tags",
        "entry_tags",
    ] {
        let count: i64 = sqlx::query_scalar(AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}")))
            .fetch_one(&mut db)
            .await
            .unwrap();
        assert_eq!(count, 1, "{table} rows must survive the users rebuild");
    }

    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(&mut db)
        .await
        .unwrap();
    assert!(violations.is_empty());

    let foreign_keys_enabled: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut db)
        .await
        .unwrap();
    assert_eq!(foreign_keys_enabled, 1);

    let orphan_result = sqlx::query(
        "INSERT INTO visits (id, entry_id, user_id) VALUES ('v-orphan', 'e1', 'missing-user')",
    )
    .execute(&mut db)
    .await;
    assert!(
        orphan_result.is_err(),
        "foreign keys must reject orphan rows"
    );
}
