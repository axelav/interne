-- Users
CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT UNIQUE,
    invite_code TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Collections
CREATE TABLE collections (
    id TEXT PRIMARY KEY,
    owner_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    invite_code TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Collection members
CREATE TABLE collection_members (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    joined_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (collection_id, user_id)
);

-- Entries
CREATE TABLE entries (
    id TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    collection_id TEXT REFERENCES collections(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    title TEXT NOT NULL,
    description TEXT,
    duration INTEGER NOT NULL,
    interval TEXT NOT NULL CHECK (interval IN ('hours', 'days', 'weeks', 'months', 'years')),
    dismissed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Visits
CREATE TABLE visits (
    id TEXT PRIMARY KEY,
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    visited_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Tags
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Entry tags
CREATE TABLE entry_tags (
    entry_id TEXT NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (entry_id, tag_id)
);

-- Indexes
CREATE INDEX idx_entries_user_id ON entries(user_id);
CREATE INDEX idx_entries_collection_id ON entries(collection_id);
CREATE INDEX idx_entries_dismissed_at ON entries(dismissed_at);
CREATE INDEX idx_entry_tags_tag_id ON entry_tags(tag_id);
CREATE INDEX idx_visits_entry_id ON visits(entry_id);
CREATE INDEX idx_collection_members_user_id ON collection_members(user_id);
