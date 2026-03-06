# RAG Chat - Testing Ground for RAG with PostgreSQL

A Rust full-stack RAG (Retrieval Augmented Generation) system using **Leptos** (frontend), **Axum** (backend), and **PostgreSQL** with hybrid search (BM25 + vector embeddings).

**Purpose**: Experimental testbed for RAG patterns—document indexing, semantic + keyword search, and LLM-augmented chat.

---

## ⚡ Quick Start

### Prerequisites
- **Rust** (1.70+): [rustup.rs](https://rustup.rs/)
- **Docker & Docker Compose**
- **AI Provider**: OpenAI-compatible API for embeddings + chat
  - Local: [LM Studio](https://lmstudio.ai/) (free, runs locally)
  - Cloud: OpenRouter, OpenAI, or Ollama

### 1. Start the Stack
```bash
docker compose up -d
```

This starts:
- **PostgreSQL** (port 15432) — hybrid search engine with `pgvector` + `pg_search`
- **Ollama** (port 11434) — local LLM & embeddings
- **Docling** (port 5001) — document processing (GPU-accelerated)

### 2. Configure AI Endpoints
Edit `config/production.toml`:
```toml
[llm.chat]
provider = "openai"
api_url = "http://localhost:1234/v1"  # LM Studio
api_key = "lm-studio"
model = "qwen2.5-7b-instruct"

[embedding]
provider = "openai"
api_url = "http://localhost:1234/v1"
api_key = "lm-studio"
model = "text-embedding-nomic-embed-text-v1.5"
dimensions = 768
```

Or use environment variables:
```bash
export APP_LLM__CHAT__API_KEY=your-key
export APP_DATABASE__URL=postgres://user:pass@host/db
```

### 3. Run the App
```bash
cargo run -- serve
```

Open **http://localhost:3000**

---

## 📚 Examples from Test Suite

See practical use cases in `tests/`:

### Search + Retrieval
```bash
# Hybrid search (BM25 + vector)
cargo test test_db_hybrid_search_syntax -- --nocapture

# Normalized scoring
cargo test test_search_scores_are_normalized -- --nocapture
```

### Chat + Conversation
```bash
# Multi-turn conversation persistence
cargo test test_multi_turn_conversation_persistence -- --nocapture

# Message save/load
cargo test test_save_and_load_messages -- --nocapture
```

### Run All Tests
```bash
cargo test
```

---

## 🐳 Docker Compose Stack

| Service | Port | Purpose |
|---------|------|---------|
| **PostgreSQL** | 15432 | Database with hybrid search (`pgvector`, `pg_search`) |
| **Ollama** | 11434 | Local LLM + embeddings (GPU-accelerated if available) |
| **Docling** | 5001 | Document extraction & enrichment |
| **Rust App** | 3000 | Web UI + REST API |

**Key files**:
- `sql/init.sql` — Database schema (documents, chunks, search functions)
- `config/` — Configuration (default, production, test)
- `src/` — Rust backend + Leptos frontend

---

## 🔍 Core Operations

### Index Documents
```bash
# Directory
cargo run -- index --path ./documents

# Single file
cargo run -- index --path ./documents/report.pdf

# Watch for changes
cargo run -- watch --folders ./documents
```

### Search API
```bash
curl -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query": "kubernetes", "limit": 5}'
```

### Chat API
```bash
curl -X POST http://localhost:3000/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [{"role": "user", "content": "What is RAG?"}],
    "conversation_id": null
  }'
```

---

## 📖 What's Inside

- **Hybrid Search**: BM25 (keyword) + vector (semantic) with Reciprocal Rank Fusion
- **5-Stage Pipeline**: Extract → Chunk → Enrich → Embed → Index
- **Job Management**: Batch import with retry, error tracking, progress monitoring
- **Conversational Memory**: Multi-turn chat with context preservation

---

## 🛠 Development

### Hot Reload
```bash
cargo leptos watch
```

### Database Migrations
```bash
sqlx migrate run
```

### Development Environment
```bash
make dev-up       # Start with cargo leptos watch
make dev-down     # Stop
make dev-build    # Rebuild dev container
```

### Run Tests
```bash
make test              # Unit tests (fast)
make test-integration  # Integration tests (requires DB)
make test-coverage     # Coverage report (requires cargo-tarpaulin)
```

### Database Backup/Restore
```bash
make db-backup                                    # Create backup
make db-restore BACKUP_FILE=./data/backups/...   # Restore from backup
```

---

## License

© 2026 build by sojoner with AI
