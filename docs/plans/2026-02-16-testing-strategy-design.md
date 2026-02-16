# Testing Strategy Design

## Goal

Cover all critical paths with tests. Not 100% coverage — focus on business logic and user-facing flows to ensure features work and users don't hit unexpected errors.

## Decisions

- **Level:** Unit tests for pure logic + integration tests for HTTP flows
- **Auth in tests:** Pre-seed sessions directly in DB via helper, test login flow separately
- **Layout:** Unit tests inline (`#[cfg(test)] mod tests`), integration tests in `tests/`
- **HTTP testing:** `tower::ServiceExt::oneshot()` — no ports, no `reqwest`

## Structure

```
src/
  routes/entries.rs      — inline unit tests for calculate_availability, format_last_viewed, validation
  routes/collections.rs  — inline unit tests for validate_collection_form
  models/entry.rs        — inline unit tests for Interval serde roundtrips

tests/
  common/mod.rs    — test app builder, user seeding, session helper
  auth.rs          — login/logout/redirect flows
  entries.rs       — CRUD, validation, visit, visibility, tags
  collections.rs   — CRUD, membership, permissions
  export.rs        — JSON export
```

### Test helpers (`tests/common/mod.rs`)

- `test_app()` — builds full Axum router with in-memory SQLite (`:memory:`), runs migrations
- `create_test_user(db) -> User` — inserts user with known invite code
- `authed_request(router, user) -> (router, session_cookie)` — pre-seeds session row, returns cookie

## Unit Tests

### `calculate_availability()` (routes/entries.rs)

- Never dismissed (`dismissed_at: None`) → always available, `available_in: None`
- Just dismissed (1 second ago), interval "3 days" → not available, shows ~3 days remaining
- Dismissed exactly at boundary (3 days ago, interval 3 days) → available
- Past boundary (4 days ago, interval 3 days) → available
- Each interval type: hours, days, weeks, months, years
- Edge: `duration = 1` with each interval type
- Edge: large duration values (999 years)

### `format_last_viewed()` (routes/entries.rs)

Fix singular forms before writing tests ("1 year ago" not "1 years ago").

- `None` → "never"
- Just now (seconds ago) → "just now"
- Minutes ago (singular + plural)
- Hours ago (singular + plural)
- Days ago (singular + plural)
- Weeks ago (singular + plural)
- Months ago (singular + plural)
- Years ago (singular + plural)

### Entry form validation (routes/entries.rs)

Extract `validate_entry_form()` from handler code to enable unit testing.

- Valid form passes
- Empty title → error
- Title > 500 chars → error
- URL without http/https → error
- Duration < 1 → error

### `validate_collection_form()` (routes/collections.rs)

- Valid name passes
- Empty name → error
- Name > 100 chars → error

### `Interval` serde (models/entry.rs)

- Deserialize from lowercase strings ("hours", "days", "weeks", "months", "years")
- Serialize back to same strings
- All variants round-trip

## Integration Tests

### Auth (`tests/auth.rs`)

- POST `/login` valid invite code → redirect to `/`, session cookie set
- POST `/login` invalid invite code → re-renders login with error
- POST `/logout` → clears session, redirect to `/login`
- GET `/` unauthenticated → redirect to `/login`
- GET `/entries/new` unauthenticated → redirect to `/login`

### Entries (`tests/entries.rs`)

**CRUD:**
- POST `/entries` valid form → creates entry, redirects to `/`
- POST `/entries` invalid data (bad URL, empty title, duration 0) → form with errors
- GET `/entries/{id}/edit` as owner → renders form
- GET `/entries/{id}/edit` as non-owner → rejected
- POST `/entries/{id}` as owner → updates entry
- DELETE `/entries/{id}` as owner → deletes, HX-Redirect
- DELETE `/entries/{id}` as non-owner → rejected

**Core flows:**
- GET `/` → shows only available entries (seed one available, one not-yet-due)
- GET `/all` → shows all entries
- POST `/entries/{id}/visit` → updated HTML partial, entry unavailable on next GET `/`

**Visibility across collections:**
- User A creates entry in collection → User B (member) sees it on GET `/`
- User B leaves collection → no longer sees User A's entries

**Tags:**
- Create entry with new tags → tags auto-created, linked
- Edit entry, change tags → old unlinked, new linked

### Collections (`tests/collections.rs`)

**CRUD:**
- POST `/collections` valid name → creates, redirects
- POST `/collections` empty name → error
- GET `/collections/{id}` as owner → shows members, invite code, owner controls
- GET `/collections/{id}` as member → shows collection, no owner controls
- GET `/collections/{id}` as non-member → rejected
- POST `/collections/{id}` update as owner → works
- POST `/collections/{id}` update as member → rejected
- DELETE `/collections/{id}` as owner → deletes
- DELETE `/collections/{id}` as member → rejected

**Membership:**
- POST `/collections/{id}/regenerate-invite` as owner → new invite code
- POST `/collections/{id}/leave` as member → removes membership
- DELETE `/collections/{id}/members/{user_id}` as owner → removes member

### Export (`tests/export.rs`)

- GET `/export` → JSON with Content-Disposition header, contains user's entries with tags

## Refactoring Required

1. **Extract `validate_entry_form()`** — currently inline in `create_entry`/`update_entry` handlers
2. **Fix `format_last_viewed()` singular forms** — "1 year ago" not "1 years ago"

## Dependencies

Add to `Cargo.toml` `[dev-dependencies]`:
- `tower` with `util` feature (for `oneshot()`)
- `http-body-util` (for reading response bodies)
