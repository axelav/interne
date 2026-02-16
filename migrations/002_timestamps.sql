-- Normalize any timestamps that used SQLite's datetime('now') format
-- (YYYY-MM-DD HH:MM:SS) to RFC3339 format (YYYY-MM-DDTHH:MM:SS+00:00)
-- so that chrono's DateTime<Utc>.parse() works consistently.

UPDATE users SET created_at = replace(created_at, ' ', 'T') || '+00:00'
    WHERE created_at NOT LIKE '%T%';
UPDATE users SET updated_at = replace(updated_at, ' ', 'T') || '+00:00'
    WHERE updated_at NOT LIKE '%T%';

UPDATE collections SET created_at = replace(created_at, ' ', 'T') || '+00:00'
    WHERE created_at NOT LIKE '%T%';
UPDATE collections SET updated_at = replace(updated_at, ' ', 'T') || '+00:00'
    WHERE updated_at NOT LIKE '%T%';

UPDATE entries SET created_at = replace(created_at, ' ', 'T') || '+00:00'
    WHERE created_at NOT LIKE '%T%';
UPDATE entries SET updated_at = replace(updated_at, ' ', 'T') || '+00:00'
    WHERE updated_at NOT LIKE '%T%';
UPDATE entries SET dismissed_at = replace(dismissed_at, ' ', 'T') || '+00:00'
    WHERE dismissed_at IS NOT NULL AND dismissed_at NOT LIKE '%T%';

UPDATE visits SET visited_at = replace(visited_at, ' ', 'T') || '+00:00'
    WHERE visited_at NOT LIKE '%T%';

UPDATE tags SET created_at = replace(created_at, ' ', 'T') || '+00:00'
    WHERE created_at NOT LIKE '%T%';

UPDATE collection_members SET joined_at = replace(joined_at, ' ', 'T') || '+00:00'
    WHERE joined_at NOT LIKE '%T%';
