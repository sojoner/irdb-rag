# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

---

# IRDB-RAG Project Guidelines

A Rust-based RAG (Retrieval Augmented Generation) application using Leptos 0.8, PostgreSQL with pgvector + pg_search (ParadeDB) for hybrid search, running in Docker with GPU acceleration.

## Directory Structure

```
.claude/
├── CLAUDE.md          # This file - project guidelines and conventions
├── memory/            # Persistent memory files (learnings, decisions, context)
├── specs/             # Feature specifications and requirements
├── plans/             # Implementation plans before coding
└── settings.local.json # Local Claude Code settings
```

### File Purposes

- **memory/**: Store learnings about the codebase, user preferences, architectural decisions
- **specs/**: Feature specifications with acceptance criteria (write before implementing)
- **plans/**: Step-by-step implementation plans (write before coding complex features)

---

## Clean Code Principles (Uncle Bob - Rust Adapted)

### SOLID in Rust

1. **Single Responsibility**: Each module/struct has one reason to change
2. **Open/Closed**: Use traits for extension without modification
3. **Liskov Substitution**: Trait implementations must be substitutable
4. **Interface Segregation**: Small, focused traits over large ones
5. **Dependency Inversion**: Depend on abstractions (traits), not concrete types

### Rust-Specific Guidelines

```rust
// GOOD: Small, focused functions with clear names
fn calculate_embedding_similarity(a: &[f32], b: &[f32]) -> f32 { ... }

// BAD: Vague names, multiple responsibilities
fn process(data: &Data) -> Result<()> { ... }
```

### Naming Conventions

- **Modules**: `snake_case` (e.g., `document_storage`, `hybrid_search`)
- **Types/Traits**: `PascalCase` (e.g., `DocumentChunk`, `Embedder`)
- **Functions**: `snake_case`, verb-first (e.g., `index_document`, `search_hybrid`)
- **Constants**: `SCREAMING_SNAKE_CASE`

### Test Naming

Tests follow hierarchical naming with common prefixes:
```rust
#[test]
fn test_search_returns_results_for_valid_query() { ... }

#[test]
fn test_search_handles_empty_query_gracefully() { ... }
```

---

## Test-Driven Development Flow

### TDD Cycle

1. **Red**: Write a failing test first
2. **Green**: Write minimal code to pass
3. **Refactor**: Clean up while tests pass

### Test Categories

| Type | Location | Command | Requires |
|------|----------|---------|----------|
| Unit | `src/**/tests.rs` or `#[cfg(test)]` | `make test-unit` | Nothing |
| Integration | `tests/*.rs` | `make test-integration` | Database |
| E2E | `tests/e2e_*.rs` | `make test-all` | Full stack |

### Test Structure (Arrange-Act-Assert)

```rust
#[tokio::test]
async fn test_hybrid_search_combines_bm25_and_vector() {
    // Arrange
    let pool = setup_test_db().await;
    let embedder = create_test_embedder();
    insert_test_documents(&pool).await;

    // Act
    let results = hybrid_search(&pool, &embedder, "test query", 10).await.unwrap();

    // Assert
    assert!(!results.is_empty());
    assert!(results[0].combined_score > 0.0);
}
```

### Database Tests

Use the test database container. Tests should:
1. Set up their own test data
2. Clean up after themselves
3. Use unique identifiers to avoid conflicts

```rust
#[tokio::test]
async fn test_document_storage() {
    let pool = get_test_pool().await;
    let doc_id = Uuid::new_v4();

    // ... test logic ...

    // Cleanup
    sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(doc_id)
        .execute(&pool)
        .await
        .unwrap();
}
```

---

## Docker Development Environment

### GPU Stack (RTX 3090)

```bash
# Start full GPU environment
make gpu-up

# Run tests in container
make gpu-test

# Shell into dev container
make gpu-shell

# View logs
make gpu-logs
```

### Services Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Host Machine (Linux with NVIDIA GPU)                       │
├─────────────────────────────────────────────────────────────┤
│  docker-compose-gpu.yml                                     │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   Ollama    │  │   Docling   │  │   PostgreSQL        │  │
│  │  :11434     │  │   :5001     │  │   :15432            │  │
│  │  GPU: cuda  │  │  GPU: cuda  │  │   pgvector          │  │
│  │             │  │             │  │   pg_search         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
│  ┌─────────────────────────────────────────────────────────┐│
│  │              Dev Container (rag-dev)                    ││
│  │  cargo leptos serve | cargo test                        ││
│  │  :3000 (app) :3001 (reload)                             ││
│  └─────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────┘
```

### Environment Variables

```bash
RUN_ENV=test-gpu           # Load config/test-gpu.toml
EMBEDDING_DIMENSIONS=1024  # Vector dimensions for qwen3-embedding
RUST_LOG=info              # Logging level
```

---

## Code Quality Gates

### Before Every Commit

All code must pass these checks (enforced via Makefile):

```bash
# Lint with all warnings as errors
cargo clippy --all-targets --all-features -- -D warnings

# Format check
cargo fmt --check

# Compile in release mode (catches more issues)
cargo build --release

# Run tests
make test
```

### Clippy Configuration

Enable strict lints in `Cargo.toml` or `.cargo/config.toml`:
```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
```

### No Warnings Policy

- All warnings must be resolved before merge
- Use `#[allow(...)]` only with explanatory comment
- Prefer fixing the issue over suppressing

---
## What NOT to do

* Do not use 'git commit' the developer has the accountability not the LLM
* do not summarize befor validation, always run a test case for validating changes
* do not plan or get hung up in design, research what you miss using the web, than write test not plans and summaries

---

The application supports multiple subcommands via `cargo run --`:

```bash
# Web server (default when no command provided)
cargo run -- serve [--port 3000] [--skip-scan]

# Index documents from path or URL
cargo run -- index [--path ./docs] [--url https://example.com/file.pdf]

# Watch folders for changes and auto-index
cargo run -- watch --folders ./documents ./pdfs

# Import Chrome bookmarks
cargo run -- import-bookmarks --path /path/to/bookmarks.html

# Manually scan knowledge base
cargo run -- scan [--paths ./docs]
```

---

## Makefile Tasks (No Bash Scripts)

### Available Commands

```bash
make help              # Show all available commands

# Testing
make test              # Core tests (unit + API)
make test-unit         # Unit tests only (fast)
make test-integration  # Integration tests (requires DB)
make test-all          # All tests including Docling/Ollama

# Database
make test-db-reset     # Reset and reinitialize test DB
make test-db-init      # Initialize test DB schema

# GPU Development
make gpu-up            # Start GPU dev environment
make gpu-down          # Stop GPU dev environment
make gpu-test          # Run tests in GPU container
make gpu-shell         # Shell into dev container
make gpu-logs          # View container logs

# Docker
make docker-build      # Build production image
make docker-release    # Build and push to registry

# Code Quality
make lint              # Run clippy with -D warnings
make fmt               # Format code with rustfmt
make fmt-check         # Check formatting without modifying
make check             # Fast compile check
make ci                # Full CI: fmt, lint, check, test-unit

# Cleanup
make clean             # Remove all artifacts
```

### Adding New Tasks

Add to `Makefile` instead of creating shell scripts:

```makefile
.PHONY: my-new-task

my-new-task:
	@echo "Running my task..."
	cargo some-command --with-flags
```

---

## Indexing & Import

### Document Indexing Pipeline

The system processes documents through several stages:

1. **Document Extraction**: Parse files (PDF, MD, TXT, HTML) using Docling or built-in parsers
2. **Enrichment**: Generate summaries, keywords, entities via LLM
3. **Chunking**: Split content into manageable chunks (default: 512 tokens)
4. **Context Enrichment**: Prepend document metadata to each chunk for self-contained retrieval
5. **Embedding**: Generate vector embeddings for all chunks
6. **Storage**: Save to PostgreSQL with pgvector + BM25 indexes

See [services/indexing.rs](src/services/indexing.rs) and [services/enrichment.rs](src/services/enrichment.rs).

### Batch Import Jobs

For bulk ingestion, use the job system ([services/import.rs](src/services/import.rs)):

- **Automatic Retry**: Exponential backoff for transient errors
- **Error Classification**: Distinguishes transient (retryable) vs permanent (skip)
- **Progress Tracking**: Detailed status at job and item levels
- **Background Processing**: Non-blocking async workers
- **Job Cleanup**: Automatic old job cleanup via [services/job_cleanup.rs](src/services/job_cleanup.rs)

### Bookmark Import

Special support for Chrome bookmarks:

```bash
cargo run -- import-bookmarks --path /path/to/bookmarks.html
```

Converts bookmarks to documents with metadata. See [services/bookmark_parser.rs](src/services/bookmark_parser.rs).

---

## Search & Retrieval

### Reranking (Optional)

The system includes optional reranking support via [infra/reranker.rs](src/infra/reranker.rs):

```rust
// Rerank search results using a dedicated model (Qwen3-Reranker)
let reranked = reranker.rerank(&search_results, &query).await?;

// Configuration in TOML:
// [reranker]
// enabled = true
// provider = "ollama"
// api_url = "http://localhost:11434"
// model = "qwen3-reranker"
```

Reranking is disabled by default and requires a compatible reranker model.

### PostgreSQL Hybrid Search Optimization

### pg_search (ParadeDB BM25)

```sql
-- BM25 index for full-text search
CREATE INDEX documents_search_idx ON documents
USING bm25 (id, content, title, summary)
WITH (key_field='id');

-- Query with BM25 scoring
SELECT id, paradedb.score(id) as score
FROM documents
WHERE id @@@ 'search query'
ORDER BY score DESC;
```

### pgvector (Vector Similarity)

```sql
-- HNSW index for fast approximate nearest neighbor
CREATE INDEX ON documents
USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Query configuration
SET hnsw.ef_search = 100;  -- Higher = more accurate, slower
```

### Hybrid Search (RRF Fusion)

The `hybrid_search` SQL function combines BM25 and vector search:

```sql
-- Reciprocal Rank Fusion formula
combined_score = bm25_weight * (1.0 / (60 + bm25_rank))
               + vector_weight * (1.0 / (60 + vector_rank))
```

### Performance Best Practices

1. **Index Maintenance**
   ```sql
   VACUUM ANALYZE documents;
   REINDEX INDEX documents_search_idx;
   ```

2. **Query Analysis**
   ```sql
   EXPLAIN (ANALYZE, BUFFERS)
   SELECT * FROM hybrid_search('query', embedding, 10, 0.5, 0.5);
   ```

3. **Memory Configuration** (in docker-compose)
   ```yaml
   POSTGRES_INITDB_ARGS: >
     -c shared_buffers=2GB
     -c effective_cache_size=6GB
     -c work_mem=128MB
   ```

4. **Embedding Dimensions**
   - Use 1024 for qwen3-embedding:0.6b (balanced)
   - Keep indexes in memory: ensure sufficient RAM

---

## Architecture Layers

```
src/
├── api/           # HTTP handlers, routes, state
│   ├── handlers.rs
│   ├── routes.rs
│   └── state.rs
├── domain/        # Business logic, models, DTOs
│   ├── models.rs
│   ├── dtos.rs
│   └── errors.rs
├── infra/         # External services integration
│   ├── db.rs
│   ├── embedder.rs
│   ├── llm.rs
│   └── reranker.rs
├── services/      # Application services
│   ├── indexing.rs
│   ├── enrichment.rs
│   └── import.rs
├── web_app/       # Leptos components
│   ├── components/
│   └── pages/
├── config.rs      # Configuration loading
├── lib.rs         # Library exports
└── main.rs        # Application entry point
```

### Dependency Rules

- `api` depends on `domain`, `services`
- `services` depends on `domain`, `infra`
- `domain` has no internal dependencies
- `infra` depends only on `domain`

---

## Configuration

### TOML Files

```
config/
├── default.toml      # Base configuration
├── test.toml         # Local testing (vLLM)
└── test-gpu.toml     # GPU testing (Ollama)
```

### Loading Priority

1. `config/default.toml` (base)
2. `config/{RUN_ENV}.toml` (environment-specific)
3. Environment variables with `APP_` prefix

```rust
// Example: APP_DATABASE__URL overrides database.url
let settings = Settings::new()?;
```

---

## Common Patterns

### Error Handling

```rust
// Use thiserror for library errors
#[derive(thiserror::Error, Debug)]
pub enum SearchError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Embedding failed: {0}")]
    Embedding(String),
}

// Use anyhow for application errors
async fn process() -> anyhow::Result<()> {
    let result = search().context("Search failed")?;
    Ok(())
}
```

### Async Patterns

```rust
// Prefer structured concurrency
let (results_a, results_b) = tokio::join!(
    search_bm25(&pool, query),
    search_vector(&pool, embedding),
);

// Use channels for background tasks
let (tx, mut rx) = tokio::sync::mpsc::channel(100);
```

### Database Queries

```rust
// Always use parameterized queries
sqlx::query_as!(
    Document,
    r#"SELECT id, title, content FROM documents WHERE id = $1"#,
    doc_id
)
.fetch_one(&pool)
.await?
```

---

## Browser Testing & Debugging

### Leptos Watch + Browser DevTools

For interactive development and debugging:

```bash
# Terminal 1: Start dev server with hot reload
make gpu-up
# Or locally:
cargo leptos watch

# App is available at: http://localhost:3000
# Hot reload enabled on port :3001
```

Open your browser and use DevTools (F12):
- Inspect Leptos components
- Check network requests and API calls
- View browser console for errors
- Debug WASM output (when present)

### Using Playwright MCP for UI Testing

When a Playwright MCP (Model Context Protocol) tool is available in Claude Code, you can use it to:

1. **Open browser and navigate**
   ```
   Open browser to http://localhost:3000/search
   Test search input by typing "document"
   ```

2. **Verify UI elements**
   ```
   Check that search results are displayed
   Verify result count is greater than 0
   ```

3. **Test interactions**
   ```
   Click on a search result
   Verify document preview opens
   Check that navigation works
   ```

4. **Debug component behavior**
   ```
   Pause in browser to inspect HTML
   Check CSS styles and layout
   Verify data attributes (data-testid)
   ```

### Manual UI Testing Checklist

Since this is a remote GPU machine without display, use manual testing against `make gpu-up`:

- [ ] Search page loads without errors
- [ ] Search input accepts text
- [ ] Search button submits queries
- [ ] Results display with title, summary, score
- [ ] Filters toggle on/off
- [ ] Document preview opens on click
- [ ] Chat interface responds
- [ ] Import page accepts files
- [ ] No console errors (check DevTools)

### Adding Test-Friendly Attributes

Add `data-testid` to Leptos components for reliable element selection:

```rust
// src/web_app/components/search.rs
view! {
    <div class="search-container">
        <input
            type="text"
            placeholder="Search documents..."
            data-testid="search-input"
            // ...
        />
        <button
            type="submit"
            data-testid="search-button"
        >
            "Search"
        </button>
        <div data-testid="results-list">
            // Results here
        </div>
    </div>
}
```

These attributes help with:
- Playwright/MCP automation when tools are available
- Manual QA testing
- Browser console queries: `document.querySelector('[data-testid="search-input"]')`

---

## Git Workflow

### Commit Messages

```
feat: add hybrid search reranking support

- Implement Qwen3-Reranker integration
- Add reranking toggle in search config
- Update hybrid_search SQL function

Co-Authored-By: Claude <noreply@anthropic.com>
```

### Branch Naming

- `feature/description` - New features
- `fix/description` - Bug fixes
- `refactor/description` - Code improvements
- `test/description` - Test additions

---

## Quick Reference

```bash
# Start development
make gpu-up && make gpu-logs

# Run specific test
cargo test test_hybrid_search -- --nocapture

# Check before commit
cargo clippy -- -D warnings && cargo fmt --check && make test

# Database shell
docker exec -it rag-db psql -U rag_user -d rag_chat

# View search index stats
docker exec -it rag-db psql -U rag_user -d rag_chat \
  -c "SELECT * FROM paradedb.index_info('documents_search_idx');"
```
