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

# Runtime - Use official TrailBase image
FROM trailbase/trailbase:latest

WORKDIR /app

# Copy frontend build to be served as static files
COPY --from=frontend-builder /app/frontend/dist /app/public

# Copy migrations
COPY backend/migrations /app/traildepot/migrations

# Create traildepot directory for data persistence
RUN mkdir -p /app/traildepot

EXPOSE 4000

# Run TrailBase with public directory for frontend and custom data directory
CMD ["/app/trail", "run", "--address", "0.0.0.0:4000", "--public-dir", "/app/public", "--data-dir", "/app/traildepot"]
