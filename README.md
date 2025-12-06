# Interne

A spaced-repetition bookmark manager that resurfaces saved websites after configurable intervals.

## Features

- Save bookmarks with title, description, and custom revisit intervals
- Entropy-based algorithm resurfaces entries at optimal times
- Multi-user support with authentication
- Search and filter bookmarks
- Keyboard shortcuts (ESC to toggle filter, / for search)
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

## License

MIT
