# High Priority Fixes Plan

Based on code review findings #3, #4, #5, #6.

## Task 1: Dead Code Cleanup

### What
Remove all dead code identified in the code review.

### Items to remove

**Model constructors (keep `Collection::new` and `CollectionMember::new` — they ARE used):**
- `Tag::new()` in `src/models/tag.rs` — never called, tags created via inline SQL. Remove the `impl Tag` block and unused imports (`chrono::Utc`, `uuid::Uuid`).
- `User::new()` in `src/models/user.rs` — never called, users created via inline SQL in CLI. Remove the `impl User` block and unused imports (`chrono::Utc`, `uuid::Uuid`, `DateTime`).

**Structs:**
- `EntryTag` in `src/models/tag.rs` — never constructed anywhere. Remove it entirely.

**Functions:**
- `get_current_user()` in `src/auth.rs` — never called. Remove it.

**Fields:**
- `user_name` in `EntryListTemplate` (`src/routes/entries.rs:24`) — set but never read in template. Remove the field and the assignments in `list_entries` and `list_all_entries`.

**Cargo.toml:**
- Remove `argon2 = "0.5"` — not imported anywhere.
- Remove `uuid` feature `serde` — `Uuid` is never serialized directly.
- Remove `sqlx` features `uuid` and `chrono` — UUIDs and dates are passed as strings.

**Templates:**
- Remove `<input type="hidden" name="_method" value="PUT">` from `templates/entries/form.html:13` — no server-side method override middleware exists.

**CLI:**
- Remove `#[allow(dead_code)]` from `LegacyEntry.id` in `src/cli.rs:35` — instead, rename to `_id` to idiomatically suppress the warning.

### Models re-export
- Remove `Tag` from `pub use tag::Tag;` in `src/models/mod.rs` if `Tag` is no longer used externally. Check if it's used in routes first.

### Verification
- `cargo check` passes with fewer warnings than before
- `cargo clippy` clean (or only pre-existing non-dead-code warnings)

---

## Task 2: Server-Side Input Validation

### What
Add server-side validation for entry forms before database insertion. Return the form with error messages on validation failure.

### Details

**In `src/routes/entries.rs`, add validation in `create_entry` and `update_entry`:**

```rust
fn validate_entry_form(form: &EntryForm) -> HashMap<String, String> {
    let mut errors = HashMap::new();

    if form.duration < 1 {
        errors.insert("duration".to_string(), "Duration must be at least 1".to_string());
    }

    if !form.url.is_empty() {
        if !form.url.starts_with("http://") && !form.url.starts_with("https://") {
            errors.insert("url".to_string(), "URL must start with http:// or https://".to_string());
        }
    }

    if form.title.trim().is_empty() {
        errors.insert("title".to_string(), "Title is required".to_string());
    }

    if form.title.len() > 500 {
        errors.insert("title".to_string(), "Title must be under 500 characters".to_string());
    }

    errors
}
```

**In `create_entry` and `update_entry`, validate before INSERT/UPDATE:**
```rust
let errors = validate_entry_form(&form);
if !errors.is_empty() {
    // Re-fetch collections for the form
    let collections = ...; // same query as new_entry_form
    let template = EntryFormTemplate {
        entry: None, // or Some(entry) for update
        collections,
        tags_string: form.tags.unwrap_or_default(),
        errors,
        user: Some(user),
    };
    return Ok(Html(template.render()?).into_response());
}
```

**In `src/routes/collections.rs`, validate `CollectionForm`:**
- Validate that `name` is not empty/whitespace.
- Validate `name.len() <= 100`.

**Add `CHECK (duration > 0)` to the SQL schema:**
- Create a new migration `migrations/002_add_duration_check.sql` that does nothing if using SQLite (CHECK constraints can't be added via ALTER TABLE in SQLite). Instead, rely on server-side validation. Document this in a comment.

### Verification
- `cargo check` passes
- Submitting a form with `duration=0` or `duration=-1` shows an error instead of inserting
- Submitting a form with a `javascript:` URL shows an error

---

## Task 3: Fix N+1 Query in Export

### What
Replace the N+1 query pattern in `src/routes/export.rs` with a single batch query using `GROUP_CONCAT`.

### Details

**Current code (N+1):**
```rust
for entry in entries {
    let tags: Vec<(String,)> = sqlx::query_as(
        "SELECT t.name FROM tags t JOIN entry_tags et ON et.tag_id = t.id WHERE et.entry_id = ?"
    )
    .bind(&entry.id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    // ...
}
```

**Replace with batch approach using GROUP_CONCAT:**
```rust
// Single query gets all entries with their tags concatenated
let rows: Vec<EntryWithTags> = sqlx::query_as(
    r#"
    SELECT e.*, GROUP_CONCAT(t.name) as tags
    FROM entries e
    LEFT JOIN entry_tags et ON et.entry_id = e.id
    LEFT JOIN tags t ON t.id = et.tag_id
    WHERE e.user_id = ?
    GROUP BY e.id
    ORDER BY e.created_at
    "#
)
.bind(&user.id)
.fetch_all(&state.db)
.await
.unwrap_or_default();
```

**Add a helper struct:**
```rust
#[derive(FromRow)]
struct EntryWithTags {
    id: String,
    user_id: String,
    collection_id: Option<String>,
    url: String,
    title: String,
    description: Option<String>,
    duration: i64,
    interval: Interval,
    dismissed_at: Option<String>,
    created_at: String,
    updated_at: String,
    tags: Option<String>,  // GROUP_CONCAT result, comma-separated or NULL
}
```

**Convert to ExportEntry:**
```rust
let export_entries: Vec<ExportEntry> = rows.into_iter().map(|row| {
    let tags = row.tags
        .map(|t| t.split(',').map(|s| s.to_string()).collect())
        .unwrap_or_default();
    ExportEntry {
        id: row.id,
        url: row.url,
        title: row.title,
        description: row.description,
        duration: row.duration,
        interval: row.interval,
        dismissed_at: row.dismissed_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
        tags,
    }
}).collect();
```

### Verification
- `cargo check` passes
- Export endpoint returns the same JSON structure as before (same fields, same data)
- Only 1 query instead of N+1

---

## Task 4: Make CLI Import Transactional

### What
Wrap the import loop in `src/cli.rs` in a SQLite transaction so partial failures don't leave orphaned data.

### Details

**Current code:**
```rust
for entry in entries {
    sqlx::query(...).execute(pool).await?;
    // If this fails at entry 50, entries 1-49 are committed
}
```

**Replace with transaction:**
```rust
let mut tx = pool.begin().await?;

for entry in entries {
    // ... same logic but use &mut *tx instead of pool ...
    sqlx::query(...).execute(&mut *tx).await?;

    if let Some(visited) = entry.visited {
        for _ in 0..visited {
            sqlx::query(...).execute(&mut *tx).await?;
        }
    }

    imported += 1;
}

tx.commit().await?;
```

**Also validate interval before insertion** (already done in previous task — the match to `Interval` enum handles this).

### Verification
- `cargo check` passes
- Import is atomic — either all entries import or none do

---

## Execution Order

1. Task 1 (Dead Code) — no dependencies, clean slate
2. Task 2 (Validation) — touches entries.rs forms
3. Task 3 (N+1 Export) — independent, touches export.rs
4. Task 4 (Transactional Import) — independent, touches cli.rs

Tasks 3 and 4 are independent and could be parallelized, but we'll do them sequentially to avoid merge conflicts in the subagent workflow.
