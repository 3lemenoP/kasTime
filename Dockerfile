# =============================================================================
# KTCS Calendar Server - Multi-stage Docker Build
# =============================================================================
# Build: docker build -t ktcs-calendar .
# Run:   docker run -p 3001:3001 --env-file .env.production ktcs-calendar

# -----------------------------------------------------------------------------
# Stage 1: Build the Rust binary
# -----------------------------------------------------------------------------
FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    perl \
    make \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy workspace manifests first for dependency caching.
# Cargo.lock is now committed/tracked and NOT excluded by .dockerignore, so the
# build consumes the pinned lockfile (see --locked below) for reproducibility.
COPY Cargo.toml Cargo.lock ./
COPY ktcs-core/Cargo.toml ktcs-core/Cargo.toml
COPY ktcs-cli/Cargo.toml ktcs-cli/Cargo.toml
COPY ktcs-calendar/Cargo.toml ktcs-calendar/Cargo.toml
COPY ktcs-wasm/Cargo.toml ktcs-wasm/Cargo.toml

# Create dummy source files to build dependencies only
RUN mkdir -p ktcs-core/src ktcs-cli/src ktcs-calendar/src ktcs-wasm/src && \
    echo "pub const VERSION: &str = \"0.1.0\";" > ktcs-core/src/lib.rs && \
    echo "fn main() {}" > ktcs-cli/src/main.rs && \
    echo "fn main() {}" > ktcs-calendar/src/main.rs && \
    echo "" > ktcs-wasm/src/lib.rs

# Build dependencies (this layer is cached unless Cargo.toml/lock changes)
RUN cargo build --release --locked -p ktcs-calendar 2>/dev/null || true

# Copy actual source code
COPY ktcs-core/src ktcs-core/src
COPY ktcs-calendar/src ktcs-calendar/src

# Touch main files to invalidate the dummy builds
RUN touch ktcs-core/src/lib.rs ktcs-calendar/src/main.rs

# Build the actual binary (--locked enforces the committed Cargo.lock)
RUN cargo build --release --locked -p ktcs-calendar

# -----------------------------------------------------------------------------
# Stage 2: Runtime image
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN groupadd -r ktcs && useradd -r -g ktcs -s /bin/false ktcs

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/ktcs-calendar .

# Create data directory for SQLite
RUN mkdir -p data && chown -R ktcs:ktcs /app

USER ktcs

EXPOSE 3001

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:3001/health || exit 1

CMD ["./ktcs-calendar"]
