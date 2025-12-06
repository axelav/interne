CREATE TABLE entries (
  id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  user_id TEXT NOT NULL,
  url TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  duration INTEGER NOT NULL,
  interval TEXT NOT NULL CHECK (interval IN ('hours', 'days', 'weeks', 'months', 'years')),
  visited INTEGER DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME,
  dismissed_at DATETIME,
  FOREIGN KEY (user_id) REFERENCES _user(id) ON DELETE CASCADE
);

CREATE INDEX idx_entries_user_id ON entries(user_id);
CREATE INDEX idx_entries_dismissed_at ON entries(dismissed_at);
