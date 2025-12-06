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
