# ── Stage 1: Angular UI ────────────────────────────────────────────────
# Kept isolated so the Rust build never runs npm. Angular 22 + TypeScript 6 +
# @angular/build, unit tests run with Vitest (see docs/ui-architecture.md).
FROM node:24-slim AS ui

WORKDIR /ui

COPY frontend/package.json frontend/package-lock.json frontend/angular.json frontend/tsconfig*.json ./
RUN npm ci

COPY frontend/ ./
# angular.json outputs to ../static (i.e. /static/browser) from the project root.
RUN npm run build

# ── Stage 2: Rust backend ──────────────────────────────────────────────
FROM rust:1.98-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Dependency cache: build with a stub crate first (Cargo.lock is versioned).
COPY Cargo.toml Cargo.lock build.rs ./
RUN mkdir src \
    && echo 'fn main() {}' > src/main.rs \
    && echo '' > src/lib.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
RUN cargo build --release

# ── Stage 3: runtime ───────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    chromium \
    && rm -rf /var/lib/apt/lists/*

RUN adduser --disabled-password --gecos "" shinobi

WORKDIR /app
COPY --from=builder /app/target/release/shinobi .

# Bundled API + UI (built in the `ui` stage). If `static/` is absent the
# server still starts and serves the JSON API; only `/` 404s.
COPY --from=ui /static ./static

RUN mkdir -p /data/downloads && chown -R shinobi:shinobi /data /app

USER shinobi

ENV PORT=8060
ENV SHINOBI_DB_PATH=/data/shinobi.db
ENV DATA_DIR=/data/downloads

EXPOSE 8060

CMD ["./shinobi"]
