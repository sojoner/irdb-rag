.PHONY: test test-all test-db-reset test-db-init test-unit test-integration clean help

# Test configuration - use 1024 for text-embedding-qwen3-embedding-0.6b
EMBEDDING_DIMENSIONS ?= 1024

help:
	@echo "Available targets:"
	@echo "  test              - Run core tests (unit + API, no external services)"
	@echo "  test-all          - Run ALL tests including Docling/vLLM (requires services running)"
	@echo "  test-unit         - Run only unit tests (fast, no DB required)"
	@echo "  test-integration  - Run integration tests (requires DB reset)"
	@echo "  test-db-reset     - Reset test database schema (DOWN and UP)"
	@echo "  test-db-init      - Initialize test database with correct embedding dimensions"
	@echo "  clean             - Clean all test artifacts (DB data, target/)"
	@echo ""
	@echo "Environment variables:"
	@echo "  EMBEDDING_DIMENSIONS - Embedding vector dimensions (default: 1024)"

# Reset and reinitialize the test database
test-db-reset:
	@echo "Resetting test database with EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS)..."
	docker compose down -v
	@echo "Cleaning up postgres data..."
	rm -rf data/postgres
	RUN_ENV=test EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS) docker compose up -d db
	@echo "Waiting for database to be healthy..."
	@sleep 5
	RUN_ENV=test EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS) docker compose up -d db-init
	@echo "Waiting for initialization to complete..."
	@sleep 3
	@echo "Test database reset complete!"

# Initialize test database (use if db is already running)
test-db-init:
	@echo "Initializing test database with EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS)..."
	RUN_ENV=test EMBEDDING_DIMENSIONS=$(EMBEDDING_DIMENSIONS) docker compose up -d db-init
	@echo "Test database initialization complete!"

# Run tests with database reset (excludes docling/integration tests requiring external services)
test: test-db-reset
	@echo "Running core tests (unit + API tests)..."
	RUN_ENV=test RUST_LOG=error cargo test --lib
	RUN_ENV=test RUST_LOG=error cargo test --test api_test
	RUN_ENV=test RUST_LOG=error cargo test --test chunking_test
	RUN_ENV=test RUST_LOG=error cargo test --test db_pool_test
	RUN_ENV=test RUST_LOG=error cargo test --test document_storage_test
	RUN_ENV=test RUST_LOG=error cargo test --test enricher_test
	@echo "✅ Core tests passed! Use 'make test-all' to run tests requiring external services (Docling, vLLM)"

# Run only unit tests (fast, no DB reset needed)
test-unit:
	@echo "Running unit tests..."
	RUN_ENV=test RUST_LOG=error cargo test --lib

# Run integration tests (requires DB)
test-integration: test-db-reset
	@echo "Running integration tests..."
	RUN_ENV=test RUST_LOG=error cargo test --test integration_test
	RUN_ENV=test RUST_LOG=error cargo test --test import_test
	RUN_ENV=test RUST_LOG=error cargo test --test embedding_test
	RUN_ENV=test RUST_LOG=error cargo test --test api_test

# Run ALL tests including those requiring external services (Docling on :5001, vLLM on :1234)
test-all: test-db-reset
	@echo "Running ALL tests (requires Docling and vLLM services)..."
	@echo "Starting Docling service..."
	RUN_ENV=test docker compose up -d docling
	@echo "Waiting for Docling to be ready (this may take 30-60 seconds)..."
	@for i in 1 2 3 4 5 6 7 8 9 10 11 12; do \
		sleep 5; \
		if curl -s http://localhost:5001/health > /dev/null 2>&1; then \
			echo "✓ Docling is ready!"; \
			break; \
		fi; \
		if [ $$i -eq 12 ]; then \
			echo "❌ Docling failed to start after 60 seconds. Check logs with: docker compose logs docling"; \
			exit 1; \
		fi; \
		echo "  Still waiting... ($$(($$i * 5))s)"; \
	done
	@echo "Checking vLLM availability..."
	@curl -s http://127.0.0.1:1234/health > /dev/null 2>&1 || (echo "⚠️  vLLM not running on :1234. Some tests may fail." && sleep 2)
	@echo "Running full test suite..."
	RUN_ENV=test RUST_LOG=error cargo test

# Clean all test artifacts
clean:
	@echo "Cleaning test artifacts..."
	docker compose down -v
	rm -rf data/postgres
	rm -rf target
	@echo "Clean complete!"
