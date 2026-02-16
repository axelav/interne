FROM rust:1.83 AS builder

WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Create dummy main.rs to cache dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release
RUN rm -rf src

# Copy actual source
COPY src ./src
COPY templates ./templates
COPY migrations ./migrations

# Build for release
RUN touch src/main.rs && cargo build --release

# Runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/interne /usr/local/bin/interne
COPY --from=builder /app/templates /app/templates
COPY --from=builder /app/migrations /app/migrations
COPY static /app/static

WORKDIR /app

EXPOSE 3000

CMD ["interne"]
