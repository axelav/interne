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
- TrailBase (`curl -sSL https://trailbase.io/install.sh | bash`)

### Setup

1. **Backend:**
   ```bash
   cd backend
   trail run --dev
   ```

   Note: The `--dev` flag enables permissive CORS for local development with the Vite dev server.

2. **Frontend:**
   ```bash
   cd frontend
   pnpm install
   pnpm dev
   ```

3. **Access:**
   - Frontend (dev): http://localhost:5173
   - Backend API: http://localhost:4000
   - TrailBase Admin: http://localhost:4000/_/admin/

### Project Structure

```
interne/
├── backend/
│   └── migrations/    # Database schema migrations
├── frontend/          # Vite React app
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── services/
│   │   ├── styles/
│   │   ├── types/
│   │   └── utils/
│   └── package.json
├── traildepot/        # TrailBase runtime data (created on first run)
├── Dockerfile
└── docker-compose.yml
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

The TrailBase data directory contains the SQLite database and configuration:

```bash
# Backup the entire traildepot directory
tar -czf backup-$(date +%Y%m%d).tar.gz traildepot/
```

Or copy just the database:
```bash
docker-compose exec interne cp /app/traildepot/main.db /app/traildepot/backup.db
```

## License

MIT
