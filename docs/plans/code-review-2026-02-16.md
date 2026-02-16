# Interne Rebuild: Code Review

**Date:** 2026-02-16
**Branch:** `rebuild`
**Reviewer:** Staff Engineer (automated)

---

## Overview

Full review of the `rebuild` branch — a complete rewrite from Next.js to Rust (Axum + Askama + SQLite). 24 commits, ~2600 lines added. The codebase is clean, well-structured, and functional. The findings below are organized by severity to help prioritize what to address.

---

## 1. Critical: Pervasive `.unwrap()` on Database and Template Operations

**Every** database call in every route handler uses `.unwrap()`. Every template render uses `.unwrap()`. Any transient error (locked database, disk full, corrupt query) will panic and kill the Tokio task or crash the server.

**Scope:** ~50+ unwrap calls across `entries.rs`, `collections.rs`, `auth.rs`, `export.rs`.

**Recommendation:** Define an `AppError` type that implements `IntoResponse`, return `Result<impl IntoResponse, AppError>` from handlers, and propagate with `?`. Adding `askama_web = { version = "0.15", features = ["axum-0.8"] }` would also eliminate the template `.render().unwrap()` boilerplate — templates would implement `IntoResponse` directly.

---

## 2. Critical: `Interval` Enum Exists but Is Not Used

`src/models/entry.rs` defines a well-typed `Interval` enum with `serde`, `sqlx::Type`, `Display`, `Copy`, `PartialEq`, and `Eq` derives. But `Entry.interval` is `String`, `EntryForm.interval` is `String`, and `calculate_availability()` matches on `.as_str()` with a silent catch-all fallback:

```rust
_ => Duration::days(entry.duration),  // silently treats invalid values as days
```

The enum provides zero type safety in practice. The SQL schema has a CHECK constraint that would cause a panic (via `.unwrap()`) on invalid values.

**Recommendation:** Change `Entry.interval` to the `Interval` type. Use it in `EntryForm` deserialization. Re-export `Interval` from `models/mod.rs`.

---

## 3. High: Dead Code

| Item | Location | Notes |
|------|----------|-------|
| `EntryTag` struct | `models/tag.rs:24` | Never constructed. Compiler warning. |
| `Entry::new()` | `models/entry.rs` | Never called — entries created via inline SQL |
| `Tag::new()` | `models/tag.rs` | Never called — tags created via inline SQL |
| `User::new()` | `models/user.rs` | Never called — users created via inline SQL in CLI |
| `get_current_user()` | `auth.rs:47` | Never called |
| `user_name` field | `routes/entries.rs:23` | Set but never read in template. Compiler warning. |
| `DateTime` import | `models/user.rs:1` | Only `Utc` is used |
| `argon2` crate | `Cargo.toml:21` | Not imported anywhere in source |
| `_method` hidden field | `templates/entries/form.html:13` | No server-side method override middleware |
| `#[allow(dead_code)]` | `cli.rs:33` | Suppresses warning on `LegacyEntry.id` |

**Recommendation:** Delete all dead code. Remove `argon2` from Cargo.toml. Remove `_method` hidden input. Remove `user_name` from `EntryListTemplate`.

---

## 4. High: No Input Validation on Entry Forms

- **`interval`**: Accepts any string. Invalid values cause a DB CHECK constraint violation → panic via `.unwrap()`.
- **`duration`**: Accepts any `i64` including 0 and negatives. HTML `min="1"` is trivially bypassed.
- **`url`**: No server-side URL validation. Could accept `javascript:` URIs → stored XSS in `<a href>`.
- **`title`**: No length limit.
- **Collection name**: No validation either.

**Recommendation:** Validate server-side before insertion. Use the `Interval` enum for deserialization. Validate URL is HTTP/HTTPS. Clamp duration > 0. Add `CHECK (duration > 0)` to the SQL schema.

---

## 5. High: N+1 Query in Export

`routes/export.rs:52-59` — For each entry, a separate query fetches its tags. 500 entries = 501 queries.

**Recommendation:** Single query with `GROUP_CONCAT` or batch fetch + HashMap grouping in Rust.

---

## 6. High: CLI Import Is Not Transactional

`cli.rs` imports entries one at a time without a transaction. If entry 50/100 fails (e.g., invalid interval hits CHECK constraint), entries 1-49 are committed and the rest are lost. Re-running duplicates those 49 entries since there's no deduplication.

**Recommendation:** Wrap the import in `pool.begin()` / `tx.commit()`. Validate interval values before insertion.

---

## 7. Medium: Session Security

| Issue | Location |
|-------|----------|
| No `session.cycle_id()` on login — session fixation risk | `routes/auth.rs:59` |
| Full `User` struct stored in session (includes `invite_code`) | `auth.rs:40` |
| Cookie missing `Secure`, explicit `HttpOnly`, and `SameSite` flags | `main.rs` session layer |
| No CSRF protection on any form | All POST routes |

**Recommendation:**
- Call `session.cycle_id().await` before `login_user()`.
- Store only the user ID in the session; look up the user per request.
- Configure the session layer:

```rust
.with_secure(true)
.with_http_only(true)
.with_same_site(SameSite::Lax)
```

The `SameSite::Lax` flag significantly mitigates CSRF for this app.

---

## 8. Medium: Timestamp Format Mismatch

All model timestamps are `String`. Rust generates `to_rfc3339()` format (`2026-02-16T12:00:00+00:00`), but SQLite's `datetime('now')` default produces `2026-02-16 12:00:00`. Any row created by a SQL default vs Rust code will have different formats. The `parse()` call in `calculate_availability()` may behave inconsistently.

**Recommendation:** Either use `chrono::DateTime<Utc>` types with sqlx's chrono feature (preferred), or standardize on a single format string.

---

## 9. Medium: Template / CSS Inconsistencies

| Issue | Details |
|-------|---------|
| Collections templates use inline styles | `collections/form.html`, `collections/show.html` — should use the CSS classes defined for entries (`form-page`, `form-heading`, `form-actions`) |
| Delete buttons styled differently | Entries use `class="delete-button"` (`#c0392b`), collections use `style="color: red;"` (`#ff0000`) |
| `.form-page` CSS rule is empty | `style.css:180` — no-op |
| `.sr-only` class defined but unused | `style.css:359` |
| `!important` overrides on `.logo` and `.link-button` | Could be solved with better specificity |
| No navigation to Collections from main page | Users must type `/collections` manually |

**Recommendation:** Move collections inline styles to CSS classes. Use `class="delete-button"` consistently. Add a Collections link to the nav.

---

## 10. Medium: HTMX Issues

- `remove_member` (`collections.rs:358`) returns `Redirect` instead of `HX-Redirect` header — htmx will swap the full page into the button element.
- Delete buttons have no `hx-target` or `hx-swap` — they work because the server sends `HX-Redirect`, but if the header is ever missing, the response body swaps into the button itself.
- Entry title link has an inline `onclick` handler that duplicates htmx logic — could be replaced with htmx attributes on the link itself.

---

## 11. Medium: Authorization Allows Collection Members to Edit/Delete Others' Entries

`update_entry` and `delete_entry` check `user_id = ? OR collection_id IN (SELECT collection_id FROM collection_members WHERE user_id = ?)`. Any collection member can modify/delete any entry in that collection. Additionally, there's no route for a member to leave a collection they joined.

---

## 12. Dependencies

### Should Update

| Crate | Current | Latest | Notes |
|-------|---------|--------|-------|
| Rust edition | 2021 | **2024** | Stable since Feb 2025; should use for a new project |
| `tower-sessions` | 0.14 | **0.15** | Released 2026-02-01; check store compatibility |

### Should Add

| Crate | Purpose |
|-------|---------|
| `askama_web` with `axum-0.8` feature | Eliminates all `Html(template.render().unwrap())` boilerplate; handles template errors gracefully |

### Should Remove

| Crate/Feature | Reason |
|---------------|--------|
| `argon2` | Not used anywhere |
| `sqlx` feature `uuid` | UUIDs are passed as strings, not as native `Uuid` types |
| `sqlx` feature `chrono` | Dates are passed as strings, not as native `DateTime` types |
| `uuid` feature `serde` | `Uuid` is never serialized directly |

### All Other Crates Are Current

axum 0.8.8, sqlx 0.8.6, askama 0.15.4, tokio 1.49.0, chrono 0.4.43, dotenvy 0.15.7, tower-http 0.6.8, serde 1.0.228 — all latest stable.

---

## 13. Docker

| Severity | Issue |
|----------|-------|
| **High** | No non-root `USER` in runtime image |
| **Medium** | `static/` directory may not exist — `COPY static /app/static` would fail |
| **Medium** | No `read_only: true` on container filesystem |
| **Low** | No `HEALTHCHECK` (app has `/health` endpoint) |
| **Low** | Rust image pinned to 1.83 — will go stale |

---

## 14. Low Priority Items

- **Pluralization bug:** `"in 1 days"`, `"in 1 hours"` — needs singular/plural handling.
- **`db.rs:8`:** `create_dir_all().ok()` silently swallows directory creation failures.
- **No graceful shutdown** — `axum::serve().await.unwrap()` with no SIGTERM handler.
- **Server binds to `0.0.0.0:3000`** unconditionally — not configurable.
- **`join_collection`** silently redirects on invalid invite code — no error feedback.
- **Owner can join own collection as member** — no ownership check.
- **Tags accumulate forever** — deleting entries removes `entry_tags` but orphans `tags`.
- **`visited` count in import** has no upper bound — 10,000 visits = 10,000 sequential INSERTs.
- **No `rust-toolchain.toml`** for pinning the minimum Rust version.
- **All IDs are bare `String`** — no newtype safety (swapping `user_id` and `entry_id` compiles fine).

---

## Summary by Count

| Severity | Count |
|----------|-------|
| Critical | 2 (unwrap panics, unused Interval enum) |
| High | 4 (dead code, no validation, N+1 export, non-transactional import) |
| Medium | 6 (session security, timestamps, CSS inconsistency, htmx, authorization, dependencies) |
| Low | 10+ |

---

## What's Good

- **No `unsafe` code, no `as` casts.** Clean Rust.
- **All SQL queries are parameterized.** No injection risk.
- **Askama auto-escapes** all template output. No XSS from template rendering.
- **Clean module structure.** Models, routes, auth, CLI are well-separated.
- **Idiomatic crate choices.** axum + sqlx + askama is the right stack for this app.
- **Docker multi-stage build** with dependency caching.
- **Solid `.dockerignore` and `.gitignore`.**
