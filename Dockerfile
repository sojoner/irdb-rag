# ============================================
# Build Stage - Idiomatic Rust Build with Caching
# ============================================
# syntax=docker/dockerfile:1
FROM rust:1-slim-bookworm AS builder

# Install build dependencies with cache mounts
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    curl \
    nodejs \
    npm \
    ca-certificates \
    lld \
    clang \
    make \
    perl

# Build parallelism - defaults to all available cores
ARG BUILD_JOBS=0

# Optimize cargo settings for Docker builds
# - Use sparse registry protocol for faster dependency downloads
# - Disable incremental compilation for Docker builds (better caching)
# - Limit codegen units to reduce memory usage
# - Use lld linker for faster linking
ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse \
    CARGO_INCREMENTAL=0 \
    CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16 \
    CARGO_NET_GIT_FETCH_WITH_CLI=true

# Install wasm32 target (cached layer)
RUN rustup target add wasm32-unknown-unknown

# Install cargo-leptos with parallel compilation and cargo cache
# Build from source for better compatibility and to leverage cargo caching
ARG CARGO_LEPTOS_VERSION=0.3.2
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    JOBS=$(if [ "${BUILD_JOBS}" -eq 0 ]; then nproc; else echo "${BUILD_JOBS}"; fi) && \
    cargo install cargo-leptos --version ${CARGO_LEPTOS_VERSION} --locked --jobs ${JOBS}

# Set working directory
WORKDIR /app

# === Cache Node.js dependencies ===
# Copy package files first for npm layer caching
COPY package.json package-lock.json ./
COPY tailwind.config.js postcss.config.js ./
# Use npm cache mount for faster installs
RUN --mount=type=cache,target=/root/.npm,sharing=locked \
    npm ci

# === Cache Rust dependencies ===
# Copy manifests first for cargo layer caching
COPY Cargo.toml Cargo.lock ./
COPY Leptos.toml ./

# Create dummy source files to build dependencies
# This project has both lib and bin, so we need both
RUN mkdir -p src && \
    echo "pub fn dummy() {}" > src/lib.rs && \
    echo "fn main() {}" > src/main.rs

# Build dependencies only (this layer will be cached)
# Update Cargo.lock for Linux platform if it was built on macOS
# Use cache mounts for faster dependency resolution
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    cargo update --workspace

# Build dependencies WITHOUT LTO for much faster builds
# LTO will only be applied during the final build with actual code
# This saves 50-80% of build time for dependency compilation
# Use BuildKit cache mounts for cargo registry, git deps, and build artifacts
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    CARGO_PROFILE_RELEASE_LTO=false \
    cargo build --release --bin rag-chat --features ssr -j $(if [ "$BUILD_JOBS" -eq 0 ]; then nproc; else echo "$BUILD_JOBS"; fi)

RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    CARGO_PROFILE_RELEASE_LTO=false \
    cargo build --release --lib --target wasm32-unknown-unknown --no-default-features --features hydrate -j $(if [ "$BUILD_JOBS" -eq 0 ]; then nproc; else echo "$BUILD_JOBS"; fi)

# Clean up dummy source files
RUN rm -rf src/*.rs

# === Build actual application ===
# Copy source code and configuration (this invalidates cache when code changes)
COPY src ./src
COPY static ./static
COPY config ./config

# Build Tailwind CSS
RUN npm run build:css

# Build the Leptos application in release mode (optimized for FAST builds - no LTO)
# For production builds with full optimization, use: --profile production
# Dependencies are already compiled, so this is fast
# Use cache mounts for cargo registry, git deps, and build artifacts
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/app/target,sharing=locked \
    cargo leptos build --release && \
    # Copy artifacts out of cache mount before it's unmounted
    # We use cp -a to preserve directory structure and avoid issues with existing directories
    mkdir -p /tmp/release /tmp/site && \
    cp -a target/release/rag-chat /tmp/release/rag-chat && \
    cp -a target/site/. /tmp/site/

# Move artifacts back to target directory
# We ensure the parent directories exist and target is clean
RUN rm -rf target/release target/site && \
    mkdir -p target/release target/site && \
    mv /tmp/release/rag-chat target/release/rag-chat && \
    mv /tmp/site/* target/site/

# ============================================
# Runtime Stage - Minimal Cloud-Ready Image
# ============================================
FROM debian:bookworm-slim

# Install runtime dependencies (minimal) with cache mounts
RUN --mount=type=cache,target=/var/cache/apt,sharing=locked \
    --mount=type=cache,target=/var/lib/apt,sharing=locked \
    apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates

# Create non-root user for security
RUN useradd -m -u 1000 -s /bin/bash appuser

# Set working directory
WORKDIR /app

# Copy the compiled binary from builder
COPY --from=builder /app/target/release/rag-chat /app/rag-chat

# Copy the compiled site assets (WASM, CSS, static files)
COPY --from=builder /app/target/site /app/site

# Copy configuration files
COPY --from=builder /app/config /app/config

# Change ownership to non-root user
RUN chown -R appuser:appuser /app

# Switch to non-root user
USER appuser

# Expose the application port
EXPOSE 3000

# Set environment variables
ENV LEPTOS_SITE_ROOT=/app/site
ENV LEPTOS_SITE_ADDR=0.0.0.0:3000
ENV RUST_LOG=info

# Run the application
CMD ["/app/rag-chat"]
