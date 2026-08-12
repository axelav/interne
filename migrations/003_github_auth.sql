-- no-transaction
PRAGMA foreign_keys = OFF;
BEGIN IMMEDIATE;

CREATE TABLE new_users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    invite_code TEXT UNIQUE,
    github_user_id TEXT UNIQUE,
    github_login TEXT,
    auth_version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

INSERT INTO new_users (
    id, name, email, invite_code, github_user_id, github_login,
    auth_version, created_at, updated_at
)
SELECT
    id, name, email, invite_code, NULL, NULL,
    1, created_at, updated_at
FROM users;

DROP TABLE users;
ALTER TABLE new_users RENAME TO users;

CREATE TABLE auth_connection_tokens (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL UNIQUE,
    purpose TEXT NOT NULL CHECK (purpose IN ('invite', 'recovery')),
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    consumed_at TEXT
);

CREATE INDEX idx_auth_connection_tokens_user_id
    ON auth_connection_tokens(user_id);
CREATE INDEX idx_auth_connection_tokens_active
    ON auth_connection_tokens(token_hash, expires_at, consumed_at);

COMMIT;
PRAGMA foreign_keys = ON;
