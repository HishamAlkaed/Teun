# ---- Frontend build ----
FROM node:22-bookworm-slim AS frontend

WORKDIR /app

COPY apps/teun/web/package.json apps/teun/web/package-lock.json ./
RUN npm ci

COPY apps/teun/web/ ./
RUN npm run build

# ---- Rust build ----
FROM rust:1.92-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    protobuf-compiler libprotobuf-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace files for dependency resolution
COPY Cargo.toml Cargo.lock ./
COPY apps/teun/service/Cargo.toml apps/teun/service/Cargo.toml

# Create dummy source file for dependency caching
RUN mkdir -p apps/teun/service/src && \
    echo "fn main() {}" > apps/teun/service/src/main.rs

ARG CARGO_BUILD_JOBS=2

RUN cargo fetch
RUN cargo build --release --package teun -j${CARGO_BUILD_JOBS} 2>/dev/null || true

# Copy actual source files
COPY apps/teun/service/ apps/teun/service/

RUN touch apps/teun/service/src/main.rs
RUN cargo build --release --package teun -j${CARGO_BUILD_JOBS}

# ---- Runtime ----
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libstdc++6 \
    libgcc-s1 \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js (required for claude CLI)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - && \
    apt-get install -y nodejs && \
    rm -rf /var/lib/apt/lists/*

# Install claude CLI globally
RUN npm install -g @anthropic-ai/claude-code

WORKDIR /app

# PDFium native library for pdfium-render (dynamic binding at /app/libpdfium.so).
# Pinned to bblanchon/pdfium-binaries chromium/7881 — the exact build that
# pdfium-render 0.9.3's `pdfium_latest` feature (pdfium_7881) targets.
# Checksum from the GitHub release asset digest (verified 2026-07-24).
ARG PDFIUM_RELEASE=chromium/7881
ARG PDFIUM_SHA256=1470e21b8b4a3b4ad7f85684e2da11d94f3b69a86d81dee11b9b6709d927ac1d
RUN curl -fsSL -o /tmp/pdfium.tgz \
        "https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_RELEASE}/pdfium-linux-x64.tgz" && \
    echo "${PDFIUM_SHA256}  /tmp/pdfium.tgz" | sha256sum -c - && \
    tar -xzf /tmp/pdfium.tgz -C /tmp lib/libpdfium.so && \
    mv /tmp/lib/libpdfium.so /app/libpdfium.so && \
    rm -rf /tmp/pdfium.tgz /tmp/lib

COPY --from=builder /app/target/release/teun /app/teun
COPY --from=frontend /app/dist /app/dist
COPY apps/teun/service/config/ /app/config/
COPY resources/ /app/resources/

RUN useradd -m -s /bin/bash app
USER app

# Credentials mounted at runtime via docker-compose
# ~/.claude/.credentials.json

ENV PORT=3000
ENV STATIC_DIR=/app/dist
EXPOSE 3000

CMD ["/app/teun"]
