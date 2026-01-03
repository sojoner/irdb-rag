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

### Index a single file
```bash
cargo run -- index --path ./documents/my-presentation.pdf
```

### Index an entire directory (with progress tracking)
```bash
# Basic indexing with progress
cargo run -- index --path ./documents

# With debug logging to see enriched metadata
RUST_LOG=debug cargo run -- index --path ./documents
```

### Index a URL
```bash
cargo run -- index --url https://example.com/whitepaper.pdf
```

### Watch folders for real-time indexing
```bash
cargo run -- watch --folders ./documents
```

### Progress Output

The indexer shows detailed progress:
```
📚 Found 50 documents to index (1250.45 MB total)

┌─ Document 1/50: guide.pdf (0.45 MB)
  ├─ Stage 1/5: Extracting & enriching content...
  │   ✓ Duration: 2.34s
  │   📄 Title: Platform Engineering Guide
  │   📝 Summary: A comprehensive guide to building platform...
  │   🔑 Keywords: ["platform", "engineering", "kubernetes"]
  │   👥 Entities: { "persons": [...], "organizations": [...] }
  ├─ Stage 2/5: Chunking content...
  │   ✓ Duration: 0.12s | Created 15 chunks
  ├─ Stage 3/5: Enriching chunks...
  │   ✓ Duration: 0.08s
  ├─ Stage 4/5: Generating embeddings...
  │   ✓ Duration: 3.45s | Embedded 16 items
  └─ Stage 5/5: Storing in database...
      ✓ Duration: 0.34s | Stored document #1
  ⏱️  Total time: 6.33s

└─ ✓ Completed

... (more documents)

🎉 Indexing complete: 50 documents processed (1250.45 MB total)
```

**Features:**
- ✅ **Document Progress**: `Document X/Y` shows overall progress
- ✅ **5-Stage Pipeline**: Extract → Chunk → Enrich → Embed → Store
- ✅ **Timing Data**: Duration for each stage and total time
- ✅ **Bin Packing**: Smaller files processed first for quicker visible progress
- ✅ **Debug Traces**: `RUST_LOG=debug` shows:
  - Extracted title, summary, keywords
  - All named entities (persons, organizations, products, locations, concepts)
  - Formatted JSON of enriched metadata

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

### Document Preview

Click on any search result to open a rich preview modal showing:

- Full document metadata (type, word count, creation date, source)
- Document summary
- All extracted keywords (as blue tags)
- Mentioned locations (as green tags)
- Full document content (scrollable)
- Author information

The preview modal takes up 60% of the screen on the right side with a clean, organized layout.

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
