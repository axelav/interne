# PocketBase Setup

## First-time Setup

1. Start PocketBase: `docker compose up -d`
2. Open admin UI: <http://localhost:8090/\_/>
3. Create admin account

## Create Entries Collection

1. Go to Collections → New Collection
2. Name: `entries`
3. Add fields:
   - `user` - Relation (users, single, required)
   - `url` - URL (required)
   - `title` - Text (required)
   - `description` - Text
   - `duration` - Number (required, min: 1)
   - `interval` - Select (options: hours, days, weeks, months, years, required)
   - `visited` - Number (default: 0)
   - `dismissed` - DateTime

## Set API Rules

In the `entries` collection settings → API Rules:

- List/Search: `@request.auth.id = user`
- View: `@request.auth.id = user`
- Create: `@request.auth.id != ""`
- Update: `@request.auth.id = user`
- Delete: `@request.auth.id = user`

## Disable Email Verification

1. Go to Settings → Auth providers → Email/Password
2. Disable "Require email verification"
