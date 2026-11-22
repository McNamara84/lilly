FROM docker/compose-bin:v5.1.3@sha256:6d7c88928df9682427c7d5d0107be041e499c0f5a46760b2330274d1173faa48 AS compose
FROM python:3.14-slim-trixie@sha256:9aaa474097d094040649f2162a627c0049b408f52138d980916788d80f04802f

# Linux host tools for local Windows/macOS acceptance and coverage runs.
RUN apt-get update && apt-get install -y --no-install-recommends \
    docker.io docker-cli curl systemd ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=compose /docker-compose /usr/libexec/docker/cli-plugins/docker-compose
RUN pip install --no-cache-dir coverage==7.14.1
WORKDIR /workspace
