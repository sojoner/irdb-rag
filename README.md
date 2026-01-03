# RAG Chat - Hybrid Search Document Chat System

A Rust-based RAG (Retrieval Augmented Generation) system using:
- **ParadeDB** - PostgreSQL with pg_search (BM25) + pgvector for hybrid search
- **Leptos/Axum** - Rust web framework
- **FastEmbed** - Local ONNX embeddings (no API needed)
- **OpenAI/Anthropic/OpenRouter** - LLM completion APIs

## 🚀 Quick Start (5 minutes)

### 1. Prerequisites
```bash
# Install Rust (if not already)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Docker for the database
docker --version  # Ensure Docker is installed
```

### 2. Start the Database
```bash
# Clone and enter the project
cd rag-chat

# Start ParadeDB (includes pgvector + pg_search)
docker compose up -d

# Wait for healthy status
docker compose ps
```

### 3. Configure
```bash
# Copy and edit config
cp .env.example .env

# Edit .env with your LLM API key
# For OpenAI:
#   LLM_API_KEY=sk-your-key
# For Anthropic:
#   LLM_API_URL=https://api.anthropic.com/v1
#   LLM_API_KEY=sk-ant-your-key
```

### 4. Build & Run
```bash
# First run (downloads embedding model, ~100MB)
cargo run

# Or run with logging
RUST_LOG=info cargo run
```

### 5. Access
- **Web UI**: http://localhost:3000
- **PgAdmin**: http://localhost:5050 (admin@rag.local / admin)

## 📄 Index Documents

### Index a PDF
```bash
cargo run -- index --path ./docs/my-presentation.pdf
```

### Index a Markdown file
```bash
cargo run -- index --path ./docs/notes.md
```

### Index a URL
```bash
cargo run -- index --url https://example.com/whitepaper.pdf
```

### Watch folders for changes
```bash
cargo run -- watch --folders ./docs,./presentations
```

## 🔍 Search Features

### Hybrid Search
Combines BM25 (keyword) + Vector (semantic) search using Reciprocal Rank Fusion (RRF).

Adjust weights in the UI sidebar:
- Higher BM25 weight → More exact keyword matches
- Higher Vector weight → More semantic similarity

### Filters
- **Category**: Filter by taxonomy (Technology, AI, etc.)
- **Date Range**: Filter by document date
- **Location**: Filter by mentioned locations
- **Keywords**: Filter by extracted keywords

## 💬 Chat with Documents

1. Switch to Chat view
2. Ask questions about your documents
3. Claude responds using retrieved context
4. Sources are cited with relevance scores

## 🛠 Architecture

```
┌─────────────────────────────────────────────────────────┐
│                     Leptos/Axum UI                      │
├─────────────────────────────────────────────────────────┤
│                       REST API                          │
├──────────────┬──────────────┬──────────────────────────┤
│   Indexer    │   Search     │       Chat               │
│  (FastEmbed) │  (Hybrid)    │  (LLM + Context)         │
├──────────────┴──────────────┴──────────────────────────┤
│                    ParadeDB                             │
│         (PostgreSQL + pg_search + pgvector)             │
└─────────────────────────────────────────────────────────┘
```

### Database Schema

| Table | Purpose |
|-------|---------|
| `documents` | Core document storage with embeddings |
| `document_chunks` | Granular chunks for retrieval |
| `document_assets` | Images, formulas, tables |
| `categories` | Wikipedia-style taxonomy |
| `conversations` | Chat history |
| `messages` | Chat messages with context refs |

### Hybrid Search Algorithm

```sql
-- RRF (Reciprocal Rank Fusion)
combined_score = 
    bm25_weight * (1 / (60 + bm25_rank)) +
    vector_weight * (1 / (60 + vector_rank))
```

## 📊 API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/search` | POST | Hybrid search with filters |
| `/api/chat` | POST | Chat with RAG context |
| `/api/documents` | GET | List documents |
| `/api/documents/:id` | GET | Get document |
| `/api/documents/:id/assets` | GET | Get extracted assets |
| `/api/documents/:id/markdown` | GET | Export as markdown |
| `/api/categories` | GET | List taxonomy categories |
| `/api/index` | POST | Index document/URL |

## ⚡ Performance

Expected latencies (Mac M2):

| Operation | Time |
|-----------|------|
| Embed 1 chunk | ~5ms |
| BM25 search (100k docs) | ~10ms |
| Vector search (100k docs) | ~15ms |
| Hybrid search (RRF) | ~30ms |
| Full RAG response | 1-3s |

## 🔧 Configuration

### Embedding Models

Change in `.env`:
```bash
# Fast, good quality (default)
EMBEDDING_MODEL=all-MiniLM-L6-v2

# Better quality, slower
EMBEDDING_MODEL=bge-small-en-v1.5

# Quantized, 2x faster
EMBEDDING_MODEL=all-MiniLM-L6-v2-q
```

### LLM Providers

**OpenAI:**
```env
LLM_PROVIDER=openai
LLM_API_URL=https://api.openai.com/v1
LLM_MODEL=gpt-4
```

**Anthropic:**
```env
LLM_PROVIDER=anthropic
LLM_API_URL=https://api.anthropic.com/v1
LLM_MODEL=claude-3-opus-20240229
```

**OpenRouter (any model):**
```env
LLM_PROVIDER=openrouter
LLM_API_URL=https://openrouter.ai/api/v1
LLM_MODEL=anthropic/claude-3-opus
```

## 🧪 Testing & Coverage

### Run Tests
```bash
# All tests
cargo test

# Integration tests only
cargo test --test '*'

# Specific test
cargo test test_indexing_pipeline_metadata_and_enrichment
```

### Code Coverage

Or run manually:
```bash
# HTML report
cargo llvm-cov --features ssr --html
open target/llvm-cov/html/index.html

# LCOV (for CI/Codecov)
cargo llvm-cov --features ssr --lcov --output-path lcov.info

# Both formats
cargo llvm-cov --features ssr --html --lcov --output-path lcov.info
```

## 📚 See Also

- [AGENT.md](./AGENT.md) - Technical deep-dive and research
- [ParadeDB Docs](https://docs.paradedb.com/)
- [FastEmbed](https://github.com/Anush008/fastembed-rs)

## 🐛 Troubleshooting

**Database connection error:**
```bash
docker compose logs db
# Ensure container is healthy
docker compose ps
```

**Embedding model download fails:**
```bash
# Models are cached in ~/.cache/huggingface/
rm -rf ~/.cache/huggingface/fastembed
cargo run
```

**Search returns no results:**
```sql
-- Check index exists
\d+ documents_search_idx

-- Reindex if needed
CALL paradedb.create_bm25(...);
```

## License

MIT
