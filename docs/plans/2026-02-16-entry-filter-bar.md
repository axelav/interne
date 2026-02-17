# Entry Filter Bar Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Replace the "View All / View Available" toggle with a four-option filter bar: Available, Hidden, No Visits, All.

**Architecture:** Refactor the two existing list handlers into a shared helper that accepts a filter enum. Add two new routes (`/hidden`, `/no-visits`). Update the template to render a filter bar with htmx navigation. Style the active filter like the active nav item.

**Tech Stack:** Rust/Axum, Askama templates, htmx, CSS

---

### Task 1: Refactor list handlers into shared helper

**Files:**
- Modify: `src/routes/entries.rs:267-334`

**Step 1: Add a `build_entry_list` helper function**

Add this function right after `fetch_entries_for_user` (after line 265):

```rust
fn build_entry_view(entry: Entry, visit_count: i64, now: DateTime<Utc>) -> EntryView {
    let (is_available, available_in) = calculate_availability(&entry, now);
    EntryView {
        id: entry.id,
        url: entry.url,
        title: entry.title,
        description: entry.description,
        last_viewed: format_last_viewed(&entry.dismissed_at, now),
        available_in,
        is_available,
        visit_count,
    }
}

async fn list_filtered_entries(
    db: &sqlx::SqlitePool,
    user: User,
    filter: &str,
) -> Result<impl IntoResponse, AppError> {
    let entries = fetch_entries_for_user(db, &user.id).await;
    let now = Utc::now();

    let entry_views: Vec<EntryView> = entries
        .into_iter()
        .map(|(entry, visit_count)| build_entry_view(entry, visit_count, now))
        .filter(|ev| match filter {
            "available" => ev.is_available,
            "hidden" => !ev.is_available,
            "no-visits" => ev.visit_count == 0,
            _ => true, // "all"
        })
        .collect();

    let template = EntryListTemplate {
        entries: entry_views,
        filter: filter.to_string(),
        static_hash: crate::STATIC_HASH,
        user: Some(user),
    };
    Ok(Html(template.render()?))
}
```

**Step 2: Rewrite the four route handlers to delegate to `list_filtered_entries`**

Replace `list_entries` and `list_all_entries` (lines 267-334) with:

```rust
async fn list_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "available").await
}

async fn list_all_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "all").await
}

async fn list_hidden_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "hidden").await
}

async fn list_no_visits_entries(
    State(state): State<AppState>,
    AuthUser(user): AuthUser,
) -> Result<impl IntoResponse, AppError> {
    list_filtered_entries(&state.db, user, "no-visits").await
}
```

**Step 3: Register the two new routes**

In the `router()` function (line 134-144), add two new routes after the `/all` line:

```rust
.route("/hidden", get(list_hidden_entries))
.route("/no-visits", get(list_no_visits_entries))
```

**Step 4: Verify it compiles**

Run: `cargo build`
Expected: compiles successfully

**Step 5: Commit**

```bash
git add src/routes/entries.rs
git commit -m "refactor(entries): extract shared list_filtered_entries helper"
```

---

### Task 2: Update the list template with filter bar

**Files:**
- Modify: `templates/entries/list.html`

**Step 1: Replace the view toggle with filter bar**

Replace the entire `header_left` block (lines 5-19) with:

```html
{% block header_left %}
<div id="view-filter" class="view-filter">
    View
    {% for f in [("available", "Available", "/"), ("hidden", "Hidden", "/hidden"), ("no-visits", "No Visits", "/no-visits"), ("all", "All", "/all")] %}
        {% if filter == f.0 %}
            <span class="view-filter-link active">{{ f.1 }}</span>
        {% else %}
            <a
                href="{{ f.2 }}"
                class="view-filter-link"
                hx-get="{{ f.2 }}"
                hx-target="#entry-list"
                hx-select="#entry-list"
                hx-select-oob="#view-filter"
                hx-swap="outerHTML"
                hx-push-url="true"
            >{{ f.1 }}</a>
        {% endif %}
        {% if !loop.last %}/{% endif %}
    {% endfor %}
</div>
{% endblock %}
```

**Note:** Askama may not support inline array literals in `for` loops. If this doesn't compile, use explicit conditionals instead:

```html
{% block header_left %}
<div id="view-filter" class="view-filter">
    View
    {% if filter == "available" %}<span class="view-filter-link active">Available</span>{% else %}<a href="/" class="view-filter-link" hx-get="/" hx-target="#entry-list" hx-select="#entry-list" hx-select-oob="#view-filter" hx-swap="outerHTML" hx-push-url="true">Available</a>{% endif %}
    /
    {% if filter == "hidden" %}<span class="view-filter-link active">Hidden</span>{% else %}<a href="/hidden" class="view-filter-link" hx-get="/hidden" hx-target="#entry-list" hx-select="#entry-list" hx-select-oob="#view-filter" hx-swap="outerHTML" hx-push-url="true">Hidden</a>{% endif %}
    /
    {% if filter == "no-visits" %}<span class="view-filter-link active">No Visits</span>{% else %}<a href="/no-visits" class="view-filter-link" hx-get="/no-visits" hx-target="#entry-list" hx-select="#entry-list" hx-select-oob="#view-filter" hx-swap="outerHTML" hx-push-url="true">No Visits</a>{% endif %}
    /
    {% if filter == "all" %}<span class="view-filter-link active">All</span>{% else %}<a href="/all" class="view-filter-link" hx-get="/all" hx-target="#entry-list" hx-select="#entry-list" hx-select-oob="#view-filter" hx-swap="outerHTML" hx-push-url="true">All</a>{% endif %}
</div>
{% endblock %}
```

**Step 2: Update empty state messages**

Replace the empty state block (lines 29-35) with:

```html
<p class="empty">
    {% if filter == "available" %}
        Nothing due. Go outside!
    {% elif filter == "hidden" %}
        Nothing hidden. Everything is due!
    {% elif filter == "no-visits" %}
        No unvisited links.
    {% else %}
        No links yet. Add one!
    {% endif %}
</p>
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles successfully

**Step 4: Commit**

```bash
git add templates/entries/list.html
git commit -m "feat(entries): replace view toggle with four-option filter bar"
```

---

### Task 3: Style the filter bar

**Files:**
- Modify: `static/style.css`
- Modify: `templates/base.html`

**Step 1: Replace `.view-toggle` styles with `.view-filter` styles**

Replace the existing `.view-toggle` block (lines 81-89) with:

```css
.view-filter {
    font-size: 0.8125rem;
    color: var(--gray-400);
    display: flex;
    align-items: center;
    gap: 0.25rem;
}

.view-filter-link {
    color: var(--gray-400);
    text-decoration: none;
}

.view-filter-link:hover {
    color: var(--gray-600);
}

.view-filter-link.active {
    color: var(--gray-600);
}
```

**Step 2: Update the nav active-path script in `templates/base.html`**

Update the path check on line 66 to include the new filter paths:

```javascript
if ((href === '/' && (path === '/' || path === '/all' || path === '/hidden' || path === '/no-visits')) ||
```

**Step 3: Verify it compiles**

Run: `cargo build`
Expected: compiles successfully

**Step 4: Commit**

```bash
git add static/style.css templates/base.html
git commit -m "feat(entries): style filter bar with active state"
```

---

### Verification

1. Run `cargo build` — should compile clean
2. Run `cargo run` — start the server
3. Visit `http://localhost:3000/` — should show "View Available / Hidden / No Visits / All" with "Available" styled as active
4. Click each filter link — entries should filter correctly, active style should move, URL should update
5. htmx should swap content without full page reload
6. "Links" nav item should stay active on all four filter paths
