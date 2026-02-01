# Wikipedia Import Plan

## Objective
Implement a high-performance, multi-threaded importer for Wikipedia XML multistream dumps (`.xml.bz2`) into a PostgreSQL database with BM25 indexing.

## Architecture
1. **Streaming Decompression:** Stream the `.bz2` file using `bzip2-rs` or `flate2` to avoid loading huge files into memory.
2. **Streaming XML Parsing:** Use `quick-xml` to iterate through `<page>` elements without a full DOM.
3. **Parallel Processing (Rayon):** 
    - Worker threads (16-20) will clean WikiText (remove templates, citations, etc.) using `parse-wiki-text`.
    - Extract plaintext for the BM25 index.
4. **Batch DB Upload (COPY):**
    - Use PostgreSQL `COPY` command (via `sqlx` or `tokio-postgres`) for bulk insertion.
    - Buffer processed pages into chunks of 10,000 before flushing to DB.

## Data Flow
`bz2 File` -> `bzip2 Stream` -> `XML Parser (page iterator)` -> `Parallel Map (WikiText -> Plaintext)` -> `Buffer` -> `Postgres COPY`

## Constraints & Hardware
- **Cores:** 20 (Target 18 for processing, 1 for I/O, 1 for Decompression).
- **RAM:** 90 GB (Large buffers for cleaning and batching allowed).
- **Speed Target:** >10,000 pages per second.

## Implementation Steps
1. Create `enwiki_pages` table with required columns.
2. Implement `WikiImporter` in `src/services/import_wiki.rs`.
3. Add `parse-wiki-text` and `bzip2` to `Cargo.toml`.
4. Implement the pipeline with error handling (skip malformed pages).
5. Add CLI command or API endpoint to trigger import.
6. Configure BM25 indices on the resulting table.
