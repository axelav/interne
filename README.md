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

- PocketBase (Go + SQLite)
- Built-in authentication
- Auto-generated REST APIs
- Admin UI for database management

**Frontend:**

- Vite + React 18 + TypeScript
- React Query for server state
- PocketBase JavaScript SDK
- CSS Modules for styling

## Development

### Prerequisites

- Node.js 20+
- pnpm (`npm install -g pnpm`)
- Docker & Docker Compose

### Setup

1. **Start PocketBase:**

   ```bash
   docker compose up -d
   ```

2. **Configure PocketBase:**
   - Open <http://localhost:8090/\_/>
   - Create admin account
   - Follow setup instructions in `scripts/setup-pocketbase.md`

3. **Install frontend dependencies:**

   ```bash
   cd frontend
   pnpm install
   ```

4. **Start development server:**

   ```bash
   pnpm dev
   ```

5. **Access:**
   - Frontend: <http://localhost:5173>
   - PocketBase Admin: <http://localhost:8090/\_/>
   - PocketBase API: <http://localhost:8090/api/>

### Project Structure

```
interne/
├── docker-compose.yml    # PocketBase container config
├── frontend/             # Vite React app
│   ├── src/
│   │   ├── components/
│   │   ├── hooks/
│   │   ├── services/
│   │   ├── styles/
│   │   ├── types/
│   │   └── utils/
│   └── package.json
├── pb_data/              # PocketBase data (gitignored)
└── scripts/
    └── setup-pocketbase.md
```

## Deployment

### Docker

1. **Update PocketBase URL** in production:

   ```bash
   # Set in frontend/.env.production
   VITE_POCKETBASE_URL=https://your-domain.com
   ```

2. **Build frontend:**

   ```bash
   cd frontend
   pnpm build
   ```

3. **Run with Docker Compose:**

   ```bash
   docker compose up -d
   ```

4. **Serve frontend** with your preferred static file server (nginx, Caddy, etc.)

### VPS Deployment

1. Transfer files to VPS
2. Configure reverse proxy for both PocketBase API and frontend
3. Point domain to server
4. Configure HTTPS with Let's Encrypt

### Backups

PocketBase stores all data in the `pb_data` directory:

```bash
# Backup the entire pb_data directory
tar -czf backup-$(date +%Y%m%d).tar.gz pb_data/

# Or just the database
docker compose exec pocketbase cp /pb_data/data.db /pb_data/backup.db
```

## License

MIT
