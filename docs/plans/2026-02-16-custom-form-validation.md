# Custom Form Validation Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace browser-native form validation with custom client-side + server-side validation so users can enter URLs without `https://` prefix (e.g. `yahoo.com`), with consistent inline error display across all fields.

**Architecture:** Add `novalidate` to the form to disable browser validation. Client-side JS validates on blur (per-field) and submit (all fields), showing errors in existing `.error-message` divs. Server-side uses the `url` crate to parse/normalize URLs after prepending `https://` if no scheme present. All existing server-side validation stays as the authoritative source of truth.

**Tech Stack:** Rust `url` crate for URL parsing/normalization, vanilla JS for client-side validation, existing Askama templates + `.error-message` CSS.

---

### Task 1: Add `url` crate dependency

**Files:**
- Modify: `Cargo.toml:6-22`

**Step 1: Add the dependency**

Add `url = "2"` to `[dependencies]` in `Cargo.toml`:

```toml
[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio", "sqlite"] }
askama = "0.15"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["fs", "trace", "set-header"] }
tower-sessions = "0.15"
tower-sessions-sqlx-store = { git = "https://github.com/maxcountryman/tower-sessions-stores", rev = "be0f230f", features = ["sqlite"] }
dotenvy = "0.15"
tracing = "0.1"
tracing-subscriber = "0.3"
time = "0.3"
url = "2"
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles successfully with the new dependency.

**Step 3: Commit**

```
git add Cargo.toml Cargo.lock
git commit -m "chore(deps): add url crate for URL parsing and normalization"
```

---

### Task 2: Write failing tests for URL normalization and validation

**Files:**
- Modify: `tests/entries.rs`

These tests document what the server accepts/rejects and how normalization works. They will fail until Task 3 implements the new validation.

**Step 1: Write the failing tests**

Replace the existing `create_entry_with_bad_url_shows_error` test and add new URL validation tests. Find this test in `tests/entries.rs` (around line 36):

```rust
#[tokio::test]
async fn create_entry_with_bad_url_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=ftp%3A%2F%2Fexample.com&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("URL must start with http"));
}
```

Replace it with:

```rust
#[tokio::test]
async fn create_entry_with_bare_domain_normalizes_url() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // "yahoo.com" should be accepted and normalized to "https://yahoo.com/"
    let body = "url=yahoo.com&title=Yahoo&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "https://yahoo.com/");
}

#[tokio::test]
async fn create_entry_with_https_url_preserves_it() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=https%3A%2F%2Fexample.com%2Fpath&title=Example&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "https://example.com/path");
}

#[tokio::test]
async fn create_entry_with_http_url_preserves_it() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=http%3A%2F%2Fexample.com&title=Example&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "http://example.com/");
}

#[tokio::test]
async fn create_entry_with_invalid_url_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // "not a url" has no valid domain structure
    let body = "url=not+a+url&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Please enter a valid URL"));
}

#[tokio::test]
async fn create_entry_with_bare_word_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    // "yahoo" alone is not a valid URL even after normalization
    let body = "url=yahoo&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Please enter a valid URL"));
}

#[tokio::test]
async fn create_entry_with_ftp_url_shows_error() {
    let app = TestApp::new().await;
    let (_user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=ftp%3A%2F%2Fexample.com&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let html = body_string(resp).await;
    assert!(html.contains("Please enter a valid URL"));
}

#[tokio::test]
async fn create_entry_with_path_and_query_normalizes() {
    let app = TestApp::new().await;
    let (user_id, invite_code) = app.create_user("Test User").await;
    let cookie = app.login(&invite_code).await;

    let body = "url=example.com%2Fpath%3Fq%3D1&title=Test&description=&duration=3&interval=days&tags=&collection_id=";
    let resp = app.post_form("/entries", body, Some(&cookie)).await;
    assert_redirect(&resp, "/");

    let (url,): (String,) = sqlx::query_as("SELECT url FROM entries WHERE user_id = ?")
        .bind(&user_id)
        .fetch_one(&app.db)
        .await
        .unwrap();
    assert_eq!(url, "https://example.com/path?q=1");
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test --test entries`
Expected: The new tests fail because the current validation uses manual string checks, not the `url` crate. The bare-domain tests will fail because the current code rejects URLs without `http://`/`https://`, and the `ftp://` test will fail because the error message changed.

**Step 3: Commit**

```
git add tests/entries.rs
git commit -m "test(entries): add URL normalization and validation tests"
```

---

### Task 3: Rewrite server-side URL validation with `url` crate

**Files:**
- Modify: `src/routes/entries.rs:104-148` (the `normalize_url` and `validate_entry_form` functions)

**Step 1: Rewrite `normalize_url` to use `url::Url`**

Replace the current `normalize_url` function and the URL validation in `validate_entry_form` (lines 104-148 of `src/routes/entries.rs`). Find:

```rust
fn normalize_url(url: &str) -> String {
    let url = url.trim().to_string();
    if url.is_empty() {
        return url;
    }
    if url.starts_with("http://") || url.starts_with("https://") {
        url
    } else {
        format!("https://{}", url)
    }
}

fn validate_entry_form(form: &EntryForm) -> HashMap<String, String> {
    let mut errors = HashMap::new();

    if form.duration < 1 {
        errors.insert("duration".to_string(), "Duration must be at least 1".to_string());
    }

    if !form.url.is_empty() {
        let host = form.url
            .strip_prefix("https://")
            .or_else(|| form.url.strip_prefix("http://"))
            .unwrap_or(&form.url);
        let domain = host.split(&['/', '?', '#'][..]).next().unwrap_or("");
        if !domain.contains('.') {
            errors.insert("url".to_string(), "Please enter a valid URL (e.g. example.com)".to_string());
        }
    }

    if form.title.trim().is_empty() {
        errors.insert("title".to_string(), "Title is required".to_string());
    }

    if form.title.len() > 500 {
        errors.insert("title".to_string(), "Title must be under 500 characters".to_string());
    }

    if let Some(ref desc) = form.description {
        if desc.len() > 5000 {
            errors.insert("description".to_string(), "Description must be under 5000 characters".to_string());
        }
    }

    errors
}
```

Replace with:

```rust
/// Normalizes a URL string: prepends `https://` if no scheme, then parses with
/// the `url` crate. Returns `Ok(normalized_url_string)` or `Err(error_message)`.
fn normalize_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(String::new());
    }

    // If no scheme, prepend https://
    let with_scheme = if raw.starts_with("http://") || raw.starts_with("https://") {
        raw.to_string()
    } else if raw.contains("://") {
        // Has a scheme but it's not http/https (e.g. ftp://)
        return Err("Please enter a valid URL (e.g. example.com)".to_string());
    } else {
        format!("https://{}", raw)
    };

    match url::Url::parse(&with_scheme) {
        Ok(parsed) => {
            // Reject if the host is empty or not a valid domain
            match parsed.host_str() {
                Some(host) if host.contains('.') => Ok(parsed.to_string()),
                _ => Err("Please enter a valid URL (e.g. example.com)".to_string()),
            }
        }
        Err(_) => Err("Please enter a valid URL (e.g. example.com)".to_string()),
    }
}

fn validate_entry_form(form: &EntryForm) -> HashMap<String, String> {
    let mut errors = HashMap::new();

    if form.duration < 1 {
        errors.insert("duration".to_string(), "Duration must be at least 1".to_string());
    }

    if !form.url.is_empty() {
        if let Err(msg) = normalize_url(&form.url) {
            errors.insert("url".to_string(), msg);
        }
    }

    if form.title.trim().is_empty() {
        errors.insert("title".to_string(), "Title is required".to_string());
    }

    if form.title.len() > 500 {
        errors.insert("title".to_string(), "Title must be under 500 characters".to_string());
    }

    if let Some(ref desc) = form.description {
        if desc.len() > 5000 {
            errors.insert("description".to_string(), "Description must be under 5000 characters".to_string());
        }
    }

    errors
}
```

**Step 2: Update create_entry and update_entry to use new normalize_url**

In both `create_entry` (around line 450) and `update_entry` (around line 595), the current code does `form.url = normalize_url(&form.url);` which returns a `String`. Now `normalize_url` returns `Result<String, String>`, so the handlers need to normalize after validation passes.

In `create_entry`, find the line after validation passes (after the `if !errors.is_empty()` block closes), before the SQL insert. Currently it uses `&form.url` in the bind. Change the SQL bind section to normalize first:

Find (in `create_entry`, after the error-handling block):
```rust
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();

    let collection_id = form.collection_id.filter(|s| !s.is_empty());

    sqlx::query(
        ...
    )
    .bind(&id)
    .bind(&user.id)
    .bind(&collection_id)
    .bind(&form.url)
```

Replace `.bind(&form.url)` with `.bind(&normalize_url(&form.url).unwrap())` — this is safe because validation already passed.

Also remove the `form.url = normalize_url(&form.url);` line that currently exists before validation in `create_entry`.

Do the same in `update_entry`: remove the `form.url = normalize_url(&form.url);` line before validation, and change `.bind(&form.url)` in the UPDATE query to `.bind(&normalize_url(&form.url).unwrap())`.

Since `form` no longer needs to be mutated, change `Form(mut form)` back to `Form(form)` in both handlers.

**Step 3: Run tests to verify they pass**

Run: `cargo test --test entries`
Expected: All tests pass, including the new URL normalization tests.

**Step 4: Commit**

```
git add src/routes/entries.rs
git commit -m "feat(entries): use url crate for URL normalization and validation"
```

---

### Task 4: Disable browser validation and add client-side JS validation

**Files:**
- Modify: `templates/entries/form.html`

**Step 1: Add `novalidate` and change URL input to `type="text"`**

In `templates/entries/form.html`, find:

```html
    <form id="entry-form" method="post" action="{% if let Some(e) = entry %}/entries/{{ e.id }}{% else %}/entries{% endif %}">
```

Replace with:

```html
    <form id="entry-form" method="post" action="{% if let Some(e) = entry %}/entries/{{ e.id }}{% else %}/entries{% endif %}" novalidate>
```

Find the URL input:

```html
            <input
                type="url"
                id="url"
                name="url"
                required
                autofocus
                value="{% if let Some(e) = entry %}{{ e.url }}{% endif %}"
                placeholder="example.com"
            >
```

Replace with:

```html
            <input
                type="text"
                id="url"
                name="url"
                autofocus
                value="{% if let Some(e) = entry %}{{ e.url }}{% endif %}"
                placeholder="example.com"
            >
```

Remove `required` from the title input as well (find `type="text"` id `title` with `required`):

```html
            <input
                type="text"
                id="title"
                name="title"
                value="{% if let Some(e) = entry %}{{ e.title }}{% endif %}"
            >
```

Remove `required` and `min="1"` from the duration input:

```html
            <input
                type="number"
                id="duration"
                name="duration"
                value="{% if let Some(e) = entry %}{{ e.duration }}{% else %}1{% endif %}"
            >
```

**Step 2: Replace the existing script block with client-side validation**

Find the entire `<script>` block at the bottom of `templates/entries/form.html` and replace it with:

```html
<script>
(function() {
    var form = document.getElementById('entry-form');

    var rules = {
        url: function(v) {
            if (!v.trim()) return 'URL is required';
            // Loose check: must have a dot somewhere in the domain portion
            var s = v.trim();
            if (s.startsWith('http://') || s.startsWith('https://')) {
                s = s.replace(/^https?:\/\//, '');
            }
            var domain = s.split(/[/?#]/)[0];
            if (!domain.includes('.')) return 'Please enter a valid URL (e.g. example.com)';
            return '';
        },
        title: function(v) {
            if (!v.trim()) return 'Title is required';
            if (v.length > 500) return 'Title must be under 500 characters';
            return '';
        },
        duration: function(v) {
            if (!v || parseInt(v, 10) < 1) return 'Duration must be at least 1';
            return '';
        },
        description: function(v) {
            if (v && v.length > 5000) return 'Description must be under 5000 characters';
            return '';
        }
    };

    function showError(name, msg) {
        var input = form.querySelector('[name="' + name + '"]');
        var errDiv = input.closest('.form-group').querySelector('.error-message');
        if (errDiv) errDiv.textContent = msg;
    }

    function validateField(name) {
        var input = form.querySelector('[name="' + name + '"]');
        if (!input || !rules[name]) return true;
        var msg = rules[name](input.value);
        showError(name, msg);
        return !msg;
    }

    // Validate on blur
    ['url', 'title', 'duration', 'description'].forEach(function(name) {
        var input = form.querySelector('[name="' + name + '"]');
        if (input) {
            input.addEventListener('blur', function() { validateField(name); });
        }
    });

    // Validate all on submit
    form.addEventListener('submit', function(e) {
        var valid = true;
        ['url', 'title', 'duration', 'description'].forEach(function(name) {
            if (!validateField(name)) valid = false;
        });
        if (!valid) e.preventDefault();
    });

    // Character counter for description
    var ta = document.getElementById('description');
    var counter = document.getElementById('desc-count');
    var max = 5000;
    function update() {
        var len = ta.value.length;
        if (len > 0) {
            counter.textContent = len + ' / ' + max;
            counter.classList.toggle('near-limit', len >= max * 0.9);
        } else {
            counter.textContent = '';
        }
    }
    ta.addEventListener('input', update);
    update();
})();
</script>
```

**Step 3: Add the duration error-message div if missing**

Check that the duration field has an `.error-message` div. It already does (line 61 in the current template), so no change needed.

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: Compiles. (Template changes don't need compilation, but good to check nothing broke.)

**Step 5: Run all tests**

Run: `cargo test --test entries`
Expected: All tests pass. The server-side validation still works, JS validation is additive.

**Step 6: Commit**

```
git add templates/entries/form.html
git commit -m "feat(entries): add custom client-side form validation, disable browser native validation"
```

---

### Task 5: Clear errors on input

**Files:**
- Modify: `templates/entries/form.html`

When a user starts typing after seeing an error, the error should clear so they get immediate feedback that they're fixing it.

**Step 1: Add input listeners to clear errors**

In the JS validation block (added in Task 4), find the blur listener loop:

```javascript
    // Validate on blur
    ['url', 'title', 'duration', 'description'].forEach(function(name) {
        var input = form.querySelector('[name="' + name + '"]');
        if (input) {
            input.addEventListener('blur', function() { validateField(name); });
        }
    });
```

Replace with:

```javascript
    // Validate on blur, clear on input
    ['url', 'title', 'duration', 'description'].forEach(function(name) {
        var input = form.querySelector('[name="' + name + '"]');
        if (input) {
            input.addEventListener('blur', function() { validateField(name); });
            input.addEventListener('input', function() { showError(name, ''); });
        }
    });
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 3: Commit**

```
git add templates/entries/form.html
git commit -m "fix(entries): clear validation errors on input"
```

---

### Task 6: Fix form spacing — consistent gaps and button breathing room

**Files:**
- Modify: `static/style.css:224-259`

The form currently has tight spacing between the last field and the action buttons. The `.form-actions` margin-top is `0.25rem` while the form gap between fields is `0.75rem`. The buttons need more breathing room to feel consistent.

**Step 1: Update form spacing in CSS**

In `static/style.css`, find the `.form-actions` rule (around line 254):

```css
.form-actions {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-top: 0.25rem;
}
```

Replace with:

```css
.form-actions {
    display: flex;
    gap: 1rem;
    align-items: center;
    margin-top: 1rem;
}
```

**Step 2: Verify it compiles**

Run: `cargo build`
Expected: Compiles.

**Step 3: Commit**

```
git add static/style.css
git commit -m "fix(css): increase spacing between form fields and action buttons"
```

---

### Task 7: Manual testing checklist

No files to modify. Verify the full flow works end-to-end.

**Step 1: Start the server**

Run: `cargo run`

**Step 2: Test these scenarios in the browser**

1. Type `yahoo.com` in URL, fill in title + duration → should submit successfully, entry saved with `https://yahoo.com/` in DB
2. Type `https://example.com/path` → should submit, stored as-is
3. Type `http://example.com` → should submit, stored as `http://example.com/`
4. Type `yahoo` → should show inline error "Please enter a valid URL (e.g. example.com)" on blur
5. Leave URL empty → should show "URL is required" on blur
6. Leave title empty → should show "Title is required" on blur
7. Set duration to 0 → should show "Duration must be at least 1" on blur
8. Type in URL field after seeing error → error clears
9. Submit with multiple errors → all show at once, form doesn't submit

**Step 3: Run the full test suite**

Run: `cargo test`
Expected: All tests pass.

**Step 4: Commit (if any fixes needed)**

Only if manual testing revealed issues that needed fixing.
