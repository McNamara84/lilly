FROM rust:1-slim AS builder

WORKDIR /app

# Install required system dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy workspace manifest and member manifests for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
COPY importer-core/Cargo.toml importer-core/Cargo.toml
COPY importer/Cargo.toml importer/Cargo.toml

# Create dummy sources to build dependencies
RUN mkdir -p backend/src importer-core/src importer/src && \
    echo "fn main() {}" > backend/src/main.rs && \
    echo "" > importer-core/src/lib.rs && \
    echo "fn main() {}" > importer/src/main.rs
RUN cargo build --release -p lilly-backend 2>/dev/null || true

# Copy actual source code
COPY backend/src/ backend/src/
COPY backend/migrations/ backend/migrations/
COPY backend/.sqlx/ backend/.sqlx/
COPY importer-core/src/ importer-core/src/

# Touch main.rs so cargo rebuilds it
RUN touch backend/src/main.rs importer-core/src/lib.rs

# Build with SQLX_OFFLINE since we don't have a DB at build time
ENV SQLX_OFFLINE=true
RUN cargo build --release -p lilly-backend

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

RUN groupadd --system appuser && useradd --system --gid appuser --no-create-home appuser

WORKDIR /app
COPY --from=builder --chown=appuser:appuser /app/target/release/lilly-backend ./
COPY --from=builder --chown=appuser:appuser /app/backend/migrations ./migrations

USER appuser

ENV RUST_LOG=info
EXPOSE 8080

CMD ["./lilly-backend"]
