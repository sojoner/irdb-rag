.PHONY: test test-all test-db-reset test-db-init test-unit test-integration clean help docker-build docker-push docker-release gpu-up gpu-down gpu-build gpu-logs gpu-shell gpu-test gpu-watch

# Test configuration - use 1024 for text-embedding-qwen3-embedding-0.6b
EMBEDDING_DIMENSIONS ?= 1024

# Docker configuration
IMAGE_NAME ?= rag-chat
IMAGE_TAG ?= latest
REGISTRY ?= docker.io
IMAGE_FULL := $(REGISTRY)/$(IMAGE_NAME):$(IMAGE_TAG)
BUILD_JOBS ?= 0

help:
	@echo "Available targets:"
	@echo "  test              - Run core tests (unit + API, no external services)"
	@echo "  test-all          - Run ALL tests including Docling/vLLM (requires services running)"
	@echo "  test-unit         - Run only unit tests (fast, no DB required)"
	@echo "  test-integration  - Run integration tests (requires DB reset)"
	@echo "  test-db-reset     - Reset test database schema (DOWN and UP)"
	@echo "  test-db-init      - Initialize test database with correct embedding dimensions"
	@echo "  clean             - Clean all test artifacts (DB data, target/)"
	@echo "  docker-build      - Build Docker image with BuildKit (fast: no LTO)"
	@echo "  docker-push       - Push Docker image to registry"
	@echo "  docker-release    - Build, tag, and push Docker image (all-in-one)"
	@echo ""
	@echo "Environment variables:"
	@echo "  EMBEDDING_DIMENSIONS - Embedding vector dimensions (default: 1024)"
	@echo "  IMAGE_NAME           - Docker image name (default: rag-chat)"
	@echo "  IMAGE_TAG            - Docker image tag (default: latest)"
	@echo "  REGISTRY             - Docker registry (default: docker.io)"
	@echo "  BUILD_JOBS           - Parallel build jobs, 0=all cores (default: 0)"
	@echo ""
	@echo "Build profiles:"
	@echo "  release       - No LTO, fastest compile (default)"
	@echo "  thin-lto      - Thin LTO, balanced speed and size"
	@echo "  production    - Full LTO, slow compile, maximum optimization"

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
	@echo "Starting GPU development environment..."
	RUN_ENV=test EMBEDDING_DIMENSIONS=1024 docker compose -f docker-compose-gpu.yml up -d
	@echo "Waiting for services to initialize..."
	@sleep 10
	@echo "GPU development environment ready!"
	@echo "  - App:     http://localhost:3000"
	@echo "  - Docling: http://localhost:5001"
	@echo "  - Ollama:  http://localhost:11434"
	@echo "  - DB:      localhost:15432"

# Stop GPU development environment
gpu-down:
	docker compose -f docker-compose-gpu.yml down

# Build dev container with parallel compilation
gpu-build:
	@echo "Building dev container with 16 parallel jobs..."
	DOCKER_BUILDKIT=1 docker compose -f docker-compose-gpu.yml build --parallel dev

# View logs
gpu-logs:
	docker compose -f docker-compose-gpu.yml logs -f

# Shell into dev container
gpu-shell:
	docker compose -f docker-compose-gpu.yml exec dev bash

# Run tests in GPU environment
gpu-test:
	docker compose -f docker-compose-gpu.yml exec dev cargo test

# Watch mode in dev container
gpu-watch:
	docker compose -f docker-compose-gpu.yml exec dev cargo leptos watch
