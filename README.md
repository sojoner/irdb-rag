# RAG Chat - Hybrid Search Document Chat System

A high-performance, full-stack Rust RAG (Retrieval Augmented Generation) system utilizing **Leptos** for a reactive UI and **Axum** for the backend. It leverages **ParadeDB** for advanced hybrid search capabilities, combining BM25 keyword search with semantic vector search.

## 🏗 Architecture

The system follows a modular, layered architecture with a specialized pipeline for document processing:

```mermaid
graph TD
    Client[Leptos Web UI] <--> API[Axum API Server]
    
    subgraph "Backend Services"
        API <--> Search[Search Service]
        API <--> Indexer[Indexing Service]
        
        Indexer --> Enricher[Enrichment Pipeline]
        Enricher -- Extract/Summary/Entities --> LLM[LLM API]
        Enricher -- Generate Vectors --> Embed[Embedding API]
    end
    
    Search <--> DB[(ParadeDB)]
    Indexer --> DB
    
    subgraph "Database"
        DB -- pg_search --> BM25[Keyword Index]
        DB -- pgvector --> Vector[Semantic Index]
    end
```

- **Frontend**: Leptos 0.8 (SSR + Hydration) for a reactive, type-safe web interface.
- **Backend**: Axum 0.8 providing a robust REST API and server-side rendering capabilities.
- **Database**: **PostgreSQL** (Custom Image) serving as the "Brain", utilizing:
  - `pg_search` for BM25 keyword search.
  - `pgvector` for semantic vector search.
  - Reciprocal Rank Fusion (RRF) to combine results.
- **AI Integration**:
  - **Embeddings**: Connects to any OpenAI-compatible embedding API (e.g., LM Studio, OpenRouter, OpenAI).
  - **LLM**: Connects to any OpenAI-compatible chat completion API for generating responses.

### Database Schema

The core database structure is defined in [`sql/init.sql`](./sql/init.sql). It includes:

- **`documents`**: Stores document metadata and full content.
- **`document_chunks`**: Stores text chunks and their vector embeddings.
- **`import_jobs` / `import_items`**: Tracks batch import operations, progress, and error states.
- **`hybrid_search`**: A custom PostgreSQL function implementing Reciprocal Rank Fusion (RRF) to combine BM25 and Vector search results.

## ✨ Features & Pipeline

The system employs a sophisticated **5-Stage Indexing Pipeline** to ensure high-quality retrieval:

1.  **Extract & Enrich**:
    - Extracts text using **Docling** (optional) or standard parsers.
    - Uses an LLM to generate a **Summary**, extract **Keywords**, and identify **Named Entities** (Persons, Orgs, Locations).
    - Classifies documents into **Wikipedia Categories**.
2.  **Chunking**: Smartly splits content into manageable chunks (default 512 tokens).
3.  **Context Enrichment**: Prepends document metadata (Title, Summary, Keywords, Questions) to *every chunk*. This "Contextual Chunking" significantly improves retrieval accuracy by making chunks self-contained.
4.  **Embedding**: Generates vector embeddings for all enriched chunks in parallel.
5.  **Storage**: Saves structured metadata and vectors to ParadeDB for hybrid search.

### Batch Import & Job Management

For large-scale ingestion, the system uses a robust **Job Management System**:

- **Resilience**: Automatic retry with exponential backoff for transient errors (e.g., API timeouts).
- **Error Handling**: Distinguishes between transient (retryable) and permanent (skippable) errors.
- **Tracking**: Detailed progress tracking at both Job and Item levels (processed, failed, skipped).
- **Background Processing**: Asynchronous workers handle imports without blocking the main API.

## 🚀 Quick Start

### 1. Prerequisites

- **Rust**: [Install Rust](https://rustup.rs/)
- **Docker**: Required for the database.
- **AI Provider**: Access to an OpenAI-compatible API for embeddings and chat (e.g., running [LM Studio](https://lmstudio.ai/) locally or using OpenRouter/OpenAI).

### 2. Start the Database

```bash
# Start the database (Custom PostgreSQL with pgvector + pg_search)
docker compose up -d

# Wait for the database to be ready
docker compose ps
```

### 3. Configuration

Configuration is managed via TOML files in the `config/` directory:

- `config/default.toml` - Base configuration
- `config/production.toml` - Production overrides
- `config/test.toml` - Test environment overrides

Edit `config/production.toml` to configure your AI provider endpoints. For example, if using LM Studio locally:

```toml
[llm.chat]
provider = "openai"
api_url = "http://localhost:1234/v1"
api_key = "lm-studio"
model = "qwen2.5-7b-instruct"

[embedding]
provider = "openai"
api_url = "http://localhost:1234/v1"
api_key = "lm-studio"
model = "text-embedding-nomic-embed-text-v1.5"
dimensions = 768
```

Environment variables can override config values using the pattern `APP_<SECTION>__<KEY>`. For example:

```bash
export APP_LLM__CHAT__API_KEY=your-api-key
export APP_DATABASE__URL=postgres://user:pass@host/db
```

### 4. Run the Application

```bash
# Run the server
cargo run -- serve
```

Access the Web UI at **http://localhost:3000**.

## 📖 Usage

### Indexing Documents

The system provides a CLI for indexing documents from various sources.

**Index a local directory:**
```bash
cargo run -- index --path ./documents
```

**Index a specific file:**
```bash
cargo run -- index --path ./documents/report.pdf
```

**Index a URL:**
```bash
cargo run -- index --url https://example.com/whitepaper.pdf
```

**Watch a folder for changes (Real-time Indexing):**
```bash
cargo run -- watch --folders ./documents
```

### API Examples

You can interact directly with the API using `curl`.

**Hybrid Search:**
```bash
curl -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "platform engineering",
    "limit": 5,
    "filters": {
      "keywords": ["kubernetes"]
    }
  }'
```

**Chat with Context:**
```bash
curl -X POST http://localhost:3000/api/chat \
  -H "Content-Type: application/json" \
  -d '{
    "messages": [
      { "role": "user", "content": "What are the key pillars of platform engineering?" }
    ],
    "conversation_id": null
  }'
```

## 🧪 Testing

The project includes a comprehensive test suite covering unit and integration tests.

```bash
# Run all tests
cargo test

# Run specific test
cargo test search_syntax_test
```

## 🛠 Development

- **Database Migrations**: Managed via `sqlx`.
  ```bash
  sqlx migrate run
  ```
- **Hot Reload**: Use `cargo-leptos` for development with hot reloading.
  ```bash
  cargo leptos watch
  ```

## License

© 2026 build by sojoner with AI   
