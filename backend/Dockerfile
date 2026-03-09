FROM rust:1-slim AS builder

WORKDIR /app

# Install required system dependencies
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

# Copy manifests first for dependency caching
COPY Cargo.toml Cargo.lock* ./

# Create a dummy main.rs to build dependencies
RUN mkdir src && echo "fn main() {}" > src/main.rs
RUN cargo build --release 2>/dev/null || true

# Copy actual source code
COPY src/ src/
COPY migrations/ migrations/

# Touch main.rs so cargo rebuilds it
RUN touch src/main.rs

# Build with SQLX_OFFLINE since we don't have a DB at build time
ENV SQLX_OFFLINE=true
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*

RUN groupadd --system appuser && useradd --system --gid appuser --no-create-home appuser

WORKDIR /app
COPY --from=builder --chown=appuser:appuser /app/target/release/lilly-backend ./
COPY --from=builder --chown=appuser:appuser /app/migrations ./migrations

USER appuser

ENV RUST_LOG=info
EXPOSE 8080

CMD ["./lilly-backend"]
