# Wikipedia Import Plan (High-Performance Batch Edition)

## Objective
Implement an ultra-high-performance, multi-threaded importer for Wikipedia XML multistream dumps (`.xml.bz2`) into PostgreSQL. Target: >20,000 pages/sec using 20 cores and 90GB RAM.

## UI & Integration
- **Import UI Enhancement:** Add a "Wikipedia Dump" source type in the Import Manager.
- **Path Input:** User provides the absolute path to the `.xml.bz2` file on the server (e.g., `/data/backups/enwiki-20260101-pages-articles-multistream.xml.bz2`).
- **Background Job:** Triggering the import creates an `ImportJob` with `source_type = 'wikipedia'`.
- **Progress Monitoring:** The background task updates `processed_items` in the `import_jobs` table every 10,000 pages, allowing the UI to show real-time progress.

## Architecture & Parallelism
1. **Producer-Consumer Pipeline (Crossbeam):**
   - **Producer (1 Thread):** Decompresses `.bz2` and streams XML. Emits raw `<page>` blocks into a bounded channel.
   - **Workers (18 Threads - Rayon):** 
     - Receive page blocks.
     - Parse XML (id, title, timestamp).
     - **Metadata Extraction:** Extract Infobox data, Categories, and Templates into structured JSONB.
     - **WikiText Cleaning:** Strips markup while preserving semantic structure.
     - **Search Vector:** Skips embeddings (LLM) for speed; focuses on BM25 indices.
   - **DB Writer (1 Thread):** Collects processed results and executes high-speed `COPY`.

2. **Database Optimization (Postgres COPY):**
   - Use `Binary COPY` protocol for maximum ingestion speed.
   - Batch size: 50,000 rows per transaction.

## Implementation Steps
1. **Schema Setup:** Create `enwiki_pages` with GIN/BM25 indices.
2. **UI Update:** Add "Wikipedia" option to `src/web_app/pages/import.rs`.
3. **Job Runner Integration:** Extend `process_import_job` in `src/services/import.rs` to recognize 'wikipedia' jobs.
4. **Streaming Parser:** Implement non-blocking XML reader in `src/services/import_wiki.rs`.
5. **Batch Ingestion:** Implement the binary `COPY` sink.
