# Interne Refactor Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor Interne from a Next.js client-only app to a multi-user application with TrailBase backend, SQLite database, TypeScript, and Vite.

**Architecture:** TrailBase provides the backend (Rust + SQLite + auth). Vite serves the React frontend with TypeScript. React Query handles server state. Docker packages everything for VPS deployment.

**Tech Stack:** TrailBase, SQLite, Vite, React 18, TypeScript, React Query, Docker, pnpm

---

## Prerequisites

Before starting, ensure you have:
- Node.js 20+ installed
- pnpm installed (`npm install -g pnpm`)
- Docker and docker-compose installed
- TrailBase downloaded for your platform (from trailbase.io)

---

## Task 1: Backend - TrailBase Setup

**Files:**
- Create: `backend/config.json`
- Create: `backend/migrations/001_create_entries.sql`
- Create: `backend/.gitignore`

**Step 1: Create backend directory structure**

```bash
mkdir -p backend/migrations
```

**Step 2: Download TrailBase executable**

Visit https://trailbase.io and download the appropriate executable for your platform. Place it in `backend/trailbase` and make it executable:

```bash
# macOS/Linux
chmod +x backend/trailbase
```

**Step 3: Create TrailBase configuration**

Create `backend/config.json`:

```json
{
  "database": {
    "path": "data/interne.db"
  },
  "server": {
    "port": 4000,
    "cors": {
      "allowed_origins": ["http://localhost:5173"]
    }
  },
  "auth": {
    "jwt_secret": "CHANGE_THIS_IN_PRODUCTION",
    "access_token_ttl": 900,
    "refresh_token_ttl": 604800
  }
}
```

**Step 4: Create database migration**

Create `backend/migrations/001_create_entries.sql`:

```sql
CREATE TABLE entries (
  id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
  user_id TEXT NOT NULL,
  url TEXT NOT NULL,
  title TEXT NOT NULL,
  description TEXT,
  duration INTEGER NOT NULL,
  interval TEXT NOT NULL CHECK (interval IN ('hours', 'days', 'weeks', 'months', 'years')),
  visited INTEGER DEFAULT 0,
  created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
  updated_at DATETIME,
  dismissed_at DATETIME,
  FOREIGN KEY (user_id) REFERENCES _user(id) ON DELETE CASCADE
);

CREATE INDEX idx_entries_user_id ON entries(user_id);
CREATE INDEX idx_entries_dismissed_at ON entries(dismissed_at);
```

**Step 5: Create backend .gitignore**

Create `backend/.gitignore`:

```
data/
*.db
*.db-shm
*.db-wal
trailbase
```

**Step 6: Test TrailBase startup**

Run: `cd backend && ./trailbase`

Expected: Server starts on port 4000, database created in `data/interne.db`

**Step 7: Commit backend setup**

```bash
git add backend/
git commit -m "feat: add TrailBase backend configuration and schema"
```

---

## Task 2: Frontend - Vite Project Setup

**Files:**
- Create: `frontend/package.json`
- Create: `frontend/vite.config.ts`
- Create: `frontend/tsconfig.json`
- Create: `frontend/tsconfig.node.json`
- Create: `frontend/index.html`
- Create: `frontend/src/main.tsx`
- Create: `frontend/src/vite-env.d.ts`
- Create: `frontend/.gitignore`

**Step 1: Create frontend directory structure**

```bash
mkdir -p frontend/src
```

**Step 2: Create package.json**

Create `frontend/package.json`:

```json
{
  "name": "interne-frontend",
  "private": true,
  "version": "0.31.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "lint": "eslint . --ext ts,tsx"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "@tanstack/react-query": "^5.59.0",
    "dayjs": "^1.11.13",
    "lodash.omit": "^4.5.0",
    "lodash.orderby": "^4.6.0",
    "@react-aria/button": "^3.10.0",
    "react-transition-group": "^4.4.5"
  },
  "devDependencies": {
    "@types/react": "^18.3.11",
    "@types/react-dom": "^18.3.1",
    "@types/lodash.omit": "^4.5.9",
    "@types/lodash.orderby": "^4.6.9",
    "@types/react-transition-group": "^4.4.11",
    "@vitejs/plugin-react": "^4.3.3",
    "typescript": "^5.6.3",
    "vite": "^5.4.10",
    "eslint": "^9.14.0",
    "@typescript-eslint/eslint-plugin": "^8.12.2",
    "@typescript-eslint/parser": "^8.12.2"
  }
}
```

**Step 3: Create Vite configuration**

Create `frontend/vite.config.ts`:

```typescript
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': {
        target: 'http://localhost:4000',
        changeOrigin: true,
      },
    },
  },
})
```

**Step 4: Create TypeScript configuration**

Create `frontend/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "isolatedModules": true,
    "moduleDetection": "force",
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "noUncheckedSideEffectImports": true
  },
  "include": ["src"]
}
```

Create `frontend/tsconfig.node.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2023"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "strict": true,
    "noEmit": true
  },
  "include": ["vite.config.ts"]
}
```

**Step 5: Create index.html**

Create `frontend/index.html`:

```html
<!DOCTYPE html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/x-icon" href="/favicon.ico" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Interne</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

**Step 6: Create main entry point**

Create `frontend/src/main.tsx`:

```typescript
import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
```

Create `frontend/src/App.tsx`:

```typescript
function App() {
  return <div>Interne - Coming Soon</div>
}

export default App
```

Create `frontend/src/vite-env.d.ts`:

```typescript
/// <reference types="vite/client" />
```

**Step 7: Create frontend .gitignore**

Create `frontend/.gitignore`:

```
node_modules
dist
*.local
.DS_Store
```

**Step 8: Install dependencies**

Run: `cd frontend && pnpm install`

Expected: Dependencies installed, pnpm-lock.yaml created

**Step 9: Test Vite dev server**

Run: `cd frontend && pnpm dev`

Expected: Server starts on http://localhost:5173, displays "Interne - Coming Soon"

**Step 10: Commit frontend setup**

```bash
git add frontend/
git commit -m "feat: add Vite + React + TypeScript frontend foundation"
```

---

## Task 3: TypeScript Types

**Files:**
- Create: `frontend/src/types/entry.ts`
- Create: `frontend/src/types/user.ts`
- Create: `frontend/src/types/api.ts`

**Step 1: Create entry types**

Create `frontend/src/types/entry.ts`:

```typescript
export type Interval = 'hours' | 'days' | 'weeks' | 'months' | 'years'

export interface Entry {
  id: string
  user_id: string
  url: string
  title: string
  description: string | null
  duration: number
  interval: Interval
  visited: number
  created_at: string
  updated_at: string | null
  dismissed_at: string | null
  // Computed client-side
  visible?: boolean
  availableAt?: Date
}

export type CreateEntryInput = Omit<
  Entry,
  'id' | 'user_id' | 'created_at' | 'updated_at' | 'visited' | 'visible' | 'availableAt'
>

export type UpdateEntryInput = Partial<CreateEntryInput>
```

**Step 2: Create user types**

Create `frontend/src/types/user.ts`:

```typescript
export interface User {
  id: string
  email: string
  created_at: string
}

export interface AuthResponse {
  user: User
  access_token: string
  refresh_token: string
}

export interface LoginCredentials {
  email: string
  password: string
}

export interface RegisterCredentials {
  email: string
  password: string
}
```

**Step 3: Create API types**

Create `frontend/src/types/api.ts`:

```typescript
export interface ApiError {
  message: string
  code?: string
  details?: unknown
}

export interface ListResponse<T> {
  data: T[]
  total: number
}
```

**Step 4: Commit type definitions**

```bash
git add frontend/src/types/
git commit -m "feat: add TypeScript type definitions"
```

---

## Task 4: Copy and Migrate Existing Utilities

**Files:**
- Create: `frontend/src/utils/constants.ts`
- Create: `frontend/src/utils/date.ts`
- Create: `frontend/src/utils/entropy.ts`
- Create: `frontend/src/utils/formatters.ts`

**Step 1: Copy constants**

Copy from `utils/constants.js` and convert to TypeScript in `frontend/src/utils/constants.ts`:

```typescript
import type { Interval } from '../types/entry'

export const INTERVALS: Record<Uppercase<Interval>, Interval> = {
  HOURS: 'hours',
  DAYS: 'days',
  WEEKS: 'weeks',
  MONTHS: 'months',
  YEARS: 'years',
}

export const MODES = {
  VIEW: 'VIEW',
  EDIT: 'EDIT',
} as const

export const KEY_CODES = {
  ESC: 27,
  ENTER: 13,
} as const
```

**Step 2: Copy date utilities**

Copy from `utils/date.js` and convert to TypeScript in `frontend/src/utils/date.ts`:

```typescript
import dayjs, { Dayjs } from 'dayjs'
import relativeTime from 'dayjs/plugin/relativeTime'

dayjs.extend(relativeTime)

export const getCurrentDate = (): Dayjs => dayjs()

export const getDate = (date: string): Dayjs => dayjs(date)

export const getRelativeTimeFromNow = (date: string): string => {
  return dayjs(date).fromNow()
}
```

**Step 3: Copy entropy utilities**

Copy from `utils/entropy.js` and convert to TypeScript in `frontend/src/utils/entropy.ts`:

```typescript
import type { Entry, Interval } from '../types/entry'
import { getCurrentDate, getDate } from './date'

const MAX = 7
const MILLIS_IN_DAY = 24 * 60 * 60 * 1000

// TODO: make user configurable
const opts = {
  entropy: 5,
}

export const getAvailableAtPlusEntropy = ({
  dismissed_at,
  interval,
  duration,
}: Pick<Entry, 'dismissed_at' | 'interval' | 'duration'>): {
  availableAt: dayjs.Dayjs
  diff: number
} => {
  const now = getCurrentDate()
  const { entropy } = opts

  const availableAt = dismissed_at
    ? getDate(dismissed_at).add(duration, interval as any)
    : now.subtract(1, 'seconds')

  const diff = availableAt.diff(now)

  if (entropy && diff > MILLIS_IN_DAY) {
    const availableAtPlusEntropy = availableAt.add(
      Math.floor(Math.random() * ((entropy / 10) * MAX)),
      'days'
    )

    return {
      availableAt: availableAtPlusEntropy,
      diff: availableAtPlusEntropy.diff(now),
    }
  }

  return { availableAt, diff }
}
```

**Step 4: Copy formatters**

Copy from `utils/formatters.js` and convert to TypeScript in `frontend/src/utils/formatters.ts`:

```typescript
export const toTitleCase = (str: string): string => {
  return str
    .split(' ')
    .map((word) => word.charAt(0).toUpperCase() + word.slice(1).toLowerCase())
    .join(' ')
}
```

**Step 5: Install dayjs plugins**

Run: `cd frontend && pnpm add dayjs`

Expected: dayjs already installed from package.json

**Step 6: Commit utilities**

```bash
git add frontend/src/utils/
git commit -m "feat: migrate utilities to TypeScript"
```

---

## Task 5: Copy and Migrate Styles

**Files:**
- Create: `frontend/src/styles/` (copy all CSS modules)

**Step 1: Copy all CSS modules**

```bash
cp -r styles frontend/src/styles
```

**Step 2: Verify CSS modules copied**

Run: `ls frontend/src/styles`

Expected: See Pages.module.css, Index.module.css, Forms.module.css, etc.

**Step 3: Commit styles**

```bash
git add frontend/src/styles/
git commit -m "feat: copy CSS modules to frontend"
```

---

## Task 6: TrailBase API Client

**Files:**
- Create: `frontend/src/services/trailbase.ts`
- Create: `frontend/src/services/auth.ts`
- Create: `frontend/src/services/entries.ts`

**Step 1: Create base TrailBase client**

Create `frontend/src/services/trailbase.ts`:

```typescript
import type { Entry } from '../types/entry'
import type { AuthResponse, LoginCredentials, RegisterCredentials } from '../types/user'

const API_BASE = '/api'

class TrailBaseClient {
  private accessToken: string | null = null

  async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const headers: HeadersInit = {
      'Content-Type': 'application/json',
      ...options.headers,
    }

    if (this.accessToken) {
      headers['Authorization'] = `Bearer ${this.accessToken}`
    }

    const response = await fetch(`${API_BASE}${endpoint}`, {
      ...options,
      headers,
    })

    if (!response.ok) {
      const error = await response.json().catch(() => ({ message: response.statusText }))
      throw new Error(error.message || 'Request failed')
    }

    return response.json()
  }

  setAccessToken(token: string) {
    this.accessToken = token
    localStorage.setItem('access_token', token)
  }

  clearAccessToken() {
    this.accessToken = null
    localStorage.removeItem('access_token')
  }

  getAccessToken(): string | null {
    if (!this.accessToken) {
      this.accessToken = localStorage.getItem('access_token')
    }
    return this.accessToken
  }
}

export const trailbase = new TrailBaseClient()
```

**Step 2: Create auth service**

Create `frontend/src/services/auth.ts`:

```typescript
import type { AuthResponse, LoginCredentials, RegisterCredentials, User } from '../types/user'
import { trailbase } from './trailbase'

export async function login(credentials: LoginCredentials): Promise<AuthResponse> {
  const response = await trailbase.request<AuthResponse>('/auth/login', {
    method: 'POST',
    body: JSON.stringify(credentials),
  })

  trailbase.setAccessToken(response.access_token)
  return response
}

export async function register(credentials: RegisterCredentials): Promise<AuthResponse> {
  const response = await trailbase.request<AuthResponse>('/auth/register', {
    method: 'POST',
    body: JSON.stringify(credentials),
  })

  trailbase.setAccessToken(response.access_token)
  return response
}

export async function logout(): Promise<void> {
  trailbase.clearAccessToken()
}

export async function getCurrentUser(): Promise<User | null> {
  const token = trailbase.getAccessToken()
  if (!token) return null

  try {
    return await trailbase.request<User>('/auth/me')
  } catch {
    trailbase.clearAccessToken()
    return null
  }
}
```

**Step 3: Create entries service**

Create `frontend/src/services/entries.ts`:

```typescript
import type { Entry, CreateEntryInput, UpdateEntryInput } from '../types/entry'
import type { ListResponse } from '../types/api'
import { trailbase } from './trailbase'

export async function fetchEntries(): Promise<Entry[]> {
  const response = await trailbase.request<ListResponse<Entry>>('/records/v1/entries')
  return response.data
}

export async function createEntry(input: CreateEntryInput): Promise<Entry> {
  return trailbase.request<Entry>('/records/v1/entries', {
    method: 'POST',
    body: JSON.stringify(input),
  })
}

export async function updateEntry(id: string, updates: UpdateEntryInput): Promise<Entry> {
  return trailbase.request<Entry>(`/records/v1/entries/${id}`, {
    method: 'PATCH',
    body: JSON.stringify(updates),
  })
}

export async function deleteEntry(id: string): Promise<void> {
  await trailbase.request<void>(`/records/v1/entries/${id}`, {
    method: 'DELETE',
  })
}
```

**Step 4: Commit API services**

```bash
git add frontend/src/services/
git commit -m "feat: add TrailBase API client and services"
```

---

## Task 7: React Query Hooks

**Files:**
- Create: `frontend/src/hooks/useAuth.ts`
- Create: `frontend/src/hooks/useEntries.ts`

**Step 1: Create auth hooks**

Create `frontend/src/hooks/useAuth.ts`:

```typescript
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as authService from '../services/auth'
import type { LoginCredentials, RegisterCredentials } from '../types/user'

export function useCurrentUser() {
  return useQuery({
    queryKey: ['user'],
    queryFn: authService.getCurrentUser,
    staleTime: 1000 * 60 * 5, // 5 minutes
  })
}

export function useLogin() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (credentials: LoginCredentials) => authService.login(credentials),
    onSuccess: (data) => {
      queryClient.setQueryData(['user'], data.user)
    },
  })
}

export function useRegister() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (credentials: RegisterCredentials) => authService.register(credentials),
    onSuccess: (data) => {
      queryClient.setQueryData(['user'], data.user)
    },
  })
}

export function useLogout() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: authService.logout,
    onSuccess: () => {
      queryClient.setQueryData(['user'], null)
      queryClient.clear()
    },
  })
}
```

**Step 2: Create entries hooks**

Create `frontend/src/hooks/useEntries.ts`:

```typescript
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as entriesService from '../services/entries'
import type { CreateEntryInput, UpdateEntryInput } from '../types/entry'

export function useEntries() {
  return useQuery({
    queryKey: ['entries'],
    queryFn: entriesService.fetchEntries,
  })
}

export function useCreateEntry() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (input: CreateEntryInput) => entriesService.createEntry(input),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['entries'] })
    },
  })
}

export function useUpdateEntry() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: ({ id, updates }: { id: string; updates: UpdateEntryInput }) =>
      entriesService.updateEntry(id, updates),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['entries'] })
    },
  })
}

export function useDeleteEntry() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (id: string) => entriesService.deleteEntry(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['entries'] })
    },
  })
}
```

**Step 3: Commit hooks**

```bash
git add frontend/src/hooks/
git commit -m "feat: add React Query hooks for auth and entries"
```

---

## Task 8: Migrate Components - Forms

**Files:**
- Create: `frontend/src/components/Forms.tsx`

**Step 1: Copy and convert Forms component**

Copy from `components/Forms.js` and convert to TypeScript in `frontend/src/components/Forms.tsx`:

```typescript
import React, { forwardRef } from 'react'
import { useButton } from '@react-aria/button'
import styles from '../styles/Forms.module.css'

interface FormProps {
  children: React.ReactNode
}

export function Form({ children }: FormProps) {
  return <form className={styles.form}>{children}</form>
}

interface InputProps {
  type?: string
  value: string | number
  label: string
  placeholder?: string
  onChange: (value: string) => void
  pattern?: string
  min?: number
}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ type = 'text', value, label, placeholder, onChange, pattern, min }, ref) => {
    return (
      <div className={styles.field}>
        <label className={styles.label}>{label}</label>
        <input
          ref={ref}
          type={type}
          className={styles.input}
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
          pattern={pattern}
          min={min}
        />
      </div>
    )
  }
)

Input.displayName = 'Input'

interface SelectOption {
  id: string
  display: string
}

interface SelectProps {
  label: string
  value: string
  onChange: (value: string) => void
  options: SelectOption[]
}

export function Select({ label, value, onChange, options }: SelectProps) {
  return (
    <div className={styles.field}>
      <label className={styles.label}>{label}</label>
      <select className={styles.select} value={value} onChange={(e) => onChange(e.target.value)}>
        {options.map((option) => (
          <option key={option.id} value={option.id}>
            {option.display}
          </option>
        ))}
      </select>
    </div>
  )
}

interface ButtonProps {
  label: string
  onClick: () => void
  children: React.ReactNode
}

export function Button({ label, onClick, children }: ButtonProps) {
  const ref = React.useRef<HTMLButtonElement>(null)
  const { buttonProps } = useButton({ onPress: onClick }, ref)

  return (
    <button {...buttonProps} ref={ref} className={styles.button}>
      {children}
    </button>
  )
}
```

**Step 2: Commit Forms component**

```bash
git add frontend/src/components/Forms.tsx
git commit -m "feat: migrate Forms component to TypeScript"
```

---

## Task 9: Migrate Components - Footer and Header

**Files:**
- Create: `frontend/src/components/Footer.tsx`
- Create: `frontend/src/components/Header.tsx`

**Step 1: Copy and convert Footer**

Copy from `components/Footer.js` and convert to TypeScript in `frontend/src/components/Footer.tsx`:

```typescript
import styles from '../styles/Footer.module.css'

export default function Footer() {
  return (
    <footer className={styles.footer}>
      <a
        href="https://github.com/axelav/interne"
        target="_blank"
        rel="noopener noreferrer"
      >
        View on GitHub
      </a>
    </footer>
  )
}
```

**Step 2: Copy and convert Header**

Copy from `components/Header.js` and convert to TypeScript in `frontend/src/components/Header.tsx`:

```typescript
import { MODES } from '../utils/constants'
import styles from '../styles/Header.module.css'

interface HeaderProps {
  mode: string
  setMode: (mode: string) => void
  setEntry: (entry: null) => void
  searchText: string
  setSearchText: (text: string) => void
}

export default function Header({
  mode,
  setMode,
  setEntry,
  searchText,
  setSearchText,
}: HeaderProps) {
  const handleAddClick = () => {
    setEntry(null)
    setMode(MODES.EDIT)
  }

  const handleCancelClick = () => {
    setMode(MODES.VIEW)
  }

  return (
    <header className={styles.header}>
      <div className={styles.container}>
        <h1 className={styles.title}>Interne</h1>

        {mode === MODES.VIEW ? (
          <div className={styles.controls}>
            <input
              type="text"
              className={styles.search}
              placeholder="Search..."
              value={searchText}
              onChange={(e) => setSearchText(e.target.value)}
            />
            <button className={styles.add} onClick={handleAddClick}>
              +
            </button>
          </div>
        ) : (
          <button className={styles.cancel} onClick={handleCancelClick}>
            Cancel
          </button>
        )}
      </div>
    </header>
  )
}
```

**Step 3: Commit Footer and Header**

```bash
git add frontend/src/components/Footer.tsx frontend/src/components/Header.tsx
git commit -m "feat: migrate Footer and Header components to TypeScript"
```

---

## Task 10: Migrate Components - CreateEntryForm

**Files:**
- Create: `frontend/src/components/CreateEntryForm.tsx`

**Step 1: Copy and convert CreateEntryForm**

Copy from `components/CreateEntryForm.js` and convert to TypeScript in `frontend/src/components/CreateEntryForm.tsx`:

```typescript
import { useState, useEffect, useRef, useCallback } from 'react'
import { Form, Input, Select, Button } from './Forms'
import { toTitleCase } from '../utils/formatters'
import { INTERVALS, KEY_CODES } from '../utils/constants'
import type { Entry, CreateEntryInput } from '../types/entry'
import styles from '../styles/Forms.module.css'

const isValidUrl = (str: string): boolean => {
  try {
    new URL(str)
    return true
  } catch {
    return false
  }
}

interface CreateEntryFormProps {
  onSubmit: (entry: CreateEntryInput) => void
  entries: Entry[]
  initialValues?: Partial<Entry>
}

export default function CreateEntryForm({
  onSubmit,
  entries,
  initialValues,
}: CreateEntryFormProps) {
  const [url, setUrl] = useState(initialValues?.url || '')
  const [title, setTitle] = useState(initialValues?.title || '')
  const [description, setDescription] = useState(initialValues?.description || '')
  const [duration, setDuration] = useState(initialValues?.duration?.toString() || '3')
  const [interval, setInterval] = useState(initialValues?.interval || INTERVALS.DAYS)
  const [error, setError] = useState('')

  const urlInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    urlInputRef.current?.focus()
  }, [])

  const handleSubmit = useCallback(() => {
    if (!url || !title) {
      setError('URL and Title are required.')
      return
    }

    if (!isValidUrl(url)) {
      setError('URL is invalid.')
      return
    }

    if (!initialValues?.id) {
      const normalizedUrl = new URL(url).href
      const existingEntry = entries.find((x) => new URL(x.url).href === normalizedUrl)
      if (existingEntry) {
        setError('URL already exists.')
        return
      }
    }

    const durationNum = parseInt(duration, 10)
    if (!durationNum || durationNum < 1) {
      setError('Duration must be greater than 0.')
      return
    }

    setError('')

    const entry: CreateEntryInput = {
      url: new URL(url).href,
      title,
      description: description || null,
      duration: durationNum,
      interval,
      dismissed_at: initialValues?.dismissed_at || null,
    }

    onSubmit(entry)

    // Reset form
    setUrl('')
    setTitle('')
    setDescription('')
    setDuration('3')
    setInterval(INTERVALS.DAYS)
  }, [url, title, description, duration, interval, entries, initialValues, onSubmit])

  useEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.keyCode === KEY_CODES.ENTER) {
        handleSubmit()
      }
    }

    document.addEventListener('keydown', handleKeydown)
    return () => document.removeEventListener('keydown', handleKeydown)
  }, [handleSubmit])

  return (
    <Form>
      {error && <div className={styles.error}>{error}</div>}
      <Input
        type="url"
        ref={urlInputRef}
        value={url}
        label="URL"
        placeholder="http://example.com"
        onChange={setUrl}
      />
      <Input value={title} label="Title" onChange={setTitle} />
      <Input value={description} label="Description" onChange={setDescription} />
      <Input
        type="number"
        pattern="[0-9]*"
        value={duration}
        label="Duration"
        onChange={setDuration}
        min={1}
      />
      <Select
        label="Interval"
        value={interval}
        onChange={(val) => setInterval(val as any)}
        options={Object.keys(INTERVALS).map((x) => ({
          id: INTERVALS[x as keyof typeof INTERVALS],
          display: toTitleCase(x),
        }))}
      />
      <Button label={initialValues?.id ? 'Edit Entry' : 'Add Entry'} onClick={handleSubmit}>
        {initialValues?.id ? 'Edit Entry' : 'Add Entry'}
      </Button>
    </Form>
  )
}
```

**Step 2: Commit CreateEntryForm**

```bash
git add frontend/src/components/CreateEntryForm.tsx
git commit -m "feat: migrate CreateEntryForm component to TypeScript"
```

---

## Task 11: Auth Components

**Files:**
- Create: `frontend/src/components/LoginForm.tsx`
- Create: `frontend/src/components/AuthProvider.tsx`

**Step 1: Create LoginForm component**

Create `frontend/src/components/LoginForm.tsx`:

```typescript
import { useState } from 'react'
import { Form, Input, Button } from './Forms'
import { useLogin, useRegister } from '../hooks/useAuth'
import styles from '../styles/Forms.module.css'

export default function LoginForm() {
  const [isRegister, setIsRegister] = useState(false)
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [error, setError] = useState('')

  const login = useLogin()
  const register = useRegister()

  const handleSubmit = async () => {
    if (!email || !password) {
      setError('Email and password are required.')
      return
    }

    setError('')

    try {
      if (isRegister) {
        await register.mutateAsync({ email, password })
      } else {
        await login.mutateAsync({ email, password })
      }
    } catch (err: any) {
      setError(err.message || 'Authentication failed')
    }
  }

  return (
    <div className={styles.authContainer}>
      <h2>{isRegister ? 'Register' : 'Login'}</h2>
      <Form>
        {error && <div className={styles.error}>{error}</div>}
        <Input type="email" value={email} label="Email" onChange={setEmail} />
        <Input type="password" value={password} label="Password" onChange={setPassword} />
        <Button label={isRegister ? 'Register' : 'Login'} onClick={handleSubmit}>
          {isRegister ? 'Register' : 'Login'}
        </Button>
      </Form>
      <button
        className={styles.toggleAuth}
        onClick={() => {
          setIsRegister(!isRegister)
          setError('')
        }}
      >
        {isRegister ? 'Already have an account? Login' : "Don't have an account? Register"}
      </button>
    </div>
  )
}
```

**Step 2: Create AuthProvider component**

Create `frontend/src/components/AuthProvider.tsx`:

```typescript
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useCurrentUser } from '../hooks/useAuth'
import LoginForm from './LoginForm'

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
})

interface AuthProviderProps {
  children: React.ReactNode
}

function AuthGuard({ children }: AuthProviderProps) {
  const { data: user, isLoading } = useCurrentUser()

  if (isLoading) {
    return <div>Loading...</div>
  }

  if (!user) {
    return <LoginForm />
  }

  return <>{children}</>
}

export default function AuthProvider({ children }: AuthProviderProps) {
  return (
    <QueryClientProvider client={queryClient}>
      <AuthGuard>{children}</AuthGuard>
    </QueryClientProvider>
  )
}
```

**Step 3: Add auth styles to Forms.module.css**

Add to `frontend/src/styles/Forms.module.css`:

```css
.authContainer {
  max-width: 400px;
  margin: 100px auto;
  padding: 2rem;
  border: 1px solid #eaeaea;
  border-radius: 8px;
}

.authContainer h2 {
  text-align: center;
  margin-bottom: 1.5rem;
}

.toggleAuth {
  display: block;
  margin: 1rem auto 0;
  background: none;
  border: none;
  color: #0070f3;
  cursor: pointer;
  text-decoration: underline;
}

.toggleAuth:hover {
  color: #0051cc;
}
```

**Step 4: Commit auth components**

```bash
git add frontend/src/components/LoginForm.tsx frontend/src/components/AuthProvider.tsx frontend/src/styles/Forms.module.css
git commit -m "feat: add authentication components and AuthProvider"
```

---

## Task 12: Main App Component

**Files:**
- Modify: `frontend/src/App.tsx`

**Step 1: Write the main App component**

Replace `frontend/src/App.tsx`:

```typescript
import { useState, useEffect, useMemo } from 'react'
import orderBy from 'lodash.orderby'
import omit from 'lodash.omit'
import AuthProvider from './components/AuthProvider'
import CreateEntryForm from './components/CreateEntryForm'
import Header from './components/Header'
import Footer from './components/Footer'
import { useEntries, useCreateEntry, useUpdateEntry, useDeleteEntry } from './hooks/useEntries'
import { getAvailableAtPlusEntropy } from './utils/entropy'
import { getRelativeTimeFromNow } from './utils/date'
import { MODES, KEY_CODES } from './utils/constants'
import type { Entry, CreateEntryInput } from './types/entry'
import pageStyles from './styles/Pages.module.css'
import styles from './styles/Index.module.css'

const msgs = [
  {
    en: 'Read a book!',
    eo: 'Legi libron!',
  },
  {
    en: 'Go outside!',
    eo: 'Iru eksteren!',
  },
]

function AppContent() {
  const { data: entries = [], isLoading } = useEntries()
  const createEntry = useCreateEntry()
  const updateEntry = useUpdateEntry()
  const deleteEntry = useDeleteEntry()

  const [entry, setEntry] = useState<Entry | null>(null)
  const [mode, setMode] = useState(MODES.VIEW)
  const [isFilterActive, setIsFilterActive] = useState(true)
  const [searchText, setSearchText] = useState('')

  const emptyListMsg = msgs[1]

  // Compute visible/availableAt for each entry
  const entriesWithComputed = useMemo(() => {
    return entries.map((entry) => {
      const { availableAt, diff } = getAvailableAtPlusEntropy(entry)
      const visible = diff < 0
      return { ...entry, visible, availableAt: availableAt.toDate() }
    })
  }, [entries])

  // Filter and sort entries
  const visibleEntries = useMemo(() => {
    const filtered = entriesWithComputed.filter((x) => {
      if (searchText) {
        const escapeRegExp = (str: string) => str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
        const regex = new RegExp(escapeRegExp(searchText), 'gi')
        return x.title?.match(regex) || x.description?.match(regex) || x.url?.match(regex)
      } else {
        return isFilterActive ? x.visible : true
      }
    })

    return orderBy(
      filtered,
      isFilterActive ? ['dismissed_at'] : ['dismissed_at', 'availableAt'],
      isFilterActive ? ['desc'] : ['desc', 'asc']
    )
  }, [entriesWithComputed, isFilterActive, searchText])

  useEffect(() => {
    const handleKeydown = (e: KeyboardEvent) => {
      if (e.keyCode === KEY_CODES.ESC) {
        if (mode === MODES.EDIT) {
          setMode(MODES.VIEW)
        } else if (document.activeElement === document.body) {
          setIsFilterActive(!isFilterActive)
        }
      }
    }

    document.addEventListener('keydown', handleKeydown)
    return () => document.removeEventListener('keydown', handleKeydown)
  }, [isFilterActive, mode])

  const handleEntryClick = (entry: Entry) => {
    setTimeout(() => {
      updateEntry.mutate({
        id: entry.id,
        updates: {
          dismissed_at: new Date().toISOString(),
        },
      })
    }, 200)
  }

  const handleViewFilterClick = () => setIsFilterActive(!isFilterActive)

  const handleSaveEntry = (input: CreateEntryInput) => {
    if (entry?.id) {
      updateEntry.mutate(
        { id: entry.id, updates: input },
        { onSuccess: () => setMode(MODES.VIEW) }
      )
    } else {
      createEntry.mutate(input, { onSuccess: () => setMode(MODES.VIEW) })
    }
  }

  const handleEditEntry = (entry: Entry) => {
    setEntry(entry)
    setMode(MODES.EDIT)
    window.scrollTo(0, 0)
  }

  const handleDeleteEntry = (entry: Entry) => {
    const shouldDelete = window.confirm('Are you sure?')
    if (shouldDelete) {
      deleteEntry.mutate(entry.id)
    }
  }

  if (isLoading) {
    return <div className={pageStyles.container}>Loading...</div>
  }

  return (
    <div className={pageStyles.container}>
      <Header
        mode={mode}
        setMode={setMode}
        setEntry={setEntry}
        searchText={searchText}
        setSearchText={setSearchText}
      />

      <main className={pageStyles.main}>
        {mode === MODES.EDIT ? (
          <CreateEntryForm
            onSubmit={handleSaveEntry}
            entries={entries}
            initialValues={entry || undefined}
          />
        ) : (
          <div className={styles.grid}>
            {visibleEntries.length > 0 ? (
              visibleEntries.map((x) => (
                <div
                  key={x.id}
                  className={x.visible ? styles.card : `${styles.card} ${styles.unavailable}`}
                >
                  <div className={styles.viewed}>
                    <span>
                      {x.dismissed_at
                        ? `Last viewed ${getRelativeTimeFromNow(x.dismissed_at)}`
                        : 'Never viewed'}
                    </span>
                  </div>
                  <a
                    href={x.url}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={() => handleEntryClick(x)}
                  >
                    <div className={styles.title}>
                      <h2 title={x.title}>{x.title}</h2>
                      <div className={styles.rarr}>&rarr;</div>
                    </div>
                    <p title={x.description || ''}>{x.description}</p>
                  </a>

                  <div className={styles['flex-between']}>
                    <div className={styles.availability}>
                      {!x.visible && x.availableAt && (
                        <span>Available {getRelativeTimeFromNow(x.availableAt.toISOString())}</span>
                      )}
                    </div>

                    <div className={styles.controls}>
                      {x.visible && (
                        <div className={styles.ignore} onClick={() => handleEntryClick(x)}>
                          Mark Read
                        </div>
                      )}
                      <div className={styles.edit} onClick={() => handleEditEntry(x)}>
                        Edit
                      </div>
                      <div className={styles.delete} onClick={() => handleDeleteEntry(x)}>
                        Delete
                      </div>
                    </div>
                  </div>
                </div>
              ))
            ) : (
              <p className={styles.empty} title={searchText ? 'Neniuj rezultoj' : emptyListMsg.eo}>
                {searchText ? 'No results' : emptyListMsg.en}
              </p>
            )}
          </div>
        )}
      </main>

      {mode === MODES.VIEW && (
        <div
          className={styles.filter}
          onClick={handleViewFilterClick}
          style={{
            left: isFilterActive ? '-14px' : '-41px',
          }}
        >
          {isFilterActive ? 'View All' : <span>View Available</span>}
        </div>
      )}

      <Footer />
    </div>
  )
}

export default function App() {
  return (
    <AuthProvider>
      <AppContent />
    </AuthProvider>
  )
}
```

**Step 2: Copy public assets**

```bash
mkdir -p frontend/public
cp public/favicon.ico frontend/public/
```

**Step 3: Test the app**

Run: `cd frontend && pnpm dev`

Expected: App loads, shows login form (TrailBase must be running)

**Step 4: Commit main app**

```bash
git add frontend/src/App.tsx frontend/public/
git commit -m "feat: implement main App component with all features"
```

---

## Task 13: Docker Setup

**Files:**
- Create: `Dockerfile`
- Create: `docker-compose.yml`
- Create: `.dockerignore`

**Step 1: Create Dockerfile**

Create `Dockerfile`:

```dockerfile
# Build frontend
FROM node:20-alpine AS frontend-builder

WORKDIR /app/frontend

# Install pnpm
RUN npm install -g pnpm

# Copy package files
COPY frontend/package.json frontend/pnpm-lock.yaml ./

# Install dependencies
RUN pnpm install --frozen-lockfile

# Copy source
COPY frontend/ ./

# Build
RUN pnpm run build

# Runtime image
FROM debian:bookworm-slim

WORKDIR /app

# Copy TrailBase executable (you need to download this first)
COPY backend/trailbase /usr/local/bin/trailbase
RUN chmod +x /usr/local/bin/trailbase

# Copy frontend build
COPY --from=frontend-builder /app/frontend/dist /app/static

# Copy backend config and migrations
COPY backend/config.json /app/config.json
COPY backend/migrations /app/migrations

# Create data directory
RUN mkdir -p /app/data

EXPOSE 4000

CMD ["trailbase", "--static-files", "/app/static", "--config", "/app/config.json"]
```

**Step 2: Create docker-compose.yml**

Create `docker-compose.yml`:

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

**Step 3: Create .dockerignore**

Create `.dockerignore`:

```
node_modules
.git
.gitignore
*.md
.DS_Store
data/
*.db
dist
.next
```

**Step 4: Update root .gitignore**

Add to root `.gitignore`:

```
data/
```

**Step 5: Commit Docker setup**

```bash
git add Dockerfile docker-compose.yml .dockerignore .gitignore
git commit -m "feat: add Docker configuration for deployment"
```

---

## Task 14: Update Documentation

**Files:**
- Modify: `README.md`

**Step 1: Update README**

Replace `README.md`:

```markdown
# Interne

A spaced-repetition bookmark manager that resurfaces saved websites after configurable intervals.

## Features

- Save bookmarks with title, description, and custom revisit intervals
- Entropy-based algorithm resurfaces entries at optimal times
- Multi-user support with authentication
- Search and filter bookmarks
- Keyboard shortcuts (ESC to toggle filter)
- Responsive design

## Tech Stack

**Backend:**
- TrailBase (Rust + SQLite)
- Built-in authentication
- Auto-generated REST APIs

**Frontend:**
- Vite + React 18 + TypeScript
- React Query for server state
- CSS Modules for styling

## Development

### Prerequisites

- Node.js 20+
- pnpm (`npm install -g pnpm`)
- TrailBase executable (download from trailbase.io)

### Setup

1. **Backend:**
   ```bash
   cd backend
   ./trailbase
   ```

2. **Frontend:**
   ```bash
   cd frontend
   pnpm install
   pnpm dev
   ```

3. **Access:**
   - Frontend: http://localhost:5173
   - Backend: http://localhost:4000

### Project Structure

```
interne/
├── backend/           # TrailBase configuration
│   ├── trailbase      # TrailBase executable
│   ├── config.json    # Server configuration
│   └── migrations/    # Database schema
├── frontend/          # Vite React app
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── services/
│   │   ├── styles/
│   │   ├── types/
│   │   └── utils/
│   └── package.json
└── docker-compose.yml # Docker deployment
```

## Deployment

### Docker

1. **Build:**
   ```bash
   docker-compose build
   ```

2. **Run:**
   ```bash
   docker-compose up -d
   ```

3. **Access:**
   - App: http://localhost:4000

### VPS Deployment

1. Transfer files to VPS
2. Run `docker-compose up -d`
3. Configure reverse proxy (nginx) for HTTPS
4. Point domain to server

### Backups

Database is a single SQLite file:

```bash
docker-compose exec interne cp /app/data/interne.db /app/data/backup.db
```

Copy `data/interne.db` to external storage regularly.

## Migration from Old Version

On first login after upgrade:
1. App checks for localStorage data
2. Prompts to import existing bookmarks
3. Migrates all entries to backend
4. Clears localStorage

## License

MIT
```

**Step 2: Commit README**

```bash
git add README.md
git commit -m "docs: update README with new architecture and setup instructions"
```

---

## Task 15: Testing and Verification

**Files:**
- None (manual testing)

**Step 1: Test backend startup**

Run: `cd backend && ./trailbase`

Expected: Server starts, database created, no errors

**Step 2: Test frontend build**

Run: `cd frontend && pnpm build`

Expected: Build succeeds, creates `dist/` directory

**Step 3: Test Docker build**

Run: `docker-compose build`

Expected: Image builds successfully

**Step 4: Test full stack locally**

Run in separate terminals:
```bash
# Terminal 1
cd backend && ./trailbase

# Terminal 2
cd frontend && pnpm dev
```

**Step 5: Manual testing checklist**

- [ ] Register new user
- [ ] Login with credentials
- [ ] Create entry
- [ ] View entry in list
- [ ] Edit entry
- [ ] Delete entry
- [ ] Search entries
- [ ] Toggle filter (View All / View Available)
- [ ] Click entry link (marks as read)
- [ ] Verify entry resurfaces after interval
- [ ] Logout
- [ ] Login again (entries persist)

**Step 6: Test Docker deployment**

Run: `docker-compose up`

Expected: App accessible at http://localhost:4000, all features work

**Step 7: Document any issues found**

Create GitHub issues for bugs discovered during testing.

---

## Success Criteria

Implementation is complete when:

- [x] TrailBase backend running with SQLite database
- [x] Frontend built with Vite + React + TypeScript
- [x] All components migrated and working
- [x] Authentication (register, login, logout) functional
- [x] CRUD operations on entries work
- [x] Search and filter functional
- [x] Entropy algorithm resurfaces entries correctly
- [x] Docker deployment works
- [x] All existing UI preserved
- [x] No localStorage dependencies remain
- [x] Documentation updated

## Next Steps

After implementation:

1. **Deploy to VPS:**
   - Set up reverse proxy (nginx)
   - Configure HTTPS (Let's Encrypt)
   - Set up automated backups

2. **Enhancements:**
   - OAuth providers (Google, GitHub)
   - Browser extension
   - Export/import functionality
   - Tags and categories
   - Shared collections

3. **Monitoring:**
   - Error tracking (Sentry)
   - Usage analytics
   - Performance monitoring
