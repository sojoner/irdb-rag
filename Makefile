.PHONY: help build test test-coverage db-backup db-restore dev-up dev-down dev-logs dev-build

# Docker configuration
IMAGE_NAME ?= rag-chat
IMAGE_TAG ?= latest
BUILD_JOBS ?= 10

# Database container
DB_CONTAINER ?= rag-db
BACKUP_DIR ?= ./data/backups

help:
	@echo "Available targets:"
	@echo ""
	@echo "  Development:"
	@echo "    dev-up           - Start dev environment with docker-compose.dev.yml (cargo leptos watch)"
	@echo "    dev-down         - Stop dev environment"
	@echo "    dev-logs         - Tail dev container logs with full WASM output"
	@echo "    dev-build        - Rebuild dev container image"
	@echo ""
	@echo "  Build & Test:"
	@echo "    build             - Build Docker image with :latest tag"
	@echo "    test              - Run unit tests (fast, no DB required)"
	@echo "    test-integration  - Run integration tests (requires DB on bm3090:15432)"
	@echo "    test-coverage     - Run tests with coverage check >= 60% (requires cargo-tarpaulin)"
	@echo ""
	@echo "  Database:"
	@echo "    db-backup         - Backup database to $(BACKUP_DIR)"
	@echo "    db-restore        - Restore database (requires BACKUP_FILE=...)"
	@echo ""

# =============================================================================
# Build
# =============================================================================

build:
	@echo "Building Docker image: $(IMAGE_NAME):$(IMAGE_TAG)"
	DOCKER_BUILDKIT=1 docker build \
		--build-arg BUILD_JOBS=$(BUILD_JOBS) \
		-t $(IMAGE_NAME):$(IMAGE_TAG) \
		.
	@echo "Build complete: $(IMAGE_NAME):$(IMAGE_TAG)"

# =============================================================================
# Test
# =============================================================================

test:
	@echo "Running unit tests..."
	@cargo test --lib --all-features

test-integration:
	@echo "Running integration tests (requires database on bm3090:15432)..."
	@cargo test --test '*' --all-features -- --test-threads=1

test-coverage:
	@echo "Running tests with coverage (requires: cargo install cargo-tarpaulin)..."
	@cargo tarpaulin --out Html --out Stdout --skip-clean --timeout 300 \
		--ignore-tests --fail-under 60 || \
		(echo "Coverage below 60% threshold!" && exit 1)
	@echo "Coverage report: tarpaulin-report.html"

# =============================================================================
# Database Backup & Restore
# =============================================================================

db-backup:
	@echo "Backing up database to $(BACKUP_DIR)..."
	@mkdir -p $(BACKUP_DIR)
	@BACKUP_FILE="$(BACKUP_DIR)/rag_chat_$$(date +%Y%m%d_%H%M%S).dump" && \
		docker exec $(DB_CONTAINER) pg_dump -U rag_user -d rag_chat -F d -j $(BUILD_JOBS) -f /tmp/temp_backup && \
		docker cp $(DB_CONTAINER):/tmp/temp_backup $$BACKUP_FILE && \
		docker exec $(DB_CONTAINER) rm -rf /tmp/temp_backup && \
		echo "Backup created: $$BACKUP_FILE" && \
		ls -lh -d $$BACKUP_FILE

db-restore:
	@if [ -z "$(BACKUP_FILE)" ]; then \
		echo "Error: BACKUP_FILE is not set"; \
		echo "Usage: make db-restore BACKUP_FILE=./data/backups/rag_chat_YYYYMMDD_HHMMSS.dump"; \
		exit 1; \
	fi
	@echo "Restoring database from $(BACKUP_FILE)..."
	@echo "Terminating active connections..."
	@docker exec $(DB_CONTAINER) psql -U rag_user -d postgres -v ON_ERROR_STOP=off -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='rag_chat' AND pid != pg_backend_pid();" 2>/dev/null || true
	@echo "Dropping database..."
	@docker exec $(DB_CONTAINER) psql -U rag_user -d postgres -v ON_ERROR_STOP=off -c "DROP DATABASE IF EXISTS rag_chat;" 2>/dev/null || true
	@echo "Creating database..."
	@docker exec $(DB_CONTAINER) psql -U rag_user -d postgres -v ON_ERROR_STOP=off -c "CREATE DATABASE rag_chat;" 2>/dev/null || true
	@echo "Restoring data from backup..."
	@docker cp $(BACKUP_FILE) $(DB_CONTAINER):/tmp/restore.dump
	@docker exec $(DB_CONTAINER) pg_restore -U rag_user -d rag_chat --no-acl --no-owner /tmp/restore.dump 2>/dev/null || true
	@docker exec $(DB_CONTAINER) rm /tmp/restore.dump 2>/dev/null || true
	@echo "Verifying restore..."
	@docker exec $(DB_CONTAINER) psql -U rag_user -d rag_chat -c "SELECT COUNT(*) as documents FROM documents; SELECT COUNT(*) as chunks FROM document_chunks;"
	@echo "Database restore complete!"

# =============================================================================
# Development with cargo-leptos watch
# =============================================================================

dev-up:
	@echo "🚀 Starting dev environment (cargo leptos watch)..."
	@echo "   📦 Database: postgres://localhost:15432/rag_chat"
	@echo "   🌐 App: http://localhost:3000"
	@echo "   ⚡ HMR: http://localhost:3001"
	@echo ""
	docker compose -f docker-compose.dev.yml up --build

dev-down:
	@echo "Stopping dev environment..."
	docker compose -f docker-compose.dev.yml down --remove-orphans

dev-build:
	@echo "Building dev container..."
	docker compose -f docker-compose.dev.yml build --no-cache
