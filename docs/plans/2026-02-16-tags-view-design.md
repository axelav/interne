# Tags View Design

## Overview

Add a tag cloud index page and tag detail drill-down page. Tags already exist in the data model (`tags`, `entry_tags` tables) and are assigned via entry create/edit forms. This feature adds browsing and filtering by tag.

## Tag Cloud Index (`/tags`)

- New nav item "Tags" between "Links" and "Collections"
- Queries all tags belonging to the logged-in user's entries, with entry counts
- Tags sorted alphabetically, rendered as flowing inline words (no grid, no boxes)
- Font size scales from ~0.75rem (1 entry) to ~2.5rem (most-used tag), mapped logarithmically so outliers don't crush everything else
- Color scales on the same curve: lightest cool tone (light teal) for low-frequency, deepest (indigo/purple) for high-frequency
- Each tag is an `<a>` linking to `/tags/:name`
- Empty state: "No tags yet."

## Tag Detail Page (`/tags/:name`)

- Tag name as heading with entry count, e.g. "rust (12)"
- All entries with that tag shown regardless of availability status
- Uses the existing `entries/entry.html` partial for entry cards
- Cards retain available/hidden visual styling
- Back link at bottom: "Back to tags" linking to `/tags`
- No filter bar, no tag edit/delete actions

## Implementation Scope

- New routes: `GET /tags` (index), `GET /tags/:name` (detail)
- New templates: `templates/tags/list.html` (cloud), `templates/tags/show.html` (detail)
- CSS additions: `.tag-cloud` container, size/color scaling
- Nav update in `base.html`: add "Tags" link with active state
- No new DB tables or migrations — queries existing `tags` and `entry_tags` joined to `entries`, scoped by user ID
