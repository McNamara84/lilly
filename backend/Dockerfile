FROM rust:1.97.1-trixie AS builder

WORKDIR /app

# Install required system dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy workspace manifest and member manifests for dependency caching
COPY Cargo.toml Cargo.lock ./
COPY backend/Cargo.toml backend/Cargo.toml
COPY importer-core/Cargo.toml importer-core/Cargo.toml
COPY importer-adapters/Cargo.toml importer-adapters/Cargo.toml
COPY importer/Cargo.toml importer/Cargo.toml

# Create dummy sources to build dependencies
RUN mkdir -p backend/src importer-core/src importer-adapters/src importer/src && \
    echo "fn main() {}" > backend/src/main.rs && \
    echo "" > importer-core/src/lib.rs && \
    echo "" > importer-adapters/src/lib.rs && \
    echo "fn main() {}" > importer/src/main.rs
RUN cargo build --release -p lilly-backend 2>/dev/null || true

# Copy actual source code
COPY backend/src/ backend/src/
COPY backend/migrations/ backend/migrations/
COPY backend/.sqlx/ backend/.sqlx/
COPY importer-core/src/ importer-core/src/
COPY importer-adapters/src/ importer-adapters/src/
COPY importer-adapters/tests/ importer-adapters/tests/
COPY importer/src/ importer/src/

# Touch main.rs so cargo rebuilds it
RUN touch backend/src/main.rs importer-core/src/lib.rs importer-adapters/src/lib.rs

# Build with SQLX_OFFLINE since we don't have a DB at build time
ENV SQLX_OFFLINE=true
RUN cargo build --release -p lilly-backend

FROM debian:trixie-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

# Keep ownership stable for the persistent media volume and the host-mounted,
# mode-0600 account-erasure ledger across image upgrades.
RUN groupadd --system --gid 999 appuser && \
    useradd --system --uid 999 --gid appuser --no-create-home appuser

# Create persistent mount points writable by appuser.
RUN mkdir -p /media /erasure-ledger && chown appuser:appuser /media /erasure-ledger

WORKDIR /app
COPY --from=builder --chown=appuser:appuser /app/target/release/lilly-backend ./
COPY --from=builder --chown=appuser:appuser /app/backend/migrations ./migrations

ENV RUST_LOG=info
EXPOSE 8080

FROM runtime AS development

# A new named development volume is populated from this image directory.
RUN touch /erasure-ledger/account-erasure.log && \
    chown appuser:appuser /erasure-ledger/account-erasure.log && \
    chmod 0600 /erasure-ledger/account-erasure.log

USER appuser

CMD ["./lilly-backend"]

FROM runtime AS production

# Production intentionally contains no ledger file. The host provisioning step
# must provide it through the bind mount or startup fails closed.
USER appuser

CMD ["./lilly-backend"]
