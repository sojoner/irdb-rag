.PHONY: help lint fmt fmt-check check ci \
        test test-all test-unit test-integration test-db-reset test-db-init \
        gpu-up gpu-down gpu-restart gpu-build gpu-logs gpu-shell gpu-test gpu-lint gpu-ci \
        gpu-verify-db gpu-db-stats \
        docker-build docker-push docker-release \
        docling-test reranker-test clean db-backup db-restore db-reset db-migrate

# Test configuration - use 768 for nomic-embed-text-v2-moe (faster embeddings)
EMBEDDING_DIMENSIONS ?= 768
RUN_ENV ?= test
COMPOSE_FILE ?= docker-compose.yml

# Docker configuration
IMAGE_NAME ?= rag-chat
IMAGE_TAG ?= latest
REGISTRY ?= docker.io
IMAGE_FULL := $(REGISTRY)/$(IMAGE_NAME):$(IMAGE_TAG)
BUILD_JOBS ?= 0

help:
	@echo "Available targets:"
	@echo ""
	@echo "Code Quality (run before commit):"
	@echo "  lint              - Run clippy with warnings as errors"
	@echo "  fmt               - Format code with rustfmt"
	@echo "  check             - Verify code compiles (fast)"
	@echo "  ci                - Full CI check: fmt, lint, check, test-unit"
	@echo ""
	@echo "Testing:"
	@echo "  test              - Run core tests (unit + API, no external services)"
	@echo "  test-all          - Run ALL tests including Docling/vLLM (requires services running)"
	@echo "  test-unit         - Run only unit tests (fast, no DB required)"
	@echo "  test-integration  - Run integration tests (requires DB reset)"
	@echo "  test-db-reset     - Reset test database schema (DOWN and UP)"
	@echo "  test-db-init      - Initialize test database with correct embedding dimensions"
	@echo ""
	@echo "GPU Development:"
	@echo "  gpu-up            - Start GPU dev environment (clean start with DB init)"
	@echo "  gpu-down          - Stop GPU dev environment"
	@echo "  gpu-restart       - Restart the dev container (useful after config changes)"
	@echo "  gpu-build         - Build dev container with parallel compilation"
	@echo "  gpu-logs          - View logs from all GPU services"
	@echo "  gpu-shell         - Shell into dev container"
	@echo "  gpu-test          - Run tests in GPU environment (optional: TEST_FILTER=test_name TEST_FLAGS=--nocapture)"
	@echo "  gpu-lint          - Run lint checks in GPU container"
	@echo "  gpu-ci            - Run full CI in GPU container"
	@echo "  gpu-verify-db     - Verify database schema initialization"
	@echo "  gpu-db-stats      - Show database statistics (documents, chunks, jobs)"
	@echo ""
	@echo "Integration Testing:"
	@echo "  docling-test      - Test Docling document processing (requires localhost:5001)"
	@echo "  docling-import-test - Test PDF import via GPU container (requires: make gpu-up)"
	@echo "  docling-health-test - Test Docling service health via GPU container (requires: make gpu-up)"
	@echo "  docling-format-test - Test Docling file format support via GPU container (requires: make gpu-up)"
	@echo ""
	@echo "Docker:"
	@echo "  docker-build      - Build Docker image with BuildKit (fast: no LTO)"
	@echo "  docker-push       - Push Docker image to registry"
	@echo "  docker-release    - Build, tag, and push Docker image (all-in-one)"
	@echo ""
	@echo "Maintenance & Backup:"
	@echo "  db-refresh-indexes - Refresh BM25 and HNSW indexes for optimal search performance"
	@echo "  db-backup         - Backup database to /data/backups"
	@echo "  db-reset          - Reset database (drop and recreate empty)"
	@echo "  db-migrate        - Apply schema migrations from init.sql to current database"
	@echo "  db-restore        - Restore from backup with schema migration (requires BACKUP_FILE=...)"
	@echo "  clean             - Clean all test artifacts (DB data, target/)"
	@echo ""
	@echo "Environment variables:"
	@echo "  EMBEDDING_DIMENSIONS - Embedding vector dimensions (default: 1024)"
	@echo "  IMAGE_NAME           - Docker image name (default: rag-chat)"
	@echo "  IMAGE_TAG            - Docker image tag (default: latest)"
	@echo "  REGISTRY             - Docker registry (default: docker.io)"
	@echo "  BUILD_JOBS           - Parallel build jobs, 0=all cores (default: 0)"

# =============================================================================
# Code Quality Targets
# =============================================================================

# Run clippy with all warnings as errors (zero warnings policy)
lint:
	@echo "Running clippy with strict warnings..."
	cargo clippy --all-targets --all-features -- -D warnings
	@echo "✅ Lint passed!"

# Format code with rustfmt
fmt:
	@echo "Formatting code..."
	cargo fmt
	@echo "✅ Code formatted!"

# Check formatting without modifying files
fmt-check:
	@echo "Checking code formatting..."
	cargo fmt --check
	@echo "✅ Format check passed!"

# Fast compile check (no codegen)
check:
	@echo "Checking code compiles..."
	cargo check --all-targets --all-features
	@echo "✅ Check passed!"

# Full CI pipeline: format, lint, check, unit tests
ci: fmt-check lint check test-unit
	@echo "✅ CI pipeline passed!"

# =============================================================================
# Database Targets
# =============================================================================



# Clean all test artifacts
clean:
	@echo "Cleaning test artifacts..."
	docker compose down -v
	rm -rf data/postgres
	rm -rf target
	@echo "Clean complete!"

# Build Docker image with BuildKit and persistent caching
docker-build:
	@echo "Building Docker image: $(IMAGE_FULL)"
	@echo "Using BuildKit with optimized caching..."
	@echo "Build parallelism: BUILD_JOBS=$(BUILD_JOBS) (0=all cores)"
	DOCKER_BUILDKIT=1 docker build \
		--build-arg BUILD_JOBS=$(BUILD_JOBS) \
		-t $(IMAGE_NAME):$(IMAGE_TAG) \
		-t $(IMAGE_FULL) \
		.
	@echo "✅ Build complete: $(IMAGE_FULL)"

# Push Docker image to registry
docker-push:
	@echo "Pushing Docker image: $(IMAGE_FULL)"
	docker push $(IMAGE_FULL)
	@echo "✅ Push complete: $(IMAGE_FULL)"

# Build, tag, and push Docker image (all-in-one)
docker-release: docker-build docker-push
	@echo "✅ Docker release complete: $(IMAGE_FULL)"

# GPU Development targets
# Start GPU development environment
gpu-up:
	@echo "Starting GPU development environment (CLEAN START)..."
	@echo ""
	@echo "Step 1: Cleaning up old containers and data..."
	RUN_ENV=test-gpu docker compose -f docker-compose-gpu.yml down -v 2>/dev/null || true
	#@sudo rm -rf /data/postgres
	#@echo "✅ Cleaned old data"
	#@echo ""
	@echo "Step 2: Preparing data directories..."
	@sudo mkdir -p /data/docling_models /data/docling_scratch /data/ollama /data/postgres /data/backups
	@sudo chown 999:999 /data/postgres
	@sudo chmod 700 /data/postgres
	@sudo chmod 777 /data/backups
	@echo "✅ Directories ready"
	@echo ""
	@echo "Step 3: Starting database service only..."
	RUN_ENV=test-gpu EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS) docker compose -f docker-compose-gpu.yml up -d db db-perms
	@echo "Waiting for database to be healthy..."
	@for i in 1 2 3 4 5 6 7 8 9 10 11 12; do \
		if docker exec rag-db pg_isready -U rag_user -d rag_chat > /dev/null 2>&1; then \
			echo "✓ Database is ready"; \
			break; \
		fi; \
		if [ $$i -eq 12 ]; then \
			echo "❌ Database failed to start after 60 seconds"; \
			exit 1; \
		fi; \
		sleep 5; \
		echo "  Waiting... ($$(($$i * 5))s)"; \
	done
	@echo "✅ Database is healthy"
	@echo ""
	@echo "Step 4: Running database schema initialization..."
	RUN_ENV=test-gpu EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS) docker compose -f docker-compose-gpu.yml run --rm db-init
	@echo "✅ Database schema initialized"
	@echo ""
	@echo "Step 5: Verifying database schema..."
	@docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT COUNT(*) as table_count FROM information_schema.tables WHERE table_schema='public'" 2>/dev/null || true
	@echo "✅ Schema verification complete"
	@echo ""
	@echo "Step 6: Starting remaining services (Ollama, Docling, Web)..."
	RUN_ENV=test-gpu EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS) docker compose -f docker-compose-gpu.yml up -d ollama docling ollama-init docling-init web
	@echo "⏳ Services starting (this may take a few minutes)..."
	@echo ""
	@echo "✅ GPU development environment started!"
	@echo ""
	@echo "Services:"
	@echo "  - App:     http://localhost:3000 (hot-reload enabled)"
	@echo "  - Docling: http://localhost:5001"
	@echo "  - Ollama:  http://localhost:11434"
	@echo "  - DB:      localhost:15432"
	@echo ""
	@echo "Monitor progress with: make gpu-logs"
	@echo "Shell into dev container: make gpu-shell"

.PHONY: db-backup gpu-down
# Stop GPU development environment
gpu-down: db-backup
	docker compose -f docker-compose-gpu.yml down

# Restart dev container (useful after config changes)
gpu-restart:
	@echo "Restarting dev container..."
	docker compose -f docker-compose-gpu.yml restart dev
	@echo "Dev container restarted! App will reload automatically."

# Build dev container with parallel compilation
gpu-build:
	@echo "Building dev container with 16 parallel jobs..."
	DOCKER_BUILDKIT=1 docker compose -f docker-compose-gpu.yml build --parallel dev

# View logs
gpu-logs:
	docker compose -f docker-compose-gpu.yml logs --since 30s -f

# Shell into dev container
gpu-shell:
	docker compose -f docker-compose-gpu.yml exec dev bash

# Run tests in GPU environment (optionally filter with TEST_FILTER env var)
# Usage: make gpu-test                          # Run all tests
#        make gpu-test TEST_FILTER=test_import  # Run specific test
#        make gpu-test TEST_FILTER=test_docling TEST_FLAGS=--nocapture
gpu-test:
	@if [ -z "$(TEST_FILTER)" ]; then \
		echo "Running all tests in GPU environment..."; \
		docker compose -f docker-compose-gpu.yml exec dev cargo test -- $(TEST_FLAGS); \
	else \
		echo "Running filtered tests: $(TEST_FILTER) in GPU environment..."; \
		docker compose -f docker-compose-gpu.yml exec dev cargo test $(TEST_FILTER) -- $(TEST_FLAGS); \
	fi

# Run lint in GPU environment
gpu-lint:
	@echo "Running lint checks in GPU container..."
	docker compose -f docker-compose-gpu.yml exec dev cargo clippy --all-targets --all-features -- -D warnings
	@echo "✅ GPU lint passed!"

# Run full CI in GPU environment
gpu-ci:
	@echo "Running full CI in GPU container..."
	docker compose -f docker-compose-gpu.yml exec dev cargo fmt --check
	docker compose -f docker-compose-gpu.yml exec dev cargo clippy --all-targets --all-features -- -D warnings
	docker compose -f docker-compose-gpu.yml exec dev cargo check --all-targets --all-features
	docker compose -f docker-compose-gpu.yml exec dev cargo test --lib
	@echo "✅ GPU CI passed!"

# Verify database schema is initialized and ready
gpu-verify-db:
	@echo "Verifying database schema initialization..."
	@echo ""
	@echo "Checking if database is accessible..."
	@docker exec rag-db pg_isready -U rag_user -d rag_chat > /dev/null 2>&1 || (echo "❌ Database not accessible"; exit 1)
	@echo "✓ Database is accessible"
	@echo ""
	@echo "Checking table count..."
	@TABLES=$$(docker exec rag-db psql -U rag_user -d rag_chat -tc "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='public'" | tr -d ' '); \
	if [ "$$TABLES" -gt 0 ]; then \
		echo "✓ Database schema created ($$TABLES tables found)"; \
	else \
		echo "❌ Database schema not initialized"; \
		exit 1; \
	fi
	@echo ""
	@echo "Database tables:"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename"
	@echo ""
	@echo "✅ Database schema verified successfully!"

# Show database statistics
gpu-db-stats:
	@echo "Database Statistics:"
	@echo "==================="
	@echo ""
	@echo "Documents:"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT COUNT(*) as document_count FROM documents"
	@echo ""
	@echo "Document Chunks:"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT COUNT(*) as chunk_count FROM document_chunks"
	@echo ""
	@echo "Import Jobs:"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT COUNT(*) as job_count FROM import_jobs"
	@echo ""
	@echo "Database Size:"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT pg_size_pretty(pg_database_size('rag_chat')) as database_size"
	@echo ""

# =============================================================================
# Maintenance & Backup
# =============================================================================

# Reset database (drop and recreate empty)
db-reset:
	@echo "Resetting database..."
	@docker exec rag-db psql -U rag_user -d postgres -c "DROP DATABASE IF EXISTS rag_chat; CREATE DATABASE rag_chat;" 2>/dev/null
	@echo "✅ Database reset complete (empty)"

# Apply schema migrations (run init.sql on current database)
db-migrate:
	@echo "Applying schema migrations..."
	@docker exec -i rag-db psql -U rag_user -d rag_chat < ./sql/init.sql > /dev/null 2>&1
	@echo "✅ Schema migrations applied"

# Refresh database indexes for optimal search performance
# Run this periodically or after large data imports
db-refresh-indexes:
	@echo "Refreshing database indexes for optimal search performance..."
	@echo "This will:"
	@echo "  1. VACUUM ANALYZE documents and document_chunks"
	@echo "  2. REINDEX BM25 indexes (documents_search_idx, chunks_search_idx)"
	@echo "  3. REINDEX HNSW vector indexes"
	@echo ""
	@docker exec rag-db psql -U rag_user -d rag_chat -c "VACUUM ANALYZE documents;"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "VACUUM ANALYZE document_chunks;"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "REINDEX INDEX documents_search_idx;"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "REINDEX INDEX chunks_search_idx;"
	@docker exec rag-db psql -U rag_user -d rag_chat -c "REINDEX INDEX CONCURRENTLY idx_documents_embedding;" 2>/dev/null || true
	@docker exec rag-db psql -U rag_user -d rag_chat -c "REINDEX INDEX CONCURRENTLY idx_document_chunks_embedding;" 2>/dev/null || true
	@echo "✅ Index refresh complete!"

# Backup the database to /data/backups
db-backup:
	@echo "Backing up database to /data/backups..."
	@sudo mkdir -p /data/backups
	@sudo chmod 777 /data/backups
	@BACKUP_FILE="/data/backups/rag_chat_$$(date +%Y%m%d_%H%M%S).dump"; \
	docker exec rag-db pg_dump -U rag_user -d rag_chat -F c -f /tmp/temp_backup.dump; \
	docker cp rag-db:/tmp/temp_backup.dump $$BACKUP_FILE; \
	docker exec rag-db rm /tmp/temp_backup.dump; \
	echo "✅ Backup created: $$BACKUP_FILE"; \
	ls -lh $$BACKUP_FILE

# Restore the database from a backup file
# Usage: make db-restore BACKUP_FILE=/data/backups/rag_chat_20260125_202752.dump
# This target:
# 1. Drops and recreates the database (fresh start)
# 2. Restores data AND schema from backup
# 3. Skips schema migrations (backup already includes current schema)
db-restore:
	@if [ -z "$(BACKUP_FILE)" ]; then \
		echo "❌ Error: BACKUP_FILE is not set. Usage: make db-restore BACKUP_FILE=/path/to/backup.dump"; \
		exit 1; \
	fi
	@echo "Restoring database from $(BACKUP_FILE)..."
	@echo ""
	@echo "Step 1: Terminating active connections..."
	@docker exec rag-db psql -U rag_user -d postgres -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname='rag_chat' AND pid != pg_backend_pid();" 2>/dev/null || true
	@echo "✅ Connections terminated"
	@echo ""
	@echo "Step 2: Dropping and recreating database..."
	@docker exec rag-db psql -U rag_user -d postgres -c "DROP DATABASE IF EXISTS rag_chat; CREATE DATABASE rag_chat;" 2>/dev/null || true
	@echo "✅ Database recreated"
	@echo ""
	@echo "Step 3: Restoring data from backup..."
	@docker cp $(BACKUP_FILE) rag-db:/tmp/restore.dump
	@echo "  Running pg_restore (this may take a few minutes for large backups)..."
	@bash -c 'docker exec rag-db pg_restore -U rag_user -d rag_chat --no-acl --no-owner /tmp/restore.dump >/dev/null 2>&1; exit 0'
	@docker exec rag-db rm /tmp/restore.dump 2>/dev/null || true
	@echo "✅ Data restored (schema already included in backup)"
	@echo ""
	@echo "Step 4: Verifying restore..."
	@docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT COUNT(*) as documents FROM documents;"
	@echo ""
	@echo "✅ Database restore complete!"

# =============================================================================
# Integration Testing
# =============================================================================

# Test Docling document processing (requires Docling service running)
docling-test:
	@echo "Testing Docling document processing..."
	@curl -s http://localhost:5001/health > /dev/null 2>&1 || (echo "❌ Docling not running. Start with: make gpu-up" && exit 1)
	@echo "✓ Docling service is healthy"
	RUN_ENV=test-gpu cargo test --test integration_test test_docling_pipeline -- --nocapture

# Test Docling PDF import via GPU container (requires: make gpu-up)
docling-import-test:
	@echo "Testing Docling PDF import in GPU environment..."
	@docker compose -f docker-compose-gpu.yml ps dev > /dev/null 2>&1 || (echo "❌ Dev container not running. Start with: make gpu-up" && exit 1)
	@echo "Running test_import_wellbeing_pdf..."
	docker compose -f docker-compose-gpu.yml exec dev cargo test --test import_test test_import_wellbeing_pdf -- --nocapture
	@echo "✅ Docling PDF import test complete!"

# Test Docling service health via GPU container
docling-health-test:
	@echo "Testing Docling service health in GPU environment..."
	@docker compose -f docker-compose-gpu.yml ps dev > /dev/null 2>&1 || (echo "❌ Dev container not running. Start with: make gpu-up" && exit 1)
	@echo "Running test_docling_service_health..."
	docker compose -f docker-compose-gpu.yml exec dev cargo test --test import_test test_docling_service_health -- --nocapture
	@echo "✅ Docling health test complete!"

# Test Docling file format support via GPU container
docling-format-test:
	@echo "Testing Docling file format support in GPU environment..."
	@docker compose -f docker-compose-gpu.yml ps dev > /dev/null 2>&1 || (echo "❌ Dev container not running. Start with: make gpu-up" && exit 1)
	@echo "Running test_docling_file_format_support..."
	docker compose -f docker-compose-gpu.yml exec dev cargo test --test import_test test_docling_file_format_support -- --nocapture
	@echo "✅ Docling format test complete!"

# Test reranker integration (requires Ollama with reranker model)
reranker-test:
	@echo "Testing reranker integration..."
	@echo "Ensure Ollama has reranker model: docker exec ollama ollama pull dengcao/Qwen3-Reranker-0.6B:Q5_K_M"
	RUN_ENV=test-gpu cargo test --test reranker_test -- --nocapture --test-threads=1

# ============================================
# Search Performance Tests
# ============================================

# Run database-level search performance tests
search-perf-test:
	@echo "Running search performance tests (requires database)..."
	@echo "Tests will measure query times for BM25, facets, filters, and chunks"
	RUN_ENV=test-gpu cargo test --test search_performance_test -- --ignored --nocapture

# Run specific database performance test
search-perf-bm25:
	@echo "Testing BM25 search performance..."
	RUN_ENV=test-gpu cargo test --test search_performance_test bm25 -- --ignored --nocapture

search-perf-facets:
	@echo "Testing faceted search performance (keywords, entities, locations)..."
	RUN_ENV=test-gpu cargo test --test search_performance_test facet -- --ignored --nocapture

search-perf-filters:
	@echo "Testing filter performance (date range, combined, chunks)..."
	RUN_ENV=test-gpu cargo test --test search_performance_test filter -- --ignored --nocapture

search-perf-comprehensive:
	@echo "Running comprehensive search performance suite..."
	RUN_ENV=test-gpu cargo test --test search_performance_test comprehensive -- --ignored --nocapture

# Run API-level search tests
api-search-test:
	@echo "Running API search tests (requires server running: make gpu-up)..."
	@echo "Tests will send HTTP requests to the search endpoints"
	cargo test --test api_search_test -- --ignored --nocapture

api-search-benchmark:
	@echo "Running API search performance benchmark..."
	cargo test --test api_search_test search_performance_benchmark -- --ignored --nocapture

# Run all search tests (database + API)
search-tests-all: search-perf-test api-search-test
	@echo "✅ All search performance tests completed!"
