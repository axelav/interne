# Code Review: Changes Since 3eabf21

**Date:** 2026-02-17
**Scope:** 49 files, ~7,900 lines added across major features (tags, filters, testing infra, form validation, cache busting, logging)

## Overall Assessment

The codebase has moved in a clearly positive direction: proper error propagation with `?` replacing `.unwrap()`, real integration tests, form validation, cache busting, and request logging. The test infrastructure is well-designed with the `TestApp` helper pattern. Below are the issues found, organized by severity.

---

## Critical

### C1. `normalize_url().unwrap()` can panic in production

**Files:** `src/routes/entries.rs` (in `create_entry` and `update_entry`)

Both `create_entry` and `update_entry` call `.unwrap()` on `normalize_url()` after validation. The validation checks happen on the untrimmed `form.url` for emptiness, then separately calls `normalize_url`. The code path is safe today because the empty check fires first. However, this `.unwrap()` is fragile -- if validation logic ever changes (or a new URL passes validation but fails normalization), this will be a 500 panic in production.

**Fix:** Store the normalized URL from validation and reuse it, or at minimum replace `.unwrap()` with `.unwrap_or_else(|_| form.url.clone())`.

### C2. Tag input is completely unsanitized -- no length or count limits

**Files:** `src/routes/entries.rs`, `src/cli.rs`

Tag names from the comma-separated `tags` field are only trimmed and lowercased. There is:
- No maximum length check per tag
- No maximum number of tags check
- No character validation

A user could submit a form with `tags=` containing an extremely long string (megabytes) or thousands of comma-separated tags, causing unbounded INSERT loops and database bloat.

**Fix:** Add validation in `validate_entry_form`:
- Maximum 20 tags allowed
- Each tag must be under 50 characters

---

## High

### H1. `leave_collection` does not verify user is actually a member

**File:** `src/routes/collections.rs`

The comment says "Only members can leave (not owners)" but there is no actual check. If the owner calls this endpoint, the DELETE simply does nothing (owners are not in `collection_members`), but this is only correct by accident of the data model. There is no check that the collection exists at all, or that `id` is a valid UUID.

**Fix:** Verify the collection exists and the user is a member (not owner). Return 404 if not.

### H2. `collection_id` in entry form is not authorization-checked

**File:** `src/routes/entries.rs` (in `create_entry` and `update_entry`)

When creating or updating an entry, the `collection_id` from the form is accepted as-is. A user could submit a `collection_id` for a collection they do not own or belong to, effectively placing their entry in someone else's collection and making it visible to that collection's members.

**Fix:** After extracting `collection_id`, verify the user has access:
```sql
SELECT c.id FROM collections c
LEFT JOIN collection_members cm ON cm.collection_id = c.id
WHERE c.id = ? AND (c.owner_id = ? OR cm.user_id = ?)
```

### H3. No upper bound on `duration` field

**File:** `src/routes/entries.rs`

Duration is validated as `>= 1` but has no upper bound. A user can submit `duration=9999999999999999`. When multiplied by 365 for years, this will silently overflow `chrono::Duration::days()` internally (which multiplies by 86,400 seconds).

**Fix:** Add an upper bound: `Duration must be between 1 and 3650`.

### H4. Entry+tag operations are not transactional in web handlers

**File:** `src/routes/entries.rs` (in `create_entry` and `update_entry`)

The entry INSERT/UPDATE and the tag INSERT loop are all separate queries without a transaction. If the entry INSERT succeeds but a tag INSERT fails, the entry exists without its tags, leaving the database in an inconsistent state. This contrasts with `import_data` in `src/cli.rs` which correctly uses a transaction.

**Fix:** Wrap the entry insert + tag operations in `pool.begin()` / `tx.commit()`.

### H5. `DefaultHasher` is not stable across Rust versions

**File:** `build.rs`

The Rust documentation explicitly warns that `DefaultHasher` output is not guaranteed to be stable across Rust versions or platforms. This means rebuilding with a different Rust toolchain could produce a different `STATIC_HASH`, causing unnecessary cache invalidation, or two identical builds producing different hashes.

**Fix:** Use a stable hash like CRC32 (`crc32fast` crate) or truncated SHA-256.

---

## Medium

### M1. 12 remaining `.unwrap_or_default()` on database queries

**Files:** `src/routes/entries.rs`, `src/routes/tags.rs`, `src/routes/export.rs`, `src/routes/collections.rs`

The refactoring converted most `.unwrap()` calls to `?`, which is great, but `.unwrap_or_default()` silently swallows database errors, logging nothing. If a query fails due to a schema issue or a locked database, the user sees an empty list instead of an error.

**Fix:** Replace `.unwrap_or_default()` with `?` and let `AppError::Database` handle logging and the 500 response.

### M2. Tag pages don't include collection-shared entries

**File:** `src/routes/tags.rs`

The tag detail page only shows entries directly owned by the user (`e.user_id = ?`), but entries can also be visible to the user through collection membership. The main entry list in `fetch_entries_for_user` includes the collection membership subquery. The tag views are inconsistent with this.

**Fix:** Match the access pattern from `fetch_entries_for_user`:
```sql
WHERE t.name = ? AND (e.user_id = ? OR e.collection_id IN (
    SELECT collection_id FROM collection_members WHERE user_id = ?
))
```

### M3. `validate_entry_form` can report wrong error for title

**File:** `src/routes/entries.rs`

The empty and too-long checks are sequential `if` statements. A whitespace-only 501-char title would first get "Title is required" inserted, then immediately get it overwritten by "Title must be under 500 characters".

**Fix:** Change to `else if`.

### M4. `validate_collection_form` has the same overwrite issue

**File:** `src/routes/collections.rs`

Same sequential-insert pattern for the name field.

**Fix:** Change to `else if`.

### M5. No CSRF protection on state-changing forms

All forms use plain POST without any CSRF tokens. The `SameSite::Lax` cookie policy helps but does not prevent POST-based cross-site attacks. Since this is invite-code-based auth with no passwords, the attack surface is smaller.

**Fix:** Consider adding a CSRF middleware or note as a known limitation.

### M6. `EntryView` and `EntryWithCount` duplicate field definitions with `Entry`

**File:** `src/routes/entries.rs`

`EntryWithCount` manually duplicates all fields of `Entry` plus `visit_count`, and `into_entry_and_count` manually moves every field. Any new field added to `Entry` must be added in three places.

**Fix:** Consider using composition or `#[sqlx(flatten)]`.

---

## Low

### L1. `build_tag_cloud` uses `ln()` which could panic on zero counts

**File:** `src/routes/tags.rs`

If a tag has `count = 0`, `ln(0)` returns negative infinity. Impossible via SQL (INNER JOIN), but defensively fragile.

**Fix:** Use `(tag.count.max(1) as f64).ln()`.

### L2. `static_hash` threaded through every template struct

Every template struct contains `static_hash: &'static str`. Since `STATIC_HASH` is a compile-time constant, this could be accessed via a custom Askama filter or function, eliminating boilerplate.

### L3. Orphaned tags accumulate over time

When updating an entry's tags, old `entry_tags` rows are deleted but the orphaned `tags` rows remain. They are invisible to users (JOIN excludes them) but waste space.

**Fix:** Add cleanup query after tag updates: `DELETE FROM tags WHERE id NOT IN (SELECT DISTINCT tag_id FROM entry_tags)`.

### L4. Docker `COPY static` comes from build context, not builder stage

**File:** `Dockerfile`

Templates and migrations come from the builder stage, but static comes from the build context directly. If files change between stages, the hash baked into the binary won't match the served files.

**Fix:** Change to `COPY --from=builder /app/static /app/static`.

---

## Test Coverage Gaps

### Well tested
- Auth flows (login, logout, redirect behavior)
- Entry CRUD with form validation (empty title, bad URL, duration limits, description length)
- URL normalization (bare domain, http, https, ftp, query strings)
- Collection CRUD with authorization (owner vs member vs outsider)
- Collection membership (join, leave, remove member)
- Entry filter views (ready, waiting, unseen, all)
- Export endpoint with tags
- Tag pages (auth, empty state, user isolation, tag detail)
- Unit tests for `calculate_availability`, `format_last_viewed`, `validate_entry_form`, `Interval` serde

### Missing
- No test for `collection_id` authorization during entry create/update (H2)
- No test for tag edge cases (long names, special chars, duplicates, thousands of tags)
- No XSS test (submit `<script>` in title/description, verify escaping in response HTML)
- No test that collection-shared entries appear on tag pages (M2)
- No test for CLI import with tags
- No test for the `health` endpoint

---

## Recommended Priority

1. Fix the two `.unwrap()` calls on `normalize_url` (C1 -- crash risk)
2. Add tag input validation (C2 -- abuse/DoS risk)
3. Validate `collection_id` authorization (H2 -- data leak risk)
4. Wrap entry+tag in transactions (H4 -- consistency)
5. Add upper bound to `duration` (H3 -- overflow risk)
6. Replace `.unwrap_or_default()` with `?` (M1 -- silent failures)
7. Fix tag page access scope (M2 -- feature correctness)
8. Fix validation `if`/`else if` logic (M3/M4 -- minor UX)

| Severity | Count | Key themes |
|----------|-------|------------|
| Critical | 2 | `unwrap()` on normalize_url, unbounded tag input |
| High | 5 | Missing authorization, no transactions, integer overflow, unstable hash |
| Medium | 6 | Silent error swallowing, access scope gaps, validation logic, CSRF |
| Low | 4 | Defensive checks, boilerplate, cleanup, Docker consistency |
