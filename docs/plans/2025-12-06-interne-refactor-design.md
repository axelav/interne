# Interne Refactor Design

**Date:** 2025-12-06
**Author:** Design Session

## Overview

Interne is a spaced-repetition bookmark manager that resurfaces saved websites after configurable intervals using an entropy-based algorithm. This document outlines the refactor from a Next.js client-only app to a multi-user application with persistent storage.

## Current State

The app runs entirely in the browser with localStorage for persistence. Next.js 12 provides the framework, webpack handles bundling, and JavaScript comprises the codebase. One user per browser instance.

**Architecture:**
- Single-page React app (pages/index.js)
- Components: CreateEntryForm, Header, Footer, form elements
- Services: localStorage operations
- Utils: entropy calculation, date formatting, constants

**Data model:**
Each entry stores URL, title, description, duration, interval (hours/days/weeks/months/years), visit count, creation timestamp, update timestamp, and dismissal timestamp. The entropy algorithm calculates when to resurface each entry.

## Goals

1. Replace Next.js with a simpler framework
2. Add backend with persistent database (SQLite)
3. Support multiple users with authentication
4. Switch from webpack to Vite
5. Convert codebase to TypeScript
6. Maintain current UI and UX

## Architecture

### Stack

**Backend:** TrailBase (single-executable Rust server)
- Includes SQLite database
- Provides built-in authentication (password + OAuth)
- Generates REST APIs automatically
- Offers real-time sync capabilities
- Ships with admin dashboard

**Frontend:** Vite + React 18 + TypeScript
- Fast development server with HMR
- Built-in TypeScript support
- CSS modules (no changes needed)
- TrailBase TypeScript SDK for API calls

**Database:** SQLite (managed by TrailBase)
- Single-file persistence
- Simple backups (copy the .db file)
- Sufficient for hundreds of users

### Project Structure

```
interne/
├── frontend/
│   ├── src/
│   │   ├── components/     # Existing React components
│   │   ├── services/       # API client for TrailBase
│   │   ├── hooks/          # React Query hooks
│   │   ├── types/          # TypeScript interfaces
│   │   ├── utils/          # Existing utilities (entropy, date)
│   │   ├── styles/         # Existing CSS modules
│   │   ├── App.tsx         # Main component
│   │   └── main.tsx        # Entry point
│   ├── index.html
│   ├── package.json
│   ├── pnpm-lock.yaml
│   ├── vite.config.ts
│   └── tsconfig.json
├── backend/
│   ├── trailbase            # TrailBase executable
│   ├── config.json          # TrailBase configuration
│   └── migrations/          # Database schema
├── Dockerfile
├── docker-compose.yml
└── README.md
```

## Database Schema

SQLite schema for the entries table:

```sql
CREATE TABLE entries (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  url TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  duration INTEGER NOT NULL,
  interval TEXT NOT NULL,
  visited INTEGER DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME,
  dismissed_at DATETIME,
  FOREIGN KEY (user_id) REFERENCES _user(id) ON DELETE CASCADE
);

CREATE INDEX idx_entries_user_id ON entries(user_id);
CREATE INDEX idx_entries_dismissed_at ON entries(dismissed_at);
```

**Field descriptions:**
- `id`: Backend-generated UUID (replaces client-side uuid generation)
- `user_id`: References TrailBase's built-in `_user` table
- `url`: Website URL (normalized via URL constructor)
- `title`: User-provided title
- `description`: Optional description
- `duration`: Number of time units (e.g., 3)
- `interval`: Time unit ('hours', 'days', 'weeks', 'months', 'years')
- `visited`: Count of times user visited the URL
- `created_at`: Entry creation timestamp (auto-set)
- `updated_at`: Last modification timestamp
- `dismissed_at`: Last time user marked as read or clicked the link

**Computed fields:**
The `availableAt` and `visible` fields remain client-side calculations. The entropy algorithm runs in the frontend, computing when each entry should resurface based on `dismissed_at`, `duration`, and `interval`. This preserves existing logic and keeps the database normalized.

## Authentication

TrailBase provides authentication out of the box. We start with email/password, with OAuth (Google, Discord) available for future enhancement.

**User flow:**
1. User visits app
2. If unauthenticated, show login page
3. User logs in or registers
4. TrailBase returns JWT access token + refresh token
5. Frontend stores tokens (httpOnly cookies recommended)
6. All API requests include Authorization header
7. TrailBase validates token and injects user_id into queries

**Access control:**
TrailBase uses ACL rules to isolate user data:

```json
{
  "entries": {
    "read": "user_id = auth.user_id",
    "create": "user_id = auth.user_id",
    "update": "user_id = auth.user_id",
    "delete": "user_id = auth.user_id"
  }
}
```

Users see only their own entries. No cross-user data leakage.

**Frontend implementation:**

```typescript
// services/auth.ts
export async function login(email: string, password: string) {
  const response = await trailbase.auth.signIn({ email, password })
  return response.user
}

export async function register(email: string, password: string) {
  const response = await trailbase.auth.signUp({ email, password })
  return response.user
}

export async function logout() {
  await trailbase.auth.signOut()
}
```

Wrap the app in an auth context provider. Check authentication state before rendering the main view. Show login form for unauthenticated users.

## API Integration

TrailBase auto-generates REST APIs from the database schema. Standard CRUD operations work immediately.

**Entry operations:**

```typescript
// services/entries.ts
export async function fetchEntries() {
  // GET /api/records/v1/entries
  // Filtered by user_id automatically
  return await trailbase.records('entries').list()
}

export async function createEntry(entry: CreateEntryInput) {
  // POST /api/records/v1/entries
  // user_id injected by TrailBase
  return await trailbase.records('entries').create(entry)
}

export async function updateEntry(id: string, updates: Partial<Entry>) {
  // PATCH /api/records/v1/entries/:id
  return await trailbase.records('entries').update(id, updates)
}

export async function deleteEntry(id: string) {
  // DELETE /api/records/v1/entries/:id
  return await trailbase.records('entries').delete(id)
}
```

Replace all localStorage calls with async API calls. Remove client-side ID generation (TrailBase generates UUIDs).

## State Management

Use React Query for server state management. This provides caching, automatic refetching, optimistic updates, and loading/error states.

**Example hooks:**

```typescript
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query'

export function useEntries() {
  return useQuery({
    queryKey: ['entries'],
    queryFn: fetchEntries
  })
}

export function useUpdateEntry() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: ({ id, updates }) => updateEntry(id, updates),
    onSuccess: () => queryClient.invalidateQueries(['entries'])
  })
}

export function useDeleteEntry() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: deleteEntry,
    onSuccess: () => queryClient.invalidateQueries(['entries'])
  })
}
```

**Component usage:**

```typescript
function Index() {
  const { data: entries, isLoading } = useEntries()
  const updateEntry = useUpdateEntry()
  const deleteEntry = useDeleteEntry()

  // Same component logic, different data source
}
```

React Query handles loading states, error handling, and cache invalidation automatically.

## Frontend Migration

### Remove Next.js Dependencies

**Changes:**
- Remove `next`, `next/head`, `next/link` imports
- Replace `next/head` with direct index.html `<head>` tags (title, favicon)
- Remove `pages/_app.js` (Vite uses `main.tsx` entry point)
- Rename `pages/index.js` to `src/App.tsx`
- Remove `next.config.js`

**Keep:**
- All components (Header, Footer, CreateEntryForm, Forms)
- All CSS modules (Vite supports them natively)
- All utilities (entropy, date, formatters, constants)
- Current UI structure and styling

### TypeScript Conversion

**File renames:**
- `.js` → `.tsx` (components with JSX)
- `.js` → `.ts` (utilities, services)

**Type definitions:**

```typescript
// types/entry.ts
export interface Entry {
  id: string
  user_id: string
  url: string
  title: string
  description: string | null
  duration: number
  interval: 'hours' | 'days' | 'weeks' | 'months' | 'years'
  visited: number
  created_at: string
  updated_at: string | null
  dismissed_at: string | null
  // Computed client-side
  visible?: boolean
  availableAt?: Date
}

export type CreateEntryInput = Omit<Entry, 'id' | 'user_id' | 'created_at' | 'updated_at' | 'visited'>
```

**Component props:**

```typescript
interface CreateEntryFormProps {
  onSubmit: (entry: CreateEntryInput) => void
  entries: Entry[]
  initialValues?: Partial<Entry>
}
```

Type all existing component props, remove PropTypes dependency.

### Dependency Changes

**Remove:**
- next
- webpack (and related packages)
- uuid (backend generates IDs)
- prop-types (TypeScript replaces runtime checks)

**Add:**
- vite
- @vitejs/plugin-react
- @tanstack/react-query
- TrailBase TypeScript SDK
- TypeScript dev dependencies (@types/react, @types/react-dom)

**Keep:**
- react, react-dom
- dayjs
- lodash.omit, lodash.orderby
- @react-aria/button
- react-transition-group

### Vite Configuration

```typescript
// vite.config.ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:4000',
        changeOrigin: true
      }
    }
  }
})
```

Proxy API requests to TrailBase during development.

## Data Migration

Users have existing data in localStorage. Provide a one-time migration on first login.

**Migration flow:**
1. User logs in after upgrade
2. Check localStorage for `INTERIOR_ENTRIES` key
3. If found, prompt: "Import existing bookmarks?"
4. If accepted, bulk create entries via API
5. Clear localStorage on success

**Implementation:**

```typescript
async function migrateFromLocalStorage() {
  const stored = localStorage.getItem('INTERIOR_ENTRIES')
  if (!stored) return

  const localEntries = JSON.parse(stored)
  const results = await Promise.allSettled(
    localEntries.map(entry => createEntry(entry))
  )

  const successful = results.filter(r => r.status === 'fulfilled').length

  if (successful === localEntries.length) {
    localStorage.removeItem('INTERIOR_ENTRIES')
    localStorage.removeItem('scrollY')
  }

  return { total: localEntries.length, imported: successful }
}
```

Show success message with import count. Handle partial failures gracefully.

## Development Workflow

**Setup:**
```bash
# Install TrailBase
cd backend
# Download TrailBase executable for your platform
chmod +x trailbase

# Install frontend dependencies
cd frontend
pnpm install
```

**Run development servers:**
```bash
# Terminal 1: TrailBase backend
cd backend
./trailbase

# Terminal 2: Vite frontend
cd frontend
pnpm dev
```

TrailBase runs on port 4000 (default). Vite runs on port 5173 with proxy to TrailBase API.

**Configuration:**
TrailBase uses `backend/config.json` for database path, auth settings, CORS, etc. Defaults work for development.

## Deployment

Use Docker for consistent deployment across environments.

**Dockerfile:**

```dockerfile
# Build frontend
FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend

RUN npm install -g pnpm
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile

COPY frontend/ ./
RUN pnpm run build

# Runtime
FROM debian:bookworm-slim
WORKDIR /app

COPY backend/trailbase /usr/local/bin/trailbase
RUN chmod +x /usr/local/bin/trailbase

COPY --from=frontend-builder /app/frontend/dist /app/static
COPY backend/config.json /app/config.json

EXPOSE 4000

CMD ["trailbase", "--static-files", "/app/static", "--config", "/app/config.json"]
```

**docker-compose.yml:**

```yaml
version: '3.8'

services:
  interne:
    build: .
    ports:
      - "4000:4000"
    volumes:
      - ./data:/app/data
    environment:
      - DATABASE_PATH=/app/data/interne.db
    restart: unless-stopped
```

**Deployment steps:**
1. Build Docker image: `docker-compose build`
2. Start container: `docker-compose up -d`
3. Access app at http://localhost:4000
4. Database persists in `./data/interne.db`

**Backups:**
```bash
docker-compose exec interne cp /app/data/interne.db /app/data/backup-$(date +%Y%m%d).db
```

Copy database file to external storage regularly.

**VPS deployment:**
Transfer docker-compose.yml and built image to VPS. Run `docker-compose up -d`. Configure reverse proxy (nginx) for HTTPS and custom domain.

## UI Preservation

The refactor changes only the data layer and authentication. The UI remains identical.

**No changes to:**
- Component layout and structure
- CSS modules and styling
- Entropy calculation algorithm
- Date formatting and display
- Keyboard shortcuts (ESC to toggle filter)
- Search/filter functionality
- Entry cards and their interactions
- "Mark Read", "Edit", "Delete" buttons

**New UI elements:**
- Login/register page (shown when unauthenticated)
- Loading spinners during API calls
- Error messages for failed operations
- Migration prompt for localStorage import

Users familiar with the current app will find the same interface, now with cloud sync and multi-device access.

## Implementation Phases

**Phase 1: Backend Setup**
- Install and configure TrailBase
- Create database schema
- Configure authentication
- Test CRUD operations via TrailBase admin UI

**Phase 2: Frontend Foundation**
- Create Vite project structure
- Set up TypeScript configuration
- Install dependencies (React Query, TrailBase SDK)
- Configure API proxy

**Phase 3: Migration**
- Convert existing components to TypeScript
- Replace localStorage with API calls
- Implement authentication flow
- Add React Query hooks

**Phase 4: Testing**
- Test authentication (login, register, logout)
- Test CRUD operations on entries
- Verify entropy calculations
- Test data migration from localStorage

**Phase 5: Docker & Deployment**
- Create Dockerfile
- Create docker-compose.yml
- Test local Docker build
- Deploy to VPS

**Phase 6: Verification**
- Verify multi-user isolation
- Test backups and restore
- Load testing with multiple users
- Monitor performance

## Success Criteria

The refactor succeeds when:

1. Multiple users can register and use the app independently
2. Each user sees only their own entries
3. All existing features work (search, filter, mark read, edit, delete)
4. Entropy algorithm resurfaces entries correctly
5. Data persists across server restarts
6. Docker deployment works on VPS
7. Backups can be created and restored
8. No localStorage dependencies remain
9. TypeScript catches type errors at compile time
10. UI matches current design pixel-perfect

## Open Questions

**TrailBase specifics:**
- Exact SDK API surface (consult TrailBase docs during implementation)
- OAuth provider configuration (if implementing beyond email/password)
- Real-time sync capabilities (future enhancement possibility)

**Performance:**
- Entry limit per user (SQLite handles millions of rows, pagination unlikely needed)
- Concurrent user capacity (test during deployment)

**Future enhancements:**
- Browser extension for quick bookmark saves
- Mobile app using TrailBase API
- Export/import functionality
- Tags and categories
- Shared collections between users

These questions resolve during implementation with TrailBase documentation and testing.
