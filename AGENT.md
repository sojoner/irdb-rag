# AGENT.md - RAG Chat System Advanced Knowledge Base

This file is the authoritative technical guide for AI agents (Goose, Claude Code, Cursor) working on this repository.

## 🏗 Project Architecture & Tech Stack
A high-performance, full-stack Rust RAG system utilizing a **Leptos 0.8** reactive UI and an **Axum** backend.

- **Framework**: Leptos 0.8 (SSR + Hydration).
- **Database (The Brain)**: **ParadeDB** (PostgreSQL-native BM25 + `pgvector`).
- **Embeddings**: **External LLM Provider** (OpenAI `text-embedding-3-small` or similar).
- **Document Parsing**: **Docling** (High-fidelity conversion of PDF/DOCX to Markdown).
- **UI Logic**: Tailwind CSS with a 2-column "Discovery vs. Synthesis" interface.

---

## 🧠 Advanced RAG Implementation (LlamaIndex-Inspired)

We implement advanced retrieval strategies to go beyond simple k-NN search:

### 1. Hybrid Retrieval & RRF
We don't rely solely on vectors. We utilize **Reciprocal Rank Fusion (RRF)** to combine:
- **Semantic Search**: via `pgvector` (External LLM Embeddings).
- **Keyword Search**: via ParadeDB's `@@@` BM25 operator.
- **Rule**: RRF normalizes ranks from both sources to ensure that exact keyword matches (like serial numbers or specific names) and semantic concepts both surface correctly.

### 2. Multi-Step Query Transformation
When a user asks a complex question, the system should:
1.  **De-compose**: Break the query into sub-questions if needed.
2.  **Rewrite**: Generate a better search query for the vector engine (HyDE - Hypothetical Document Embeddings - pattern).
3.  **Synthesize**: Combine multiple retrieved chunks into a coherent answer.

### 3. Response Synthesis Patterns
- **Refine**: Loop through retrieved chunks and progressively refine the answer.
- **Compact & Summarize**: Condense all retrieved context into a single prompt for the LLM if they fit within the context window.
- **Tree Summarization**: For large context, summarize chunks in pairs to create a final root answer.

---

## 📂 Project Structure & Module Rules

- **`src/app/`**: (Hydrate) Leptos components. Faceted filters for Categories, Keywords, and Typed Entities (Persons, Orgs).
- **`src/server.rs`**: (SSR) Server functions for DB and API calls.
- **`src/engine/`**: (SSR Only) Business logic for:
  - **`indexer.rs`**: Docling integration and chunking logic.
  - **`db.rs`**: SQL query builders for ParadeDB and RRF logic.
  - **`llm.rs`**: Clients for external Embedding and Chat APIs.

### ⚠️ Component Gating
Heavy crates (`sqlx`, `reqwest`, `lopdf`) **MUST** be gated:
```rust
#[cfg(feature = "ssr")]
pub async fn server_only_logic() { ... }
```

---

## 🛠 Advanced Development Guide

### Database: Typed Entity Search
Our schema supports **Typed Named Entity Recognition (NER)**. Use JSONB operators to filter search results:
```sql
-- Search for 'Pareto Principle' only within the 'concepts' category
SELECT * FROM documents 
WHERE entities->'concepts' ? 'Pareto Principle'
AND embedding <=> $1 < 0.5;
```

### Retrieval Optimization
If the agent needs to improve answer quality, focus on:
1.  **Context Window Management**: Limit chunks to 5-10 high-relevance items.
2.  **Citation Integrity**: Ensure the LLM returns sources by mapping `chunk_id` to document metadata.
3.  **Metadata Filtering**: Automate filter application (e.g., if a user mentions "Netflix", apply an organization filter for "Netflix" automatically).

---

## 🚀 Common Commands

```bash
# Start Docker Environment (ParadeDB + Docling)
docker compose up -d

# Develop with hot-reload (Leptos)
cargo leptos watch

# SQL Migration
sqlx migrate run
```

---

## 📖 RAG Research Summary for Agents
*For deeper context, refer to these LlamaIndex concepts we strive to mirror in Rust:*

- **Query Engines**: Mapping user intent to specific retrieval methods.
- **Response Synthesizers**: Strategies for merging context into answers.
- **Faceted Search**: Using structured metadata (from our LLM enrichment) to narrow the vector search space (Pre-filtering).

## 📝 Prompt Engineering for NER
When processing documents, use the "Mechanical Turk" prompt in `indexer.rs` to extract:
- **Keywords**: Thematic/Thematic topics (e.g., "Performance", "Security").
- **Entities**: Proper Nouns classified into `persons`, `organizations`, `locations`, `products`, `concepts`.

---
*Note: This project is sensitive to WASM size. Always prefer performing heavy computations (like similarity math or text processing) on the server side.*