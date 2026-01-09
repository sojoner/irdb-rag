# ============================================
# Build Stage - Idiomatic Rust Build with Caching
# ============================================
FROM rust:1-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    curl \
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

# Install wasm32 target (cached layer)
RUN rustup target add wasm32-unknown-unknown

# Install cargo-leptos with reduced parallelism to avoid OOM
# Using -j 2 to balance build speed and memory usage
RUN cargo install cargo-leptos --version 0.3.2 --jobs 2

# Set working directory
WORKDIR /app

# === Cache Node.js dependencies ===
# Copy package files first for npm layer caching
COPY package.json package-lock.json ./
COPY tailwind.config.js postcss.config.js ./
RUN npm ci

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
RUN cargo update --workspace

# Build server dependencies (SSR feature)
RUN cargo build --release --bin rag-chat --features ssr

# Build WASM dependencies (hydrate feature) - only the lib target
RUN cargo build --release --lib --target wasm32-unknown-unknown --no-default-features --features hydrate

# Clean up dummy source files
RUN rm -rf src/*.rs

# === Build actual application ===
# Copy source code and configuration (this invalidates cache when code changes)
COPY src ./src
COPY static ./static
COPY config ./config

# Build Tailwind CSS
RUN npm run build:css

# Build the Leptos application in release mode
# Dependencies are already compiled, so this is fast
RUN cargo leptos build --release

# ============================================
# Runtime Stage - Minimal Cloud-Ready Image
# ============================================
FROM debian:bookworm-slim

# Install runtime dependencies (minimal)
RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

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
