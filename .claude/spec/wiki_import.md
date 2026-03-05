# Wikipedia Import Specification (Metadata & BM25)

## Database Schema (Optimized for Search)

```sql
CREATE TABLE enwiki_pages (
    id BIGINT PRIMARY KEY,
    title TEXT NOT NULL,
    content TEXT NOT NULL,       -- Cleaned plaintext for RAG/BM25
    infobox JSONB,               -- Structured data (Key-Value pairs)
    categories TEXT[],           -- Array of category tags
    templates TEXT[],            -- List of used templates
    references_count INT,        -- Popularity/Importance signal
    tsv tsvector,                -- Pre-computed search vector (for BM25)
    last_modified TIMESTAMPTZ
);

-- Advanced BM25 / Full-Text Search Indices
CREATE INDEX idx_enwiki_pages_tsv ON enwiki_pages USING GIN (tsv);
CREATE INDEX idx_enwiki_infobox_path ON enwiki_pages USING GIN (infobox jsonb_path_ops);
CREATE INDEX idx_enwiki_categories ON enwiki_pages USING GIN (categories);
```

## Constraints
- **No Embeddings:** Wikipedia pages are NOT passed through the embedding models (LLM) during import to maintain high throughput. Search is handled via BM25 (FTS) indices.
- **Background Task:** Must integrate with `src/services/import.rs` job runner.
- **Progress Reporting:** The worker must update the `import_jobs` table every $N$ items to reflect progress in the UI.

## Metadata Extraction Strategy
- **Infobox:** Extract parameters using recursive template matching.
- **Categories:** Extract from `[[Category:...]]` tags.
- **Clean WikiText:** Preserves headers and paragraphs, removes templates and citations.

## UI Integration
- **Source Type:** `wikipedia`
- **Source Path:** Absolute path to the `.bz2` file.
- **Job Tracking:** Visible in the standard Import Job list with percentage progress.
