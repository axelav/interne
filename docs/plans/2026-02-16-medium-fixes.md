# Medium Severity Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Address all 6 medium severity items from the code review (items 7–12).

**Architecture:** Targeted fixes across session config, auth, SQL migrations, route handlers, templates, and CSS. Each task is self-contained with a single commit.

**Tech Stack:** Rust / Axum / SQLite / Askama / tower-sessions / htmx

---

### Task 1: Update Dependencies

**Files:**
- Modify: `Cargo.toml`
- Create: `rust-toolchain.toml`

Code review item #12. Three changes: edition 2024, tower-sessions 0.15, and pin the Rust toolchain.

Note: `argon2`, sqlx `uuid`/`chrono` features, and uuid `serde` feature were already cleaned up in a prior commit. `askama_web` is deferred — the current `Html(template.render()?)` pattern works fine after the AppError refactor.

**Step 1: Update Cargo.toml**

Change line 4:
```toml
edition = "2024"
```

Change line 16:
```toml
tower-sessions = "0.15"
```

**Step 2: Create rust-toolchain.toml**

```toml
[toolchain]
channel = "stable"
```

**Step 3: Check for tower-sessions 0.15 API changes**

The `Session` API may have changed between 0.14 and 0.15. After updating, run `cargo check` and fix any compilation errors. Known potential changes:
- `session.insert()` signature
- `session.get()` return type
- `SessionManagerLayer` builder methods
- `Expiry` enum variants

Fix whatever breaks. The session security work in Task 2 builds on this.

**Step 4: Verify**

Run: `cargo build`
Expected: Compiles with no errors.

**Step 5: Commit**

```
chore(deps): update edition to 2024, tower-sessions to 0.15
```

---

### Task 2: Session Security

**Files:**
- Modify: `src/auth.rs`
- Modify: `src/routes/auth.rs`
- Modify: `src/main.rs`

Code review item #7. Four sub-fixes: cycle session ID on login, store only user ID in session (not the full User struct with invite_code), look up user per-request, and set cookie security flags.

**Step 1: Change `login_user` to store only the user ID**

In `src/auth.rs`, change `login_user`:

```rust
pub async fn login_user(session: &Session, user: &User) -> Result<(), tower_sessions::session::Error> {
    session.insert(USER_ID_KEY, &user.id).await
}
```

Note: now takes `&User` instead of `User` (caller keeps ownership).

**Step 2: Change `AuthUser` extractor to look up user from DB**

The `AuthUser` extractor currently deserializes a full `User` from the session. Change it to read just the user ID string, then query the database.

In `src/auth.rs`:

```rust
use crate::AppState;
use axum::extract::State;

// ... keep existing imports ...

pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AuthRedirect;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AuthRedirect)?;

        let user_id: Option<String> = session.get(USER_ID_KEY).await.ok().flatten();

        let Some(user_id) = user_id else {
            return Err(AuthRedirect);
        };

        let user: Option<User> = sqlx::query_as("SELECT * FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AuthRedirect)?;

        user.map(AuthUser).ok_or(AuthRedirect)
    }
}
```

Key change: the generic `S: Send + Sync` becomes concrete `AppState` so we can access `state.db`. This works because all routes already use `AppState`.

Add `sqlx` to the imports at the top of `src/auth.rs`.

**Step 3: Add `session.cycle_id()` before login**

In `src/routes/auth.rs`, the `login_submit` handler:

```rust
Some(user) => {
    session.cycle_id().await.map_err(|e| AppError::Internal(e.to_string()))?;
    login_user(&session, &user).await?;
    Ok(Redirect::to("/").into_response())
}
```

This regenerates the session ID to prevent session fixation attacks.

**Step 4: Configure cookie security flags**

In `src/main.rs`, add cookie configuration to the session layer:

```rust
use tower_sessions::cookie::SameSite;

// ...

let session_layer = SessionManagerLayer::new(session_store)
    .with_expiry(Expiry::OnInactivity(Duration::days(30)))
    .with_secure(true)
    .with_http_only(true)
    .with_same_site(SameSite::Lax);
```

`SameSite::Lax` mitigates CSRF for this app (all state-changing routes are POST/DELETE, which Lax blocks cross-origin). `Secure` ensures cookies are only sent over HTTPS. `HttpOnly` prevents JavaScript access to the session cookie.

Note: `with_secure(true)` means cookies won't be sent over plain HTTP. For local development, you may need to set this conditionally or use HTTPS locally. If this is a concern, make it conditional on an env var:

```rust
let secure = env::var("SECURE_COOKIES").unwrap_or_else(|_| "true".to_string()) == "true";

let session_layer = SessionManagerLayer::new(session_store)
    .with_expiry(Expiry::OnInactivity(Duration::days(30)))
    .with_secure(secure)
    .with_http_only(true)
    .with_same_site(SameSite::Lax);
```

**Step 5: Verify**

Run: `cargo build`
Expected: Compiles. Then test manually: log in, verify session works, log out, verify redirect.

**Step 6: Commit**

```
fix(auth): harden session security
```

---

### Task 3: Timestamp Format Standardization

**Files:**
- Create: `migrations/002_timestamps.sql`

Code review item #8. SQL `datetime('now')` produces `2026-02-16 12:00:00`. Rust `Utc::now().to_rfc3339()` produces `2026-02-16T12:00:00.000000+00:00`. The `.parse::<DateTime<Utc>>()` in `calculate_availability` silently falls back to `Utc::now()` when it can't parse the SQL format.

In practice the SQL defaults are never triggered — all inserts from Rust specify timestamps explicitly. But the defaults should match for correctness.

SQLite doesn't support `ALTER TABLE ... ALTER COLUMN DEFAULT`. The only way to change defaults is to recreate the table. That's invasive and risky for a format-only fix. Instead: standardize the defaults in a new migration using `strftime` to produce RFC3339-compatible output, applied only to **new** tables or future schema changes.

Actually, since we can't change existing table defaults without recreating tables, the pragmatic fix is:

1. Document that Rust code is the source of truth for timestamps (all inserts specify timestamps explicitly).
2. Update any existing rows that used the SQL default format (from CLI `create-user` or direct SQL).

**Step 1: Create migration to normalize existing timestamps**

Create `migrations/002_timestamps.sql`:

```sql
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
```

**Step 2: Run the migration**

Check how the app runs migrations. Look at `src/db.rs` — if it uses `sqlx::migrate!()` macro, the new file will be picked up automatically. If it runs migrations manually, add it to the list.

Run: `cargo build && cargo run` (start the server, which triggers migrations on boot)

**Step 3: Commit**

```
fix(db): normalize timestamps to RFC3339 format
```

---

### Task 4: Authorization Fixes

**Files:**
- Modify: `src/routes/entries.rs` (lines 474–485, 532–543, 639–650)
- Modify: `src/routes/collections.rs` (lines 200–225, 369–392, router at 103–115)

Code review item #11. Three sub-fixes:

1. **Restrict entry edit/delete to entry owner only.** Currently any collection member can edit/delete any entry in the collection. Change the access check for `edit_entry_form`, `update_entry`, and `delete_entry` to require `user_id = ?` (owner only). Keep the broader access for `visit_entry` and `fetch_entries_for_user` (viewing is fine for members).

2. **Prevent owner from joining own collection.** In `join_collection`, check if the user is the collection owner before inserting into `collection_members`.

3. **Add leave collection route.** Allow a member to remove themselves from a collection.

**Step 1: Restrict edit/delete to entry owner**

In `src/routes/entries.rs`, change the access check in `edit_entry_form` (around line 474), `update_entry` (around line 532), and `delete_entry` (around line 639) from:

```rust
"SELECT * FROM entries WHERE id = ? AND (user_id = ? OR collection_id IN (
    SELECT collection_id FROM collection_members WHERE user_id = ?
))"
```

To simply:

```rust
"SELECT * FROM entries WHERE id = ? AND user_id = ?"
```

And remove the third `.bind(&user.id)` call (only two binds needed now).

**Step 2: Prevent owner from joining own collection**

In `src/routes/collections.rs`, in `join_collection` (around line 212), add an owner check:

```rust
if let Some(collection) = collection {
    if collection.owner_id == user.id {
        return Ok(Redirect::to("/collections"));
    }
    let member = CollectionMember::new(collection.id, user.id);
    // ... rest unchanged
}
```

**Step 3: Add leave collection route**

Add a new route to `src/routes/collections.rs`:

In the router (around line 114), add:
```rust
.route("/collections/{id}/leave", post(leave_collection))
```

Add the handler:
```rust
async fn leave_collection(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    // Only members can leave (not owners)
    sqlx::query("DELETE FROM collection_members WHERE collection_id = ? AND user_id = ?")
        .bind(&id)
        .bind(&user.id)
        .execute(&state.db)
        .await?;

    Ok(Redirect::to("/collections"))
}
```

Add a "Leave" button to `templates/collections/show.html` — visible only to non-owner members. After the members list, before the back link:

```html
{% if !is_owner %}
<form method="post" action="/collections/{{ collection.id }}/leave" style="margin-top: 1rem;">
    <button type="submit" class="link-button delete-button"
        onclick="return confirm('Leave this collection?')">
        Leave Collection
    </button>
</form>
{% endif %}
```

**Step 4: Verify**

Run: `cargo build`
Expected: Compiles.

**Step 5: Commit**

```
fix(auth): restrict entry edit/delete to owner, add leave collection
```

---

### Task 5: HTMX Fixes

**Files:**
- Modify: `src/routes/collections.rs` (line 391)
- Modify: `templates/collections/show.html` (lines 29–36)
- Modify: `templates/entries/entry.html` (lines 4–7)

Code review item #10.

**Step 1: Fix `remove_member` to use HX-Redirect**

In `src/routes/collections.rs`, change `remove_member` return (line 391) from:

```rust
Ok(Redirect::to(&format!("/collections/{}", collection_id)))
```

To:

```rust
Ok(([("HX-Redirect", format!("/collections/{}", collection_id))], "").into_response())
```

This matches the pattern used by `delete_collection` on line 347.

**Step 2: Replace inline onclick with htmx on entry title link**

In `templates/entries/entry.html`, replace lines 4–7:

```html
<a href="{{ entry.url }}" target="_blank" rel="noopener noreferrer"
    onclick="fetch('/entries/{{ entry.id }}/visit',{method:'POST'}).then(r=>r.text()).then(h=>{let el=document.getElementById('entry-{{ entry.id }}');if(el)el.outerHTML=h})">
    {{ entry.title }} &rarr;
</a>
```

With:

```html
<a href="{{ entry.url }}" target="_blank" rel="noopener noreferrer"
    hx-post="/entries/{{ entry.id }}/visit"
    hx-target="#entry-{{ entry.id }}"
    hx-swap="outerHTML"
    hx-trigger="click"
>
    {{ entry.title }} &rarr;
</a>
```

Note: htmx does not prevent default behavior on anchor clicks by default, so the link will still open in a new tab while the POST fires asynchronously. This replaces the inline JavaScript with declarative htmx attributes.

**Step 3: Verify**

Run: `cargo build`
Expected: Compiles. Test manually: remove a member from a collection — page should redirect properly instead of swapping HTML into the button.

**Step 4: Commit**

```
fix(ui): use HX-Redirect for remove_member, replace inline onclick with htmx
```

---

### Task 6: CSS and Template Cleanup

**Files:**
- Modify: `templates/collections/form.html`
- Modify: `templates/collections/show.html`
- Modify: `templates/base.html`
- Modify: `static/style.css`

Code review item #9.

**Step 1: Move collections inline styles to CSS classes**

In `static/style.css`, add these rules (before the `/* Utilities */` section):

```css
/* Collection pages */
.collection-header {
    margin-bottom: 2rem;
}

.collection-title {
    font-size: 1.25rem;
    margin-bottom: 0.5rem;
}

.invite-box {
    margin-bottom: 1rem;
    padding: 1rem;
    border: var(--border);
}

.invite-code {
    background: var(--gray-100);
    padding: 0.25rem 0.5rem;
}

.invite-actions {
    display: inline;
    margin-left: 0.5rem;
}
```

**Step 2: Update collections/show.html to use CSS classes**

Replace the full content of `templates/collections/show.html`:

```html
{% extends "base.html" %}

{% block title %}{{ collection.name }} - Interne{% endblock %}

{% block content %}
<div class="collection-header">
    <h1 class="collection-title">{{ collection.name }}</h1>

    {% if is_owner %}
    <div class="invite-box">
        <strong>Invite Code:</strong>
        <code class="invite-code">{{ collection.invite_code }}</code>
        <form method="post" action="/collections/{{ collection.id }}/regenerate-invite" class="invite-actions">
            <button type="submit" class="link-button">Regenerate</button>
        </form>
    </div>
    {% endif %}
</div>

<h2 class="section-heading">Members</h2>

<div class="entry-list">
    {% for member in members %}
    <div class="entry">
        <div class="entry-header">
            <div class="entry-title">{{ member.name }}</div>
            {% if is_owner %}
            <div class="entry-actions">
                <button
                    class="link-button delete-button"
                    hx-delete="/collections/{{ collection.id }}/members/{{ member.id }}"
                    hx-confirm="Remove {{ member.name }} from this collection?"
                >
                    Remove
                </button>
            </div>
            {% endif %}
        </div>
        <div class="entry-meta">
            {% if let Some(email) = &member.email %}{{ email }}{% endif %}
        </div>
    </div>
    {% endfor %}
</div>

{% if !is_owner %}
<form method="post" action="/collections/{{ collection.id }}/leave" style="margin-top: 1rem;">
    <button type="submit" class="link-button delete-button"
        onclick="return confirm('Leave this collection?')">
        Leave Collection
    </button>
</form>
{% endif %}

<p style="margin-top: 2rem;">
    <a href="/collections">&larr; Back to collections</a>
</p>
{% endblock %}
```

Key changes: `style="..."` → CSS classes, `style="color: red;"` → `class="delete-button"`, consistent with entries pattern.

**Step 3: Update collections/form.html to use CSS classes**

Replace `templates/collections/form.html`:

```html
{% extends "base.html" %}

{% block title %}{% if collection.is_some() %}Edit{% else %}New{% endif %} Collection - Interne{% endblock %}

{% block content %}
<div class="form-page">
    <h1 class="form-heading">
        {% if collection.is_some() %}Edit Collection{% else %}New Collection{% endif %}
    </h1>

    <form method="post" action="{% if let Some(c) = collection %}/collections/{{ c.id }}{% else %}/collections{% endif %}">
        <div class="form-group">
            <label for="name">Name</label>
            <input
                type="text"
                id="name"
                name="name"
                required
                autofocus
                value="{% if let Some(c) = collection %}{{ c.name }}{% endif %}"
            >
            <div class="error-message">{% if let Some(err) = errors.get("name") %}{{ err }}{% endif %}</div>
        </div>

        <div class="form-actions">
            <button type="submit">Save</button>
            <a href="/collections">Cancel</a>
            {% if collection.is_some() %}
            <button
                type="button"
                class="link-button delete-button"
                hx-delete="/collections/{{ collection.as_ref().unwrap().id }}"
                hx-confirm="Delete this collection? All entries will become private."
            >
                Delete
            </button>
            {% endif %}
        </div>
    </form>
</div>
{% endblock %}
```

Key changes: uses `.form-page`, `.form-heading`, `.form-actions`, `.delete-button` classes — consistent with entries form.

**Step 4: Add Collections link to nav**

In `templates/base.html`, add a Collections link to the footer (line 25):

```html
<footer>
    <a href="/collections">Collections</a>
    <a href="/export">Export</a>
    {% if user.is_some() %}
    <form action="/logout" method="post" style="display: inline;">
        <button type="submit" class="link-button">Logout</button>
    </form>
    {% endif %}
</footer>
```

**Step 5: Clean up CSS**

In `static/style.css`:

1. **Remove empty `.form-page` rule** (lines 180–181). Replace with actual styles:
```css
.form-page {
    max-width: 400px;
}
```

2. **Remove unused `.sr-only`** (lines 359–369). Delete the entire block.

3. **Remove `!important` from `.logo`** (lines 63–66). The `.logo` class is specific enough. Replace with:
```css
.logo {
    font-style: italic;
    font-size: 1.125rem;
    font-weight: 600;
    color: var(--black);
    letter-spacing: -0.01em;
}
```

Since `.header-left a` sets `font-size: 0.8125rem` and `color: var(--gray-600)`, and `.logo` has the same specificity (one class), we need `.logo` to win. Both are single-class selectors, but `.logo` appears later in the file, so it wins by source order. The `!important` is unnecessary.

4. **Remove `!important` from `.link-button`** (lines 284–297). The `!important` flags fight `button[type="submit"]` styles. Since `.link-button` is only used on `<button>` elements without `type="submit"`, and the `button[type="submit"]` selector is more specific than `.link-button`, we need to keep some specificity. Change to:
```css
button.link-button {
    background: none;
    border: none;
    color: var(--gray-600);
    cursor: pointer;
    font-size: inherit;
    font-family: inherit;
    padding: 0;
    text-decoration: none;
}

button.link-button:hover {
    color: var(--black);
}
```

The `button.link-button` selector (element + class) is more specific than `button[type="submit"]` (element + attribute), so it wins without `!important`.

5. **Remove `!important` from `.delete-button`** (lines 217–224):
```css
.delete-button {
    margin-left: auto;
    color: #c0392b;
}

.delete-button:hover {
    color: #a93226;
}
```

Since `.delete-button` is used alongside `.link-button` (which sets color), and both are single-class selectors, `.delete-button` needs to appear after `.link-button` in the file (which it already does). So `!important` is unnecessary.

**Step 6: Verify**

Run: `cargo build`
Expected: Compiles. Visually check: collections form matches entries form styling, delete buttons are consistent red, Collections link appears in footer.

**Step 7: Commit**

```
fix(ui): move inline styles to CSS, clean up unused rules, add collections nav
```

---

## Summary

| Task | Review Item | Description |
|------|------------|-------------|
| 1 | #12 | Dependencies: edition 2024, tower-sessions 0.15, rust-toolchain.toml |
| 2 | #7 | Session security: cycle_id, store user_id only, cookie flags |
| 3 | #8 | Timestamp standardization: normalize to RFC3339 |
| 4 | #11 | Authorization: owner-only edit/delete, leave collection, prevent self-join |
| 5 | #10 | HTMX: HX-Redirect for remove_member, replace inline onclick |
| 6 | #9 | CSS/templates: inline styles to classes, cleanup, nav link |
