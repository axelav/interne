# UI Fixes Plan

## Current Issues

### 1. Link clicks don't mark entries as visited
**Problem:** Clicking the entry title link opens the URL in a new tab, but doesn't mark the entry as visited. Only clicking "Mark Read" does.

**Fix:** Add JavaScript onclick handler that calls the visit endpoint while still allowing the link to open normally.

### 2. Description not visible
**Problem:** Entries have a `description` field but it's not displayed in the entry card.

**Fix:** Add description rendering to entry.html, styled as secondary text below the title.

### 3. View toggle positioning
**Problem:** The "View All"/"View Available" toggle is positioned with `position: fixed; left: -2rem` which puts it mostly off-screen.

**Decision:** Place next to the logo in the header.

### 4. Header layout
**Problem:** Header has logo, header_actions block, and date. The `+ Add` link sits between logo and date but the relationship between elements isn't clear.

**Current structure:**
```
[Logo] [+ Add] [Date]
```

**Decision:** Keep everything currently in the header and add the view toggle.

### 5. Footer styling inconsistent
**Problem:** Footer has "Export" as a link and "Logout" as a button inside a form. The button styling (via `.link-button`) conflicts with `button[type="submit"]` styles, making Logout appear as a black filled button instead of a text link.

**Fix:** Make `.link-button` more specific to override submit button styles, or restructure the logout to not be a submit button.

### 6. Entry card layout
**Problem:** Cards currently stack elements vertically which wastes horizontal space.

**Current structure:**
```
[Title →]                    [Mark Read] [Edit]
[3 days ago · Available in 2 days]
```

**Decision:** Keep two-row layout, add truncated descriptions.

## Proposed Changes (pending answers)

Each fix should be a separate commit:

1. `fix: mark entry visited when clicking title link`
2. `fix: display entry description in cards`
3. `fix: reposition view toggle to [location TBD]`
4. `fix: improve header layout`
5. `fix: consistent footer link styling`
6. `fix: entry card layout [details TBD]`

## Decisions

1. View toggle goes next to the logo in the header
2. Header keeps current contents + view toggle
3. Two-row entry card layout with truncated descriptions
4. General design polish pass
