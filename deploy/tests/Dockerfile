FROM python:3.14-slim-trixie@sha256:9aaa474097d094040649f2162a627c0049b408f52138d980916788d80f04802f

# Linux host tools for local Windows/macOS acceptance and coverage runs.
RUN apt-get update && apt-get install -y --no-install-recommends \
    docker.io docker-cli docker-compose curl systemd ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN pip install --no-cache-dir coverage==7.14.1
WORKDIR /workspace
