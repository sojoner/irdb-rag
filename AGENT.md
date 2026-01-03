# AGENT.md - RAG Chat System Knowledge Base

This file provides technical guidance for AI agents (Claude Code, Cursor, etc.) working with code in this repository.

## Project Overview
A Rust-based RAG (Retrieval Augmented Generation) chat system using:
- **ParadeDB** (PostgreSQL + pg_search + pgvector) for hybrid search
- **Axum** for REST API (not Leptos - project uses Axum with static HTML)
- **FastEmbed** for local embeddings (ONNX runtime)
- **Docling** concepts for document parsing

See [README.md](README.md) for user-facing documentation and quick start.

---

## Essential Commands

### Development Workflow
```bash
# 1. Start database first
docker compose up -d

# 2. Run the server (default port 3000)
cargo run

# Run with logging
RUST_LOG=info cargo run

# Run with specific port
cargo run -- serve --port 8080

# Build for release
cargo build --release
```

### Document Indexing
```bash
# Index a single file
cargo run -- index --path /path/to/document.pdf

# Index entire directory
cargo run -- index --path /path/to/docs/

# Index from URL
cargo run -- index --url https://example.com/doc.pdf

# Watch folders for changes (auto-index)
cargo run -- watch --folders ./docs,./presentations
```

### Database Management
```bash
# Check database status
docker compose ps

# View database logs
docker compose logs db

# Connect to database directly
docker exec -it <container> psql -U rag_user -d rag_chat

# Access PgAdmin UI
# http://localhost:5050 (admin@rag.local / admin)
```

---

## Architecture Overview

### Module Structure

The codebase is organized into four main modules:

- **[src/main.rs](src/main.rs)** - CLI entry point with clap for command parsing (`serve`, `index`, `watch`)
- **[src/db.rs](src/db.rs)** - Database layer: connection pooling, hybrid search, CRUD operations
- **[src/api.rs](src/api.rs)** - Axum REST API handlers, LLM integration, app state
- **[src/indexer.rs](src/indexer.rs)** - Document parsing (PDF/Markdown/URLs), chunking, embedding generation

### Database Schema

The PostgreSQL schema is defined in [sql/init.sql](sql/init.sql). Key tables:

**Core Tables:**
- `documents` - Full documents with embeddings (vector(384)), metadata, full content
- `document_chunks` - Granular 512-token chunks with embeddings for retrieval
- `document_assets` - Extracted images, formulas, tables (base64 encoded)
- `categories` - Wikipedia-style taxonomy with parent-child relationships
- `conversations` & `messages` - Chat history with context tracking

**Critical Indexes:**
- BM25 full-text indexes via ParadeDB's pg_search on `documents` and `document_chunks`
- HNSW vector indexes on embeddings for fast ANN search
- GIN indexes on `keywords[]` and `locations[]` arrays

### Hybrid Search Implementation

The system uses **Reciprocal Rank Fusion (RRF)** to combine BM25 and vector search ([sql/init.sql:205-284](sql/init.sql#L205-L284)):

```sql
rrf_score = bm25_weight * (1 / (60 + bm25_rank)) +
            vector_weight * (1 / (60 + vector_rank))
```

**Implementation flow:**
1. BM25 search using ParadeDB's `@@@` operator ([db.rs:158](src/db.rs#L158))
2. Vector search using pgvector's `<=>` cosine distance ([db.rs:186](src/db.rs#L186))
3. RRF combination in the `hybrid_search()` SQL function ([sql/init.sql:264-265](sql/init.sql#L264-L265))
4. Results filtered by category, date range, and locations

**Key insight:** RRF uses ranks (not scores) which is scale-agnostic—works regardless of whether BM25 scores range 0-10 or 0-1000.

### Document Processing Pipeline

**Chunking Strategy** ([indexer.rs:166-189](src/indexer.rs#L166-L189)):
- Target: 512 tokens per chunk
- Overlap: 50 tokens
- Method: Fixed-size with overlap, splits on whitespace (word boundaries)
- No mid-sentence splitting

**Supported Formats:**
- PDF via `lopdf` crate ([indexer.rs:69-99](src/indexer.rs#L69-L99))
- Markdown ([indexer.rs:102-120](src/indexer.rs#L102-L120))
- Plain text
- URLs - HTML converted via `html2text`, PDFs downloaded to temp

**Processing Flow:**
1. Parse document → extract text
2. Generate chunks with overlap
3. Extract keywords (top 10 frequent non-stopwords, [indexer.rs:192-224](src/indexer.rs#L192-L224))
4. Embed document (using first chunk or summary)
5. Embed all chunks individually
6. Insert document + chunks into database

### LLM Integration

The chat API ([api.rs:161-213](src/api.rs#L161-L213)) follows this pattern:
1. Embed user query using FastEmbed
2. Retrieve top-k relevant chunks via `get_relevant_chunks()` (vector search on chunks)
3. Build context from chunk contents (separated by `---`)
4. Call LLM with system prompt + context + user question
5. Return response with source citations

**API Format:** OpenAI-compatible `/chat/completions` endpoint ([api.rs:312-348](src/api.rs#L312-L348))
- Works with OpenAI, Anthropic, and OpenRouter
- Configured via `LLM_PROVIDER`, `LLM_API_URL`, `LLM_API_KEY`, `LLM_MODEL` env vars

---

## Environment Configuration

Create a `.env` file in the repository root:

```bash
# Database
DATABASE_URL=postgres://rag_user:rag_password@localhost:15432/rag_chat

# LLM Provider (OpenAI example)
LLM_PROVIDER=openai
LLM_API_URL=https://api.openai.com/v1
LLM_API_KEY=sk-your-key-here
LLM_MODEL=gpt-4

# For Anthropic:
# LLM_PROVIDER=anthropic
# LLM_API_URL=https://api.anthropic.com/v1
# LLM_API_KEY=sk-ant-your-key
# LLM_MODEL=claude-3-opus-20240229

# For OpenRouter (any model):
# LLM_PROVIDER=openrouter
# LLM_API_URL=https://openrouter.ai/api/v1
# LLM_MODEL=anthropic/claude-3-opus

# Embedding Model (optional, defaults to all-MiniLM-L6-v2)
EMBEDDING_MODEL=all-MiniLM-L6-v2

# Logging (optional)
RUST_LOG=info
```

---

## REST API Endpoints

| Endpoint | Method | Purpose |
|----------|--------|---------|
| `/api/health` | GET | Health check |
| `/api/search` | POST | Hybrid search with filters and configurable weights |
| `/api/chat` | POST | RAG chat with context retrieval |
| `/api/documents` | GET | List documents (paginated with limit/offset) |
| `/api/documents/:id` | GET | Get single document by UUID |
| `/api/documents/:id/assets` | GET | Get extracted images, formulas, tables |
| `/api/documents/:id/markdown` | GET | Export document as markdown |
| `/api/categories` | GET | List taxonomy categories |
| `/api/index` | POST | Index new document by path or URL |

---

## Key Design Decisions

1. **Chunk-level retrieval:** While documents are stored whole in the `documents` table, RAG retrieval happens at the `document_chunks` level for better precision in large documents.

2. **RRF over weighted score fusion:** BM25 scores and vector cosine distances are on incompatible scales. RRF normalizes by rank instead, making it scale-agnostic.

3. **Local embeddings:** FastEmbed runs ONNX models locally (~5ms per chunk on M2), eliminating external API costs and 100ms+ latency per embedding.

4. **ParadeDB pg_search:** Native PostgreSQL BM25 indexing via ParadeDB eliminates need for Elasticsearch/Meilisearch while keeping everything in one database.

5. **Dual embedding strategy:** Both full documents AND chunks are embedded. Document embeddings enable high-level search; chunk embeddings provide precise RAG context.

---

## Common Development Tasks

### Testing Search Changes

```bash
# 1. Index test documents
cargo run -- index --path ./test-docs

# 2. Test via API
curl -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"machine learning","bm25_weight":0.5,"vector_weight":0.5,"limit":10}'

# 3. Test chat endpoint
curl -X POST http://localhost:3000/api/chat \
  -H "Content-Type: application/json" \
  -d '{"message":"What is this about?","context_chunks":5}'
```

### Testing Hybrid Search Function Directly

Connect to the database and test the SQL function:

```sql
-- Test with dummy embedding (replace with real 384-dim vector)
SELECT * FROM hybrid_search(
  'machine learning'::TEXT,
  '[0.1, 0.2, 0.3, ...]'::vector(384),  -- 384 values
  10,     -- match_count
  0.5,    -- bm25_weight
  0.5     -- vector_weight
);
```

### Verifying Embeddings

```sql
-- Check if documents have embeddings
SELECT COUNT(*) FROM documents WHERE embedding IS NOT NULL;
SELECT COUNT(*) FROM document_chunks WHERE embedding IS NOT NULL;

-- Check embedding dimensions
SELECT id, array_length(embedding::real[], 1) as dims
FROM documents
WHERE embedding IS NOT NULL
LIMIT 1;
```

### Rebuilding BM25 Index

If BM25 search stops working or you need to recreate the index:

```sql
-- Drop existing index
CALL paradedb.drop_bm25('documents_search_idx');

-- Recreate (copy exact config from sql/init.sql:164-172)
CALL paradedb.create_bm25(
    index_name => 'documents_search_idx',
    schema_name => 'public',
    table_name => 'documents',
    key_field => 'id',
    text_fields => paradedb.field('content', tokenizer => paradedb.tokenizer('en_stem')) ||
                   paradedb.field('title', tokenizer => paradedb.tokenizer('en_stem'), boost => 2.0) ||
                   paradedb.field('summary', tokenizer => paradedb.tokenizer('en_stem'), boost => 1.5)
);
```

---

## Important Notes for Code Changes

### Changing Embedding Dimensions

If you change the embedding model to one with different dimensions (e.g., 768 instead of 384), you must update:

1. **Rust code:** Vector lengths in [db.rs](src/db.rs) and [indexer.rs](src/indexer.rs)
2. **SQL schema:** All `vector(384)` → `vector(768)` in [sql/init.sql](sql/init.sql)
3. **SQL functions:** `query_embedding vector(384)` parameter in `hybrid_search()` function
4. **Database:** Either drop/recreate tables or use `ALTER TABLE` to change column types

### Adding New Document Types

To add support for new file formats:

1. Add parser in [indexer.rs](src/indexer.rs:257-274) following the existing pattern
2. Extract text → chunk → extract keywords
3. Return `ProcessedDocument` struct
4. Update file extension matching in `index_file()` function

Example structure:
```rust
fn parse_docx(path: &Path) -> Result<ProcessedDocument> {
    // Extract text from .docx
    let content = extract_docx_text(path)?;

    let title = path.file_stem()...;
    let chunks = chunk_text(&content, 512, 50);
    let keywords = extract_keywords(&content);

    Ok(ProcessedDocument {
        title,
        content,
        source_type: "docx".to_string(),
        chunks,
        keywords,
    })
}
```

### Modifying Chunk Size

To change chunk size and overlap:

1. Update `chunk_text()` calls in [indexer.rs](src/indexer.rs:89, 110, 149, 266)
   - Currently: `chunk_text(&content, 512, 50)` (512 tokens, 50 overlap)
2. Consider trade-offs:
   - Smaller chunks = more precise retrieval, less context per chunk
   - Larger chunks = more context, potentially less precise matching
3. Re-index existing documents after changes

### Modifying LLM Integration

If adding a new LLM provider with different API format:

1. Update [api.rs](src/api.rs:312-348) `call_llm()` function
2. Handle different request/response formats
3. Add provider-specific logic if needed (e.g., Anthropic's `anthropic-version` header)
4. Test thoroughly with actual API before committing

---

## Hybrid Search Strategy (State of the Art 2024-2025)

### Why Hybrid Search?
Pure vector search misses exact keyword matches. Pure BM25 misses semantic meaning.
**Hybrid search combines both for 15-25% better recall** (benchmarks from BEIR dataset).

### Reciprocal Rank Fusion (RRF)
The gold standard for combining rankings:
```
RRF_score(d) = Σ 1/(k + rank_i(d))
```
Where `k=60` is standard (found empirically optimal).

**Key insight**: RRF is score-agnostic—only uses ranks, so it works regardless of whether BM25 scores range 0-10 or 0-1000.

### Recommended Weights (from ParadeDB research)
| Use Case | BM25 Weight | Vector Weight |
|----------|-------------|---------------|
| Technical docs | 0.6 | 0.4 |
| Conversational | 0.3 | 0.7 |
| Legal/exact match | 0.8 | 0.2 |
| General purpose | 0.5 | 0.5 |

### Re-ranking for Production
After initial retrieval, use a **cross-encoder reranker** for top-k results:
1. Retrieve top 50 with hybrid search
2. Re-rank with `BAAI/bge-reranker-base` or similar
3. Return top 10

**Rust implementation**: Use `fastembed` with reranking models.

---

## Embedding Models (Local, Mac M-series Optimized)

### Recommended Models
| Model | Dims | Speed | Quality | Use Case |
|-------|------|-------|---------|----------|
| `all-MiniLM-L6-v2` | 384 | Fast | Good | General, MVP |
| `bge-small-en-v1.5` | 384 | Fast | Better | Production |
| `nomic-embed-text-v1.5` | 768 | Medium | Excellent | High quality |

### FastEmbed Rust Usage
```rust
use fastembed::{TextEmbedding, InitOptions, EmbeddingModel};

let model = TextEmbedding::try_new(
    InitOptions::new(EmbeddingModel::AllMiniLML6V2)
        .with_show_download_progress(true)
)?;

let embeddings = model.embed(vec!["your text"], None)?;
```

**Quantized models available**: Append `Q` (e.g., `AllMiniLML6V2Q`) for 2x faster with ~1% quality loss.

---

## Document Processing Pipeline

### Docling-Inspired Architecture
1. **Detection**: Identify document regions (text, tables, figures, formulas)
2. **Extraction**: OCR for images, LaTeX for formulas
3. **Structuring**: Convert to markdown with hierarchy
4. **Chunking**: Semantic chunking (not fixed-size)

### Chunking Strategy
```
Target: 512 tokens per chunk
Overlap: 50 tokens
Method: Sentence-boundary aware
```

**Semantic chunking** (2025 best practice):
- Use embedding similarity to find natural break points
- Keep paragraphs together when semantically coherent
- Never split mid-sentence

### Rust PDF Processing
```rust
use lopdf::Document;

let doc = Document::load("file.pdf")?;
for page_num in doc.get_pages().keys() {
    let content = doc.extract_text(&[*page_num])?;
}
```

---

## ParadeDB pg_search Best Practices

### BM25 Index Configuration
```sql
-- Optimized for English content
CALL paradedb.create_bm25(
    index_name => 'docs_idx',
    table_name => 'documents',
    key_field => 'id',
    text_fields => 
        paradedb.field('title', boost => 2.0) ||
        paradedb.field('content', tokenizer => paradedb.tokenizer('en_stem'))
);
```

### Query Syntax
```sql
-- Basic search
SELECT * FROM documents WHERE id @@@ 'machine learning';

-- Phrase search
SELECT * FROM documents WHERE id @@@ '"neural network"';

-- Fuzzy search
SELECT * FROM documents WHERE id @@@ 'learninng~1';

-- Field-specific
SELECT * FROM documents WHERE id @@@ 'title:rust AND content:web';
```

### pgvector HNSW Tuning
```sql
-- For 100k-1M documents
CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Query-time parameter
SET hnsw.ef_search = 40;  -- Higher = more accurate, slower
```

---

## Leptos Full-Stack Architecture

### Component Structure
```
src/
├── app.rs           # Root component
├── components/
│   ├── search_bar.rs
│   ├── document_view.rs
│   └── chat_panel.rs
└── server/
    └── api.rs       # Server functions
```

### Server Functions Pattern
```rust
#[server(SearchDocuments)]
pub async fn search_documents(
    query: String,
    filters: SearchFilters,
) -> Result<Vec<Document>, ServerFnError> {
    use crate::db::hybrid_search;
    hybrid_search(&query, &filters).await
}
```

### Reactive Search
```rust
let (query, set_query) = create_signal(String::new());
let search_results = create_resource(
    move || query.get(),
    |q| async move { search_documents(q).await }
);
```

---

## LLM Integration

### OpenAI-Compatible API
```rust
let client = reqwest::Client::new();
let response = client
    .post(&format!("{}/chat/completions", config.llm_url))
    .header("Authorization", format!("Bearer {}", config.api_key))
    .json(&ChatRequest {
        model: "gpt-4",
        messages: vec![
            Message { role: "system", content: system_prompt },
            Message { role: "user", content: user_query },
        ],
    })
    .send()
    .await?;
```

### Context Window Management
| Model | Context | Recommended Chunks |
|-------|---------|-------------------|
| GPT-4 | 128k | Up to 20 chunks |
| Claude | 200k | Up to 30 chunks |
| Llama 3 | 8k | 3-5 chunks |

### Prompt Template
```
You are a helpful assistant answering questions based on the provided context.

CONTEXT:
{retrieved_chunks}

USER QUESTION:
{query}

Answer based ONLY on the context above. If the context doesn't contain the answer, say so.
```

---

## Configuration

### Environment Variables
```bash
# Database
DATABASE_URL=postgres://rag_user:rag_password@localhost:15432/rag_chat

# LLM Provider
LLM_PROVIDER=openai  # openai, anthropic, openrouter
LLM_API_URL=https://api.openai.com/v1
LLM_API_KEY=sk-...
LLM_MODEL=gpt-4

# Embedding
EMBEDDING_MODEL=all-MiniLM-L6-v2

# Indexer
WATCH_FOLDERS=/path/to/docs,/path/to/slides
WATCH_URLS=https://example.com/doc1.pdf
```

---

## Performance Benchmarks

### Expected Latencies (Mac M2)
| Operation | Time |
|-----------|------|
| Embed 1 chunk (384d) | ~5ms |
| BM25 search (100k docs) | ~10ms |
| Vector search (100k docs) | ~15ms |
| Hybrid search (RRF) | ~30ms |
| Full RAG response | 1-3s (LLM dependent) |

### Scaling Guidelines
- Up to 100k documents: Single PostgreSQL instance
- 100k-1M: Consider pgvector with IVF index
- 1M+: Evaluate Qdrant or dedicated vector DB

---

## Research References

1. **Hybrid Search**: "Reciprocal Rank Fusion outperforms Condorcet and individual Rank Learning Methods" (Cormack et al., 2009)
2. **BM25**: "A probabilistic model of information retrieval: development and comparative experiments" (Robertson & Walker, 2000)
3. **Late Chunking**: "Late Chunking: Contextual Chunk Embeddings Using Long-Context Embedding Models" (Jina AI, 2024)
4. **ParadeDB**: https://github.com/paradedb/paradedb
5. **FastEmbed**: https://github.com/Anush008/fastembed-rs

---

## Quick Start Commands

```bash
# Start database
docker compose up -d

# Run the app
cargo run

# Index a folder
cargo run -- index --path /path/to/docs

# Index a URL
cargo run -- index --url https://example.com/doc.pdf
```

# RAG Setup Guide: Docling + LM Studio on Mac M4 (32GB)

## 1. Start Docker Services (Docling + ParadeDB)

```bash
# Navigate to your project directory
cd /path/to/your/rag-project

# Start all Docker services
docker-compose up -d

# Verify services are running
docker-compose ps

# View Docling logs
docker-compose logs -f docling

# Wait for Docling to be ready (first start downloads ~2GB models)
# You'll see "Application startup complete" in logs
```

**First Run:** Docling downloads ~2GB of models on first startup. This happens once and is cached in the `docling_models` volume.

---

## 2. Install & Run LM Studio Locally (Mac M4)

LM Studio runs on your Mac host (not Docker) to use MLX acceleration with Apple Silicon.

### Installation:
```bash
# Download from official site
# https://lmstudio.ai

# Or install via Homebrew (if available)
brew install lm-studio
```

### Quick Setup:
1. **Open LM Studio**
2. Go to **Discover** tab
3. Search for and download one of these models for Mac M4 32GB:

**Recommended for Your Config:**
- **`Qwen2.5-7B-Instruct-GGUF`** (Q4_K_M) ⭐ **Best for long context RAG (32k)**
- `Llama-3.2-8B-Instruct-GGUF` (Q4_K_M) - Balanced, 8k context
- `Mixtral-8x7B-Instruct-GGUF` (Q4_K_M) - Best quality, 32k context

4. Go to **Developer** tab
5. Click **Start Server** (default port 1234)
6. Verify: `curl http://localhost:1234/v1/models` should return loaded model info

---

## 3. Test Docling API

```bash
# Check Docling health
curl http://localhost:5001/health

# Test with a sample document
curl -X POST "http://localhost:5001/v1/convert/source" \
  -H "Content-Type: application/json" \
  -d '{
    "sources": [{"kind": "http", "url": "https://arxiv.org/pdf/2408.09869"}],
    "options": {
      "do_ocr": true,
      "do_table_structure": true,
      "generate_picture_images": true
    }
  }' | jq .

# Or use Docling UI
# Open: http://localhost:5001/ui
```

---

## 4. Test LM Studio API

```bash
# Verify LM Studio is running
curl http://localhost:1234/v1/models

# Test chat completions
curl -X POST "http://localhost:1234/v1/chat/completions" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "local-model",
    "messages": [
      {"role": "system", "content": "You are a helpful assistant."},
      {"role": "user", "content": "What is RAG?"}
    ],
    "temperature": 0.7,
    "max_tokens": 512
  }' | jq .
```

---

## 5. Document Processing Workflow

### Option A: Via Docling UI
1. Open http://localhost:5001/ui
2. Upload PDF/PPTX/DOCX
3. Download processed markdown + JSON

### Option B: Via REST API (for automation)
```bash
# Single file
curl -X POST "http://localhost:5001/v1/convert/file" \
  -F "file=@presentation.pdf" | jq .

# Multiple URLs (async for large docs)
curl -X POST "http://localhost:5001/v1/convert/source/async" \
  -H "Content-Type: application/json" \
  -d '{
    "sources": [
      {"kind": "http", "url": "https://example.com/doc1.pdf"},
      {"kind": "http", "url": "https://example.com/doc2.pdf"}
    ]
  }' | jq .

# Poll job status
JOB_ID="<from-response>"
curl "http://localhost:5001/v1/jobs/{job_id}" | jq .
```

---

## 6. Database Schema for Document Storage

Add to your `./sql/init.sql`:

```sql
-- Documents table
CREATE TABLE documents (
  id SERIAL PRIMARY KEY,
  filename TEXT NOT NULL,
  source_url TEXT,
  processed_at TIMESTAMP DEFAULT NOW(),
  raw_markdown TEXT,
  metadata JSONB
);

-- Document chunks (for RAG)
CREATE TABLE document_chunks (
  id SERIAL PRIMARY KEY,
  document_id INT REFERENCES documents(id) ON DELETE CASCADE,
  chunk_text TEXT NOT NULL,
  chunk_index INT,
  embedding VECTOR(384),  -- Adjust dimension for your embedder
  created_at TIMESTAMP DEFAULT NOW()
);

-- Index for vector search
CREATE INDEX ON document_chunks USING ivfflat (embedding vector_cosine_ops)
  WITH (lists = 100);

-- Index for BM25 (full-text search)
CREATE INDEX ON document_chunks USING paradedb.inverted (chunk_text);
```

---

## 7. Mac M4 Performance Tips

**GPU/MLX Acceleration:**
- LM Studio uses Apple MLX automatically on M-series Macs
- In LM Studio Settings, ensure "GPU Acceleration" is enabled
- You should see "Neural Engine" or "MLX" in the model info

**Optimal Settings in LM Studio:**
```
Context Length: 8192 (balance speed vs context)
Temperature: 0.7 (creative) or 0.3 (factual)
Top P: 0.9
Repeat Penalty: 1.1
GPU Layers: Max (use all layers)
Batch Size: 1 (for stability)
```

**Memory Management:**
- ParadeDB + Docling: ~3-4GB (Docker containers)
- LM Studio (Qwen2.5-7B): ~6-8GB RAM + Neural Engine
- Total: ~10-12GB, leaving 20GB buffer
- Safe for your 32GB setup

**Expected Performance:**
- PDF processing (Docling): 2-3 seconds/page (CPU), 0.5s (MLX)
- Token generation (LM Studio): 20-40 tokens/sec (MLX on M4)
- Full RAG query: 3-5 seconds end-to-end

---

## 8. Useful Commands

```bash
# View all container logs
docker-compose logs -f

# Restart services
docker-compose restart

# Stop services
docker-compose down

# Remove volumes (caution: deletes data)
docker-compose down -v

# Monitor resource usage
docker stats

# Access ParadeDB directly
psql -h localhost -p 15432 -U rag_user -d rag_chat
```

---

## 9. Troubleshooting

**Docling container won't start:**
- Check logs: `docker-compose logs docling`
- May need to pull fresh image: `docker-compose pull`
- First run downloads models (~2GB) - takes 2-3 minutes

**LM Studio not responding:**
- Ensure it's actually running (check menu bar on Mac)
- Try: `curl http://localhost:1234/v1/models`
- Check LM Studio logs in the app

**Out of memory errors:**
- Reduce context length in LM Studio
- Use smaller model (Mistral-7B instead of Mixtral-8x7B)
- Check `docker stats` for container memory

**Docling API timeout:**
- Increase `DOCLING_SERVE_MAX_SYNC_WAIT` in compose file
- Use async endpoint for large documents

---
# UI Redesign Plan: 2-Column RAG Interface

## Objective
Redesign the current 3-column layout into a comprehensive 2-column interface that integrates advanced search, faceted filtering, and RAG-powered chat.

## Architecture

### Layout Structure
- **Container**: Full-screen flex/grid container.
- **Left Column (45%)**: "Discovery & Context"
    - **Header**: App Title & Branding.
    - **Search Section**: Advanced Search Bar + Syntax Help.
    - **Filters Section**: Collapsible/Accordion Facets (Categories, Keywords, Locations, Date).
    - **Results Section**: Scrollable list of search results with metadata and selection toggles.
- **Right Column (55%)**: "Synthesis & Chat"
    - **Header**: Chat Status & Context Indicator.
    - **Chat Area**: Message history (User & Assistant).
    - **Input Area**: Chat input + Context controls.

### Components

#### 1. Left Column: Discovery
- **Advanced Search Bar**:
    - Input field with placeholder showing syntax examples.
    - "Help" button toggling a tooltip with syntax guide (`~fuzzy`, `*prefix`, `phrase`, `AND/OR`).
    - Search weight sliders (BM25 vs Vector) tucked away in a "Settings" or "Advanced" toggle.
- **Faceted Filters**:
    - **Categories**: Dropdown or pill selection.
    - **Keywords**: Multi-select checkboxes with counts (e.g., "Machine Learning (12)").
    - **Locations**: Multi-select checkboxes.
    - **Date Range**: From/To date pickers.
    - **Logic**: Auto-apply filters on change.
- **Results List**:
    - Card-based layout for each result.
    - **Content**: Title, Relevance Score, Snippet, Metadata badges.
    - **Interaction**:
        - Click to expand/preview (optional).
        - Checkbox to "Select for Chat Context".
        - Visual highlight when selected.

#### 2. Right Column: Chat
- **Context Awareness**:
    - Visual indicator: "Using X documents for context" (linked to selection in Left Column).
    - Toggle: "Auto-select top results" vs "Manual selection".
- **Message Stream**:
    - User messages (Right aligned).
    - Assistant messages (Left aligned).
    - **Citations**: Footnotes or chips linking back to specific documents in the Left Column.
- **Input Area**:
    - Textarea for questions.
    - Send button.

## Technical Implementation

### Frontend (Alpine.js + Tailwind)
- **State Management (`ragApp`)**:
    - `searchQuery`: String
    - `filters`: Object (category, date, etc.)
    - `results`: Array of documents
    - `selectedContext`: Array of document IDs
    - `chatHistory`: Array of messages
- **Logic**:
    - `search()`: Calls `/api/search` with query + filters.
    - `updateContext()`: Syncs `selectedContext` based on "Auto" toggle or manual selection.
    - `sendChat()`: Sends message + `selectedContext` IDs to `/api/chat`.

### Backend (Rust/Axum)
- **Existing Endpoints**:
    - `/api/search`: Supports query + filters.
    - `/api/chat`: Supports `document_ids` for context.
    - `/api/categories`: For filter population.
- **Enhancements (Optional/Future)**:
    - `/api/aggregations`: To efficiently populate filter counts (currently done client-side).

## Execution Steps
1.  **Scaffold Layout**: Create the 2-column grid in `index.html`.
2.  **Migrate Search & Filters**: Move existing search/filter logic to the Left Column.
3.  **Migrate Results**: Move results list to the Left Column (below filters).
4.  **Enhance Chat**: Update Right Column to take up remaining space and improve context visualization.
5.  **Refine Styling**: Apply professional, data-centric styling (compact density, clear visual hierarchy).


---

## 10. Next Steps

1. ✅ Start Docker services: `docker-compose up -d`
2. ✅ Launch LM Studio and download model
3. ✅ Test APIs (Docling + LM Studio)
4. ✅ Upload test documents
5. ✅ Build RAG pipeline (retrieve + generate)
6. ✅ Implement hybrid search (BM25 + vector)
---
# Docling Performance Improvements

This document outlines the enhancements made to improve document indexing quality using Docling and Vision Language Models (VLMs).

## Overview of Changes

### 1. Enhanced Docling Configuration

**File**: [src/indexer.rs](src/indexer.rs:159-206)

#### Improvements:
- ✅ Enabled **OCR** (`do_ocr: true`) for better text extraction from scanned documents
- ✅ Enabled **table structure detection** (`do_table_structure: true`) for accurate table parsing
- ✅ Enabled **image extraction** (`generate_picture_images: true`) to capture figures and diagrams
- ✅ Increased **image resolution** (`images_scale: 2.0`) for better OCR accuracy
- ✅ Configured **OCR engine** (EasyOCR) for multilingual support

#### Configuration Options:
```rust
let options = json!({
    "do_ocr": true,
    "do_table_structure": true,
    "generate_picture_images": true,
    "generate_page_images": false,
    "images_scale": 2.0,
    "ocr_engine": "easyocr"
});
```

### 2. Enhanced Metadata Extraction

**File**: [src/indexer.rs](src/indexer.rs:225-306)

#### Previous Issues:
- ❌ `author` field always null
- ❌ `locations` field always null
- ❌ Limited entity extraction
- ❌ Only 4K characters analyzed

#### Improvements:
- ✅ Increased analysis window from 4K to **8K characters**
- ✅ Added **author extraction** from document metadata, headers, and signatures
- ✅ Enhanced **location extraction** (cities, countries, regions, addresses)
- ✅ Added **topics/categories** extraction
- ✅ Improved entity extraction comprehensiveness
- ✅ Better prompt engineering for structured output

#### New Prompt Features:
- Explicit author identification instructions
- Multi-word keyword phrases support
- Comprehensive entity types (persons, organizations, locations, dates)
- Topic/category classification
- 3-5 key questions the document answers

### 3. Database Field Population

**File**: [src/indexer.rs](src/indexer.rs:457-486)

#### Improvements:
- ✅ Extract and populate `author` field from LLM response
- ✅ Extract and populate `locations` array from entities
- ✅ Log extraction success for monitoring
- ✅ Store enhanced `entities` JSON with topics and questions

### 4. Comprehensive Testing

**File**: [tests/indexing_pipeline_test.rs](tests/indexing_pipeline_test.rs:429-616)

#### New Tests:

**Test 1: Enhanced Metadata Quality** (`test_enhanced_metadata_extraction_quality`)
- Verifies summary quality (>50 chars)
- Verifies keyword extraction (≥5 keywords)
- Checks author field population
- Checks locations field population
- Validates complete entity structure (persons, organizations, locations, dates, questions, topics)
- Verifies word count calculation

**Test 2: Docling OCR Quality** (`test_docling_enhanced_ocr_quality`)
- Tests enhanced OCR with high-resolution settings
- Verifies table structure preservation
- Checks content extraction quality (>1000 chars, >10 lines)
- Validates Docling configuration options

## Using Vision Language Models (VLMs)

### Recommended VLMs for Docling:

Based on research, these VLMs integrate well with Docling:

1. **SmolDocling** (256M parameters)
   - Fast: 0.35s per page on A100 GPU
   - Compact and efficient
   - Good for consumer GPUs

2. **Granite-Docling-258M** (IBM)
   - High-fidelity document conversion
   - Excellent table/formula handling
   - Apache 2.0 licensed

3. **Qwen2.5-VL** (Alibaba)
   - State-of-the-art vision-language model
   - Native resolution processing
   - Advanced visual perception

### Serving VLMs with VLLM:

```bash
# Serve Granite-Docling with VLLM
vllm serve ibm-granite/granite-docling-258M --revision untied

# Configure Docling to use VLLM endpoint
# In your environment or docker-compose:
export DOCLING_VLM_URL=http://localhost:8000
```

### Configuring Docling with VLM:

To use VLM instead of traditional OCR, you can configure Docling:

```python
# Example Docling VLM configuration
pipeline_options = VlmPipelineOptions(
    enable_remote_services=True,
    vlm_options=ApiVlmOptions(
        model="ibm-granite/granite-docling-258M",
        endpoint="http://localhost:8000"
    )
)
```

## Performance Improvements

### Before:
- Basic OCR with default settings
- Limited metadata extraction (4K chars)
- Many null fields (author, locations)
- Missing topic classification

### After:
- Enhanced OCR with 2x resolution
- Table structure preservation
- Image extraction enabled
- Comprehensive metadata (8K chars analyzed)
- Author and location extraction
- Topic/category classification
- Richer entity extraction

## Running Tests

```bash
# Run all indexing tests
cargo test --test indexing_pipeline_test

# Run specific enhanced tests
cargo test test_enhanced_metadata_extraction_quality
cargo test test_docling_enhanced_ocr_quality
```

## Docker Compose Configuration

To enable VLM support, update your `docker-compose.yml`:

```yaml
services:
  docling:
    image: docling/docling-serve:latest
    environment:
      - DOCLING_VLM_ENABLED=true
      - DOCLING_VLM_MODEL=ibm-granite/granite-docling-258M
    volumes:
      - ./models:/models  # Cache model weights
    ports:
      - "5001:5001"
```

## Future Enhancements

1. **VLM Integration**: Fully integrate vision models for end-to-end document understanding
2. **Image Captioning**: Use VLMs to generate descriptions for extracted images
3. **Chart/Graph Understanding**: Extract data and insights from visualizations
4. **Formula OCR**: Better math equation extraction with specialized models
5. **Multi-modal Embeddings**: Combine text and image embeddings for richer search

## References

- [Docling Documentation](https://docling-project.github.io/docling/)
- [SmolDocling](https://medium.com/@speaktoharisudhan/smoldocling-a-compact-vision-language-model-c54795474faf)
- [Granite-Docling](https://www.ibm.com/new/announcements/granite-docling-end-to-end-document-conversion)
- [VLLM Documentation](https://docs.vllm.ai/)
---
# Test Suite Improvements

## Problem

The original indexing pipeline tests were:
- **Extremely slow** (40+ seconds for metadata extraction)
- **Failing** due to LLM JSON parsing issues
- **Tightly coupled** - testing multiple components together
- **Hard to debug** - failures could be from any component

## Solution

Created focused, isolated tests for each pipeline component:

### New Test Files

1. **[chunking_test.rs](tests/chunking_test.rs)** - Pure logic testing (no I/O)
   - Validates text splitting and chunk sizes
   - Tests run in < 0.1 seconds
   - No external dependencies

2. **[embedding_test.rs](tests/embedding_test.rs)** - Embedding generation
   - Fast: ~2 seconds per embedding
   - Tests similarity, determinism, and batch performance
   - Uses actual embedding service

3. **[llm_enrichment_test.rs](tests/llm_enrichment_test.rs)** - Simple metadata extraction
   - Focused on keywords and entities only
   - No complex JSON schemas
   - Clearer error messages

4. **[docling_service_test.rs](tests/docling_service_test.rs)** - PDF parsing
   - Tests Docling speed and capabilities
   - Verifies table detection
   - Checks metadata extraction

5. **[document_storage_test.rs](tests/document_storage_test.rs)** - Database operations
   - CRUD operations
   - Upsert functionality
   - No business logic coupling

## Benefits

### Speed
- Component tests run in seconds, not minutes
- Faster feedback during development
- Can run specific tests without the full suite

### Clarity
- Each test has a single purpose
- Failures point to specific components
- Test names describe what they verify

### Maintainability
- Easy to add new tests
- Simple to update when requirements change
- Clear test organization with [README](tests/README.md)

## Running Tests

```bash
# Fast component tests
cargo test --test chunking_test         # < 1 second
cargo test --test embedding_test        # ~10 seconds
cargo test --test llm_enrichment_test   # ~15 seconds

# Integration tests (slower)
cargo test --test indexing_pipeline_test

# All tests
cargo test
```

## Next Steps

1. **Simplify metadata extraction** in the main pipeline
   - Use the simple keyword/entity approach from tests
   - Remove complex JSON schemas
   - Add better error handling

2. **Add performance benchmarks**
   - Track embedding speed over time
   - Monitor Docling parsing performance
   - Set SLA targets for each component

3. **Refactor indexing_pipeline_test.rs**
   - Split into focused integration tests
   - Use the new component tests as building blocks
   - Keep only essential end-to-end tests
---
# Test Suite

Focused, isolated tests for each component of the RAG pipeline.

## Test Files

### Component Tests (Isolated)

- **[chunking_test.rs](chunking_test.rs)** - Text splitting and chunking logic
  - Validates chunk sizes
  - Tests content preservation
  - Edge case handling

- **[embedding_test.rs](embedding_test.rs)** - Embedding generation
  - Basic embedding generation
  - Semantic similarity validation
  - Determinism checks
  - Performance benchmarks

- **[llm_enrichment_test.rs](llm_enrichment_test.rs)** - LLM metadata extraction
  - Simple keyword extraction
  - Named entity recognition
  - Minimal content handling

- **[docling_service_test.rs](docling_service_test.rs)** - Docling PDF parsing
  - Parsing speed verification
  - Table detection
  - Metadata extraction from PDFs

- **[document_storage_test.rs](document_storage_test.rs)** - Database operations
  - Document CRUD operations
  - Upsert functionality

### Integration Tests

- **[db_pool_test.rs](db_pool_test.rs)** - Database connection pooling
- **[api_test.rs](api_test.rs)** - API endpoint testing
- **[integration_test.rs](integration_test.rs)** - End-to-end workflows

### Legacy Tests

- **[indexing_pipeline_test.rs](indexing_pipeline_test.rs)** - Original complex integration tests (to be refactored)

## Running Tests

```bash
# Run all tests
cargo test

# Run specific component tests
cargo test --test chunking_test
cargo test --test embedding_test
cargo test --test llm_enrichment_test
cargo test --test docling_service_test
cargo test --test document_storage_test

# Run with output
cargo test --test embedding_test -- --nocapture
```

## Test Configuration

Tests use [test.env](test.env) for configuration. Key settings:

- `DATABASE_URL` - PostgreSQL connection
- `DOCLING_URL` - Docling service endpoint
- `LLM_API_URL` - LLM API endpoint (LM Studio)
- `LLM_MODEL` - Model for metadata extraction
- `EMBEDDING_MODEL` - Model for embeddings
- `EMBEDDING_DIMENSIONS` - Expected embedding vector size

## Test Philosophy

1. **Isolated** - Each test file focuses on one component
2. **Fast** - Unit tests complete in seconds, not minutes
3. **Clear** - Test names describe what they verify
4. **Focused** - Tests validate one thing at a time


---

# Enhanced Metadata Extraction & Typed NER (January 2026)

## Problem Statement

Initial document indexing produced poor quality metadata:
- **Keywords duplicated entities**: "Pareto Principle" appeared in both keywords AND entities
- **Flat entity structure**: All entities in a single `names` array with no type classification
- **Missing metadata**: `author` and `locations` fields always null
- **No semantic separation**: Keywords were proper nouns instead of thematic topics
- **Poor search relevance**: Unable to filter by entity type or distinguish concepts from organizations

### Example of Poor Indexing:
```json
{
  "keywords": ["Pareto Principle", "Solomon's Paradox", "Grice's Razor"],
  "entities": {"names": ["Pareto Principle", "Solomon's Paradox"]},
  "author": null,
  "locations": null
}
```

## Solution: Typed Entity Extraction with LLM-based NER

Implemented a comprehensive metadata extraction pipeline with typed entities and DocLing metadata integration.

### Key Changes

#### 1. Enhanced LLM Prompt for Typed Entities

**File**: [src/indexer.rs:318-431](src/indexer.rs#L318-L431)

**New Prompt Structure:**
```
Extract:
1. Keywords: 5-8 TOPICS/THEMES (not entity names)
2. Entities (categorized):
   - persons: Real people, authors
   - organizations: Companies, institutions, teams
   - locations: Geographic places
   - products: Software, tools, frameworks
   - concepts: Principles, laws, theories

Rules:
- Keywords = thematic topics (e.g., "security", "performance")
- Entities = proper nouns only
- No duplication between keywords and entities
- Empty arrays OK for missing entity types
```

**Result:**
```json
{
  "keywords": ["productivity", "decision-making", "psychology"],
  "entities": {
    "persons": [],
    "organizations": [],
    "locations": [],
    "products": [],
    "concepts": ["Pareto Principle", "Solomon's Paradox", "Occam's Razor"]
  }
}
```

#### 2. DocLing Metadata Integration

**File**: [src/indexer.rs:234-284](src/indexer.rs#L234-L284)

**Changes:**
- Modified `convert_file()` to return `(String, serde_json::Value)` tuple
- Extracts document metadata from DocLing response
- Passes metadata through to entity extraction
- Auto-populates `author` field from PDF/DOCX properties

**DocLing Metadata Fields:**
- `title` - Document title
- `author` / `authors` - Creator information
- `creation_date` - When document was created
- `page_count` - Number of pages
- `language` - Document language

#### 3. Function Signature Updates

**extract_metadata()** - [src/indexer.rs:319-412](src/indexer.rs#L319-L412)
```rust
// OLD
pub async fn extract_metadata(content: &str)
  -> Result<(String, Vec<String>, serde_json::Value)>

// NEW
pub async fn extract_metadata(
    content: &str,
    docling_meta: Option<&serde_json::Value>
) -> Result<(String, Vec<String>, serde_json::Value, Option<String>)>
//           ^summary  ^keywords   ^typed entities       ^author
```

**insert_document()** - [src/db.rs:282-325](src/db.rs#L282-L325)
```rust
// Added author parameter
pub async fn insert_document(
    // ... existing params
    author: Option<&str>,  // NEW
) -> Result<Uuid>
```

#### 4. Database Schema Impact

**No schema changes required** - Existing schema already supports:
```sql
CREATE TABLE documents (
  -- ... other fields
  author TEXT,           -- Now populated from DocLing or LLM
  entities JSONB,        -- Now structured with types
  keywords TEXT[],       -- Now thematic topics only
  locations TEXT[]       -- Extracted from entities.locations
);
```

### Implementation Flow

```
Document (PDF/DOCX/etc.)
    ↓
[DocLing Conversion] → (markdown, metadata)
    ↓
[LLM Extraction with metadata context]
    ↓
{
  keywords: ["topic1", "topic2"],
  entities: {
    persons: [...],
    organizations: [...],
    locations: [...],
    products: [...],
    concepts: [...]
  }
}
    ↓
[Database Storage]
- author from DocLing metadata
- keywords as TEXT[]
- entities as structured JSONB
- locations extracted from entities.locations
```

## Test Suite

**File**: [tests/llm_enrichment_test.rs](tests/llm_enrichment_test.rs)

### New Test Cases:

1. **test_typed_entity_extraction_tech_document** - Validates:
   - Organizations: AWS, Netflix, CNCF
   - Products: Kubernetes, Prometheus
   - Persons: Martin Fowler
   - Keywords are topics, not entity names

2. **test_typed_entity_extraction_principles_document** - Validates:
   - Concepts: Pareto Principle, Solomon's Paradox, Occam's Razor
   - Keywords: "decision-making", "psychology", "management"
   - No concept names in keywords

3. **test_typed_entity_extraction_platform_engineering** - Validates:
   - Organizations: ACME Financial One
   - Products: Docker, Kubernetes
   - Locations: San Francisco, London
   - Concepts: Golden Path methodology

4. **test_typed_entity_no_duplication** - Validates:
   - No overlap between keywords and entities
   - Proper classification of proper nouns vs topics

### Running Tests:
```bash
# All new entity extraction tests
cargo test test_typed_entity -- --nocapture

# Specific test
cargo test test_typed_entity_extraction_tech_document -- --nocapture
```

## Benefits & Impact

### 1. Better Search Relevance
**Before**: Searching for "Pareto Principle" would match both keywords AND entities
**After**: "Pareto Principle" only in `entities.concepts`, keywords are thematic

### 2. Structured Filtering
```sql
-- Find documents by organization
SELECT * FROM documents
WHERE entities->'organizations' ? 'Netflix';

-- Find documents by concept
SELECT * FROM documents
WHERE entities->'concepts' ? 'Pareto Principle';

-- Find documents by location
SELECT * FROM documents
WHERE entities->'locations' ? 'San Francisco';
```

### 3. Faceted Search UI
Enable UI filters like:
- 👤 **People**: Martin Fowler, Einstein
- 🏢 **Organizations**: Netflix, ACME Corp, MIT
- 📍 **Locations**: San Francisco, London
- 📦 **Technologies**: Kubernetes, PostgreSQL, Docker
- 💡 **Concepts**: Pareto Principle, Occam's Razor

### 4. Author Metadata
Documents now have author information extracted from:
- PDF metadata (via DocLing)
- DOCX properties (via DocLing)
- Document headers/signatures (via LLM)

### 5. Analytics & Insights
```sql
-- Most mentioned organizations
SELECT org, COUNT(*)
FROM documents, jsonb_array_elements_text(entities->'organizations') org
GROUP BY org
ORDER BY COUNT(*) DESC;

-- Technology stack across documents
SELECT product, COUNT(*)
FROM documents, jsonb_array_elements_text(entities->'products') product
GROUP BY product;

-- Geographic distribution
SELECT location, COUNT(*)
FROM documents, jsonb_array_elements_text(entities->'locations') location
GROUP BY location;
```

## Migration Guide

### For Existing Data

If you have existing documents with old entity format:

```sql
-- Check current entity structure
SELECT id, entities FROM documents LIMIT 5;

-- Re-index existing documents
-- Option 1: Via Rust
cargo run -- index --path /path/to/documents

-- Option 2: Direct SQL migration (if needed)
-- This is complex - recommend re-indexing instead
```

### For New Implementations

1. **Ensure LLM supports structured output**
   - Works with: GPT-4, Claude, Qwen, DeepSeek
   - Requires: JSON mode or good instruction following

2. **Configure metadata model** (optional)
   ```bash
   # Use faster model for metadata extraction
   export METADATA_LLM_MODEL="qwen2.5-7b-instruct"
   export METADATA_MAX_TOKENS=500
   ```

3. **Test with sample document**
   ```bash
   cargo test test_typed_entity_extraction_tech_document -- --nocapture
   ```

## Configuration

### Environment Variables

```bash
# LLM for metadata extraction (optional - falls back to LLM_MODEL)
METADATA_LLM_MODEL=qwen2.5-7b-instruct

# Max tokens for metadata response (default: 300)
METADATA_MAX_TOKENS=500

# DocLing service URL
DOCLING_URL=http://localhost:5001

# LLM API (for entity extraction)
LLM_API_URL=http://localhost:1234/v1
LLM_MODEL=qwen2.5-7b-instruct
```

## Performance

**Metadata Extraction:**
- Processing time: ~2-5 seconds per document
- LLM tokens: ~200-400 tokens per response
- Memory: Minimal (4K chars analyzed)

**No Performance Regression:**
- Same speed as before (LLM call still single-pass)
- Structured JSON output is same size
- Database storage unchanged (JSONB handles nesting)

## Future Enhancements

1. **Entity Relationships**: Extract relationships between entities
   - "Netflix uses AWS"
   - "Martin Fowler works for ThoughtWorks"

2. **Temporal Entities**: Extract dates and events
   - "Launched in 2015"
   - "Conference on June 2024"

3. **Hierarchical Concepts**: Build concept taxonomies
   - "Pareto Principle" → "Productivity" → "Management"

4. **Entity Disambiguation**: Link entities to knowledge bases
   - "Apple" (company) vs "Apple" (fruit)

5. **Multi-language NER**: Extract entities from non-English documents

## Troubleshooting

### Issue: Entities not properly categorized

**Symptom**: All entities in `concepts`, none in `organizations`

**Solution**:
- Check LLM model supports structured output
- Increase `METADATA_MAX_TOKENS` to 500
- Use reasoning model for better classification

### Issue: Keywords still contain entity names

**Symptom**: "PostgreSQL" appears in keywords

**Solution**:
- Review LLM prompt clarity
- Add examples to system prompt
- Use temperature=0.1 for more consistent output

### Issue: Author field always null

**Symptom**: No author extracted

**Solution**:
- Check DocLing metadata: `curl http://localhost:5001/...`
- Verify PDF has author metadata
- LLM fallback should extract from content

### Issue: Entity extraction fails with JSON parse error

**Symptom**: "Failed to parse LLM JSON response"

**Solution**:
- Check LLM model compatibility
- Inspect raw response: enable `RUST_LOG=debug`
- Handles reasoning models (`<think>` tags), markdown blocks

## Code References

**Key Files Modified:**
- [src/indexer.rs:318-431](src/indexer.rs#L318-L431) - Enhanced metadata extraction
- [src/indexer.rs:234-284](src/indexer.rs#L234-L284) - DocLing metadata extraction
- [src/db.rs:282-325](src/db.rs#L282-L325) - Database insertion with author
- [tests/llm_enrichment_test.rs](tests/llm_enrichment_test.rs) - Comprehensive tests

**Database Schema:**
- [sql/init.sql](sql/init.sql) - No changes required (existing schema sufficient)


---

# Enhanced Metadata Extraction & Typed NER (January 2026)

## Problem Statement

Initial document indexing produced poor quality metadata:
- **Keywords duplicated entities**: "Pareto Principle" appeared in both keywords AND entities
- **Flat entity structure**: All entities in a single `names` array with no type classification
- **Missing metadata**: `author` and `locations` fields always null
- **No semantic separation**: Keywords were proper nouns instead of thematic topics
- **Poor search relevance**: Unable to filter by entity type or distinguish concepts from organizations

### Example of Poor Indexing:
```json
{
  "keywords": ["Pareto Principle", "Solomon's Paradox", "Grice's Razor"],
  "entities": {"names": ["Pareto Principle", "Solomon's Paradox"]},
  "author": null,
  "locations": null
}
```

## Solution: Typed Entity Extraction with LLM-based NER

Implemented a comprehensive metadata extraction pipeline with typed entities and DocLing metadata integration.

### Key Changes

#### 1. Enhanced LLM Prompt for Typed Entities

**File**: [src/indexer.rs:318-431](src/indexer.rs#L318-L431)

**New Prompt Structure:**
```
Extract:
1. Keywords: 5-8 TOPICS/THEMES (not entity names)
2. Entities (categorized):
   - persons: Real people, authors
   - organizations: Companies, institutions, teams
   - locations: Geographic places
   - products: Software, tools, frameworks
   - concepts: Principles, laws, theories

Rules:
- Keywords = thematic topics (e.g., "security", "performance")
- Entities = proper nouns only
- No duplication between keywords and entities
- Empty arrays OK for missing entity types
```

**Result:**
```json
{
  "keywords": ["productivity", "decision-making", "psychology"],
  "entities": {
    "persons": [],
    "organizations": [],
    "locations": [],
    "products": [],
    "concepts": ["Pareto Principle", "Solomon's Paradox", "Occam's Razor"]
  }
}
```

#### 2. DocLing Metadata Integration

**File**: [src/indexer.rs:234-284](src/indexer.rs#L234-L284)

**Changes:**
- Modified `convert_file()` to return `(String, serde_json::Value)` tuple
- Extracts document metadata from DocLing response
- Passes metadata through to entity extraction
- Auto-populates `author` field from PDF/DOCX properties

**DocLing Metadata Fields:**
- `title` - Document title
- `author` / `authors` - Creator information
- `creation_date` - When document was created
- `page_count` - Number of pages
- `language` - Document language

#### 3. Function Signature Updates

**extract_metadata()** - [src/indexer.rs:319-412](src/indexer.rs#L319-L412)
```rust
// OLD
pub async fn extract_metadata(content: &str)
  -> Result<(String, Vec<String>, serde_json::Value)>

// NEW
pub async fn extract_metadata(
    content: &str,
    docling_meta: Option<&serde_json::Value>
) -> Result<(String, Vec<String>, serde_json::Value, Option<String>)>
//           ^summary  ^keywords   ^typed entities       ^author
```

**insert_document()** - [src/db.rs:282-325](src/db.rs#L282-L325)
```rust
// Added author parameter
pub async fn insert_document(
    // ... existing params
    author: Option<&str>,  // NEW
) -> Result<Uuid>
```

#### 4. Database Schema Impact

**No schema changes required** - Existing schema already supports:
```sql
CREATE TABLE documents (
  -- ... other fields
  author TEXT,           -- Now populated from DocLing or LLM
  entities JSONB,        -- Now structured with types
  keywords TEXT[],       -- Now thematic topics only
  locations TEXT[]       -- Extracted from entities.locations
);
```

### Implementation Flow

```
Document (PDF/DOCX/etc.)
    ↓
[DocLing Conversion] → (markdown, metadata)
    ↓
[LLM Extraction with metadata context]
    ↓
{
  keywords: ["topic1", "topic2"],
  entities: {
    persons: [...],
    organizations: [...],
    locations: [...],
    products: [...],
    concepts: [...]
  }
}
    ↓
[Database Storage]
- author from DocLing metadata
- keywords as TEXT[]
- entities as structured JSONB
- locations extracted from entities.locations
```

## Test Suite

**File**: [tests/llm_enrichment_test.rs](tests/llm_enrichment_test.rs)

### New Test Cases:

1. **test_typed_entity_extraction_tech_document** - Validates:
   - Organizations: AWS, Netflix, CNCF
   - Products: Kubernetes, Prometheus
   - Persons: Martin Fowler
   - Keywords are topics, not entity names

2. **test_typed_entity_extraction_principles_document** - Validates:
   - Concepts: Pareto Principle, Solomon's Paradox, Occam's Razor
   - Keywords: "decision-making", "psychology", "management"
   - No concept names in keywords

3. **test_typed_entity_extraction_platform_engineering** - Validates:
   - Organizations: ACME Financial One
   - Products: Docker, Kubernetes
   - Locations: San Francisco, London
   - Concepts: Golden Path methodology

4. **test_typed_entity_no_duplication** - Validates:
   - No overlap between keywords and entities
   - Proper classification of proper nouns vs topics

### Running Tests:
```bash
# All new entity extraction tests
cargo test test_typed_entity -- --nocapture

# Specific test
cargo test test_typed_entity_extraction_tech_document -- --nocapture
```

## Benefits & Impact

### 1. Better Search Relevance
**Before**: Searching for "Pareto Principle" would match both keywords AND entities
**After**: "Pareto Principle" only in `entities.concepts`, keywords are thematic

### 2. Structured Filtering
```sql
-- Find documents by organization
SELECT * FROM documents
WHERE entities->'organizations' ? 'Netflix';

-- Find documents by concept
SELECT * FROM documents
WHERE entities->'concepts' ? 'Pareto Principle';

-- Find documents by location
SELECT * FROM documents
WHERE entities->'locations' ? 'San Francisco';
```

### 3. Faceted Search UI
Enable UI filters like:
- 👤 **People**: Martin Fowler, Einstein
- 🏢 **Organizations**: Netflix, ACME Corp, MIT
- 📍 **Locations**: San Francisco, London
- 📦 **Technologies**: Kubernetes, PostgreSQL, Docker
- 💡 **Concepts**: Pareto Principle, Occam's Razor

### 4. Author Metadata
Documents now have author information extracted from:
- PDF metadata (via DocLing)
- DOCX properties (via DocLing)
- Document headers/signatures (via LLM)

### 5. Analytics & Insights
```sql
-- Most mentioned organizations
SELECT org, COUNT(*)
FROM documents, jsonb_array_elements_text(entities->'organizations') org
GROUP BY org
ORDER BY COUNT(*) DESC;

-- Technology stack across documents
SELECT product, COUNT(*)
FROM documents, jsonb_array_elements_text(entities->'products') product
GROUP BY product;

-- Geographic distribution
SELECT location, COUNT(*)
FROM documents, jsonb_array_elements_text(entities->'locations') location
GROUP BY location;
```

## Migration Guide

### For Existing Data

If you have existing documents with old entity format:

```sql
-- Check current entity structure
SELECT id, entities FROM documents LIMIT 5;

-- Re-index existing documents
-- Option 1: Via Rust
cargo run -- index --path /path/to/documents

-- Option 2: Direct SQL migration (if needed)
-- This is complex - recommend re-indexing instead
```

### For New Implementations

1. **Ensure LLM supports structured output**
   - Works with: GPT-4, Claude, Qwen, DeepSeek
   - Requires: JSON mode or good instruction following

2. **Configure metadata model** (optional)
   ```bash
   # Use faster model for metadata extraction
   export METADATA_LLM_MODEL="qwen2.5-7b-instruct"
   export METADATA_MAX_TOKENS=500
   ```

3. **Test with sample document**
   ```bash
   cargo test test_typed_entity_extraction_tech_document -- --nocapture
   ```

## Configuration

### Environment Variables

```bash
# LLM for metadata extraction (optional - falls back to LLM_MODEL)
METADATA_LLM_MODEL=qwen2.5-7b-instruct

# Max tokens for metadata response (default: 300)
METADATA_MAX_TOKENS=500

# DocLing service URL
DOCLING_URL=http://localhost:5001

# LLM API (for entity extraction)
LLM_API_URL=http://localhost:1234/v1
LLM_MODEL=qwen2.5-7b-instruct
```

## Performance

**Metadata Extraction:**
- Processing time: ~2-5 seconds per document
- LLM tokens: ~200-400 tokens per response
- Memory: Minimal (4K chars analyzed)

**No Performance Regression:**
- Same speed as before (LLM call still single-pass)
- Structured JSON output is same size
- Database storage unchanged (JSONB handles nesting)

## Future Enhancements

1. **Entity Relationships**: Extract relationships between entities
   - "Netflix uses AWS"
   - "Martin Fowler works for ThoughtWorks"

2. **Temporal Entities**: Extract dates and events
   - "Launched in 2015"
   - "Conference on June 2024"

3. **Hierarchical Concepts**: Build concept taxonomies
   - "Pareto Principle" → "Productivity" → "Management"

4. **Entity Disambiguation**: Link entities to knowledge bases
   - "Apple" (company) vs "Apple" (fruit)

5. **Multi-language NER**: Extract entities from non-English documents

## Troubleshooting

### Issue: Entities not properly categorized

**Symptom**: All entities in `concepts`, none in `organizations`

**Solution**:
- Check LLM model supports structured output
- Increase `METADATA_MAX_TOKENS` to 500
- Use reasoning model for better classification

### Issue: Keywords still contain entity names

**Symptom**: "PostgreSQL" appears in keywords

**Solution**:
- Review LLM prompt clarity
- Add examples to system prompt
- Use temperature=0.1 for more consistent output

### Issue: Author field always null

**Symptom**: No author extracted

**Solution**:
- Check DocLing metadata: `curl http://localhost:5001/...`
- Verify PDF has author metadata
- LLM fallback should extract from content

### Issue: Entity extraction fails with JSON parse error

**Symptom**: "Failed to parse LLM JSON response"

**Solution**:
- Check LLM model compatibility
- Inspect raw response: enable `RUST_LOG=debug`
- Handles reasoning models (`<think>` tags), markdown blocks

## Code References

**Key Files Modified:**
- [src/indexer.rs:318-431](src/indexer.rs#L318-L431) - Enhanced metadata extraction
- [src/indexer.rs:234-284](src/indexer.rs#L234-L284) - DocLing metadata extraction
- [src/db.rs:282-325](src/db.rs#L282-L325) - Database insertion with author
- [tests/llm_enrichment_test.rs](tests/llm_enrichment_test.rs) - Comprehensive tests

**Database Schema:**
- [sql/init.sql](sql/init.sql) - No changes required (existing schema sufficient)

---
Happy RAG building! 🚀