# Tag Autocomplete Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add typeahead autocomplete to the tags input on the entry form so users can discover and reuse their existing tags as they type.

**Architecture:** A new `GET /api/tags/search?q=` endpoint returns an HTML fragment of matching tag buttons. Vanilla JS on the form template handles segment extraction from the comma-separated input, debounced fetching, dropdown display, and keyboard navigation. CSS styles position the dropdown below the input.

**Tech Stack:** Rust/Axum (endpoint), vanilla JS (client logic), CSS (dropdown styling)

---

### Task 1: Write failing tests for the tag search endpoint

**Files:**
- Modify: `tests/tags.rs`

**Step 1: Write the failing tests**

Add four tests at the end of `tests/tags.rs`:

```rust
#[tokio::test]
async fn tag_search_requires_auth() {
    let app = TestApp::new().await;
    let resp = app.get("/api/tags/search?q=r", None).await;
    assert!(resp.status().is_redirection());
}

#[tokio::test]
async fn tag_search_returns_matching_tags() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Entry&description=&duration=3&interval=days&tags=rust%2C+music%2C+reading&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    let resp = app.get("/api/tags/search?q=r", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("rust"));
    assert!(html.contains("reading"));
    assert!(!html.contains("music"));
}

#[tokio::test]
async fn tag_search_empty_for_no_match() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Entry&description=&duration=3&interval=days&tags=rust&collection_id=";
    app.post_form("/entries", body, Some(&cookie)).await;

    let resp = app.get("/api/tags/search?q=zzz", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.is_empty());
}

#[tokio::test]
async fn tag_search_scoped_to_user() {
    let app = TestApp::new().await;
    let (_user1_id, invite1) = app.create_user("User 1").await;
    let cookie1 = app.login(&invite1).await;

    let body = "url=https%3A%2F%2Fexample.com&title=Entry&description=&duration=3&interval=days&tags=secret&collection_id=";
    app.post_form("/entries", body, Some(&cookie1)).await;

    let (_user2_id, invite2) = app.create_user("User 2").await;
    let cookie2 = app.login(&invite2).await;

    let resp = app.get("/api/tags/search?q=sec", Some(&cookie2)).await;
    let html = body_string(resp).await;
    assert!(!html.contains("secret"));
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test tags`

Expected: 4 new tests fail (404 — route doesn't exist yet). Existing tests still pass.

**Step 3: Commit**

```bash
git add tests/tags.rs
git commit -m "test(tags): add failing tests for tag search endpoint"
```

---

### Task 2: Implement the tag search endpoint

**Files:**
- Modify: `src/routes/tags.rs:1-14` (imports) and `src/routes/tags.rs:98-102` (router)

**Step 1: Add imports and query params struct**

At the top of `src/routes/tags.rs`, add `Query` to the axum extract imports and add `serde::Deserialize`:

```rust
use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
```

Add after the existing `use` block (after line 14):

```rust
#[derive(Deserialize)]
struct TagSearchParams {
    q: Option<String>,
}
```

Note: `serde::Deserialize` is available via `#[derive(Deserialize)]` because `serde` is already a project dependency. Add `use serde::Deserialize;` to the imports if needed, or check if it's re-exported.

**Step 2: Add the route**

In the `router()` function (line 98-102), add the new route:

```rust
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tags", get(list_tags))
        .route("/tags/{name}", get(show_tag))
        .route("/api/tags/search", get(search_tags))
}
```

**Step 3: Implement the handler**

Add at the end of the file (after `show_tag`):

```rust
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

async fn search_tags(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
    Query(params): Query<TagSearchParams>,
) -> Result<impl IntoResponse, AppError> {
    let prefix = params.q.unwrap_or_default().trim().to_lowercase();
    if prefix.is_empty() {
        return Ok(Html(String::new()));
    }

    let pattern = format!("{}%", prefix);
    let tags: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT DISTINCT t.name
        FROM tags t
        JOIN entry_tags et ON et.tag_id = t.id
        JOIN entries e ON e.id = et.entry_id
        WHERE e.user_id = ? AND t.name LIKE ?
        ORDER BY t.name ASC
        LIMIT 10
        "#,
    )
    .bind(&user.id)
    .bind(&pattern)
    .fetch_all(&state.db)
    .await?;

    let html: String = tags
        .into_iter()
        .map(|(name,)| {
            let escaped = escape_html(&name);
            format!(
                r#"<button type="button" class="autocomplete-item" data-value="{escaped}">{escaped}</button>"#
            )
        })
        .collect();

    Ok(Html(html))
}
```

**Step 4: Run tests to verify they pass**

Run: `cargo test --test tags`

Expected: All tests pass including the 4 new ones.

**Step 5: Commit**

```bash
git add src/routes/tags.rs
git commit -m "feat(tags): add tag search API endpoint for autocomplete"
```

---

### Task 3: Add autocomplete CSS

**Files:**
- Modify: `static/style.css` (insert after line 319, before the `.char-count.near-limit` block ends)

**Step 1: Add the dropdown styles**

Insert after the `.char-count.near-limit` rule (after line 319 in `style.css`):

```css
/* Tag autocomplete */
.tags-autocomplete-wrapper {
    position: relative;
}

.tags-autocomplete {
    position: absolute;
    top: 100%;
    left: 0;
    right: 0;
    background: var(--white);
    border: var(--border);
    border-top: none;
    border-radius: 0 0 var(--radius) var(--radius);
    z-index: 10;
    max-height: 200px;
    overflow-y: auto;
    box-shadow: 0 4px 6px rgba(0, 0, 0, 0.05);
    display: none;
}

.tags-autocomplete:empty {
    display: none;
}

.autocomplete-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 0.5rem 0.625rem;
    border: none;
    background: none;
    font-size: 0.9375rem;
    font-family: inherit;
    color: var(--black);
    cursor: pointer;
}

.autocomplete-item:hover,
.autocomplete-item.active {
    background: var(--gray-100);
}
```

**Step 2: Verify build**

Run: `cargo build`

Expected: Compiles. CSS is a static file, no Rust impact.

**Step 3: Commit**

```bash
git add static/style.css
git commit -m "style(tags): add autocomplete dropdown CSS"
```

---

### Task 4: Update form template with autocomplete HTML and JS

**Files:**
- Modify: `templates/entries/form.html:72-81` (tags form-group) and append new script block

**Step 1: Wrap the tags input in the autocomplete container**

Replace lines 72-81 of `templates/entries/form.html`:

```html
        <div class="form-group">
            <label for="tags">Tags</label>
            <div class="tags-autocomplete-wrapper">
                <input
                    type="text"
                    id="tags"
                    name="tags"
                    value="{{ tags_string }}"
                    placeholder="comma, separated, tags"
                >
                <div class="tags-autocomplete" id="tags-autocomplete"></div>
            </div>
        </div>
```

**Step 2: Add the autocomplete JS**

Insert a new `<script>` block after the existing `</script>` on line 197 (before `{% endblock %}`):

```html
<script>
(function() {
    var input = document.getElementById('tags');
    var dropdown = document.getElementById('tags-autocomplete');
    if (!input || !dropdown) return;

    var debounceTimer = null;
    var activeIndex = -1;

    function getCurrentSegment() {
        var parts = input.value.split(',');
        return parts[parts.length - 1].trim();
    }

    function getExistingTags() {
        return input.value.split(',')
            .map(function(s) { return s.trim().toLowerCase(); })
            .filter(function(s) { return s.length > 0; });
    }

    function replaceCurrentSegment(tagName) {
        var parts = input.value.split(',');
        if (parts.length === 1) {
            parts[0] = tagName;
        } else {
            parts[parts.length - 1] = ' ' + tagName;
        }
        input.value = parts.join(',') + ', ';
        input.focus();
        hideDropdown();
    }

    function showDropdown(html) {
        dropdown.innerHTML = html;
        dropdown.style.display = html ? 'block' : 'none';
        activeIndex = -1;
    }

    function hideDropdown() {
        dropdown.style.display = 'none';
        dropdown.innerHTML = '';
        activeIndex = -1;
    }

    function updateActiveItem() {
        var items = dropdown.querySelectorAll('.autocomplete-item');
        items.forEach(function(el) { el.classList.remove('active'); });
        if (activeIndex >= 0 && activeIndex < items.length) {
            items[activeIndex].classList.add('active');
            items[activeIndex].scrollIntoView({ block: 'nearest' });
        }
    }

    input.addEventListener('input', function() {
        clearTimeout(debounceTimer);
        var segment = getCurrentSegment();
        if (!segment) {
            hideDropdown();
            return;
        }
        debounceTimer = setTimeout(function() {
            fetch('/api/tags/search?q=' + encodeURIComponent(segment.toLowerCase()))
                .then(function(r) { return r.text(); })
                .then(function(html) {
                    var existing = getExistingTags();
                    var temp = document.createElement('div');
                    temp.innerHTML = html;
                    var buttons = temp.querySelectorAll('.autocomplete-item');
                    buttons.forEach(function(btn) {
                        if (existing.indexOf(btn.getAttribute('data-value')) !== -1) {
                            btn.remove();
                        }
                    });
                    showDropdown(temp.innerHTML);
                });
        }, 300);
    });

    input.addEventListener('keydown', function(e) {
        var items = dropdown.querySelectorAll('.autocomplete-item');
        if (!items.length || dropdown.style.display === 'none') return;

        if (e.key === 'ArrowDown') {
            e.preventDefault();
            activeIndex = (activeIndex + 1) % items.length;
            updateActiveItem();
        } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            activeIndex = (activeIndex - 1 + items.length) % items.length;
            updateActiveItem();
        } else if (e.key === 'Enter' && activeIndex >= 0) {
            e.preventDefault();
            replaceCurrentSegment(items[activeIndex].getAttribute('data-value'));
        } else if (e.key === 'Escape') {
            hideDropdown();
        }
    });

    dropdown.addEventListener('click', function(e) {
        var item = e.target.closest('.autocomplete-item');
        if (item) {
            replaceCurrentSegment(item.getAttribute('data-value'));
        }
    });

    document.addEventListener('click', function(e) {
        if (!e.target.closest('.tags-autocomplete-wrapper')) {
            hideDropdown();
        }
    });
})();
</script>
```

**Step 3: Verify build**

Run: `cargo build`

Expected: Compiles (Askama re-checks templates at compile time).

**Step 4: Manual verification**

1. Start server: `cargo run`
2. Log in and go to `/entries/new`
3. Type a prefix matching existing tags — dropdown appears after 300ms
4. Arrow keys highlight items, Enter selects, Escape closes
5. Clicking a suggestion inserts it with trailing comma
6. Already-entered tags are excluded from suggestions
7. Edit an existing entry — autocomplete works the same way

**Step 5: Commit**

```bash
git add templates/entries/form.html
git commit -m "feat(tags): add autocomplete to tags input on entry form"
```

---

### Task 5: Run full test suite and final verification

**Step 1: Run all tests**

Run: `cargo test`

Expected: All tests pass.

**Step 2: Final build check**

Run: `cargo build`

Expected: Clean compile, no warnings related to our changes.
