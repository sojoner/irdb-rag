# Wikipedia Import Specification

## Database Schema

```sql
-- Table for raw wikipedia pages
CREATE TABLE enwiki_pages (
    id BIGINT PRIMARY KEY,
    title TEXT NOT NULL,
    ns INT NOT NULL,
    content TEXT NOT NULL, -- Cleaned plaintext
    raw_xml TEXT,          -- Optional: store raw for debugging/reprocessing
    timestamp TIMESTAMPTZ,
    tsv tsvector           -- Generated column for BM25/FTS
);

-- BM25 / Full-Text Search Index
CREATE INDEX idx_enwiki_pages_tsv ON enwiki_pages USING GIN (tsv);

-- If using ParadeDB or pg_search (Advanced BM25)
-- CALL paradedb.create_bm25_index('enwiki_pages', 'content', 'idx_bm25_enwiki');
```

## Rust Component: `src/services/import_wiki.rs`

### Data Structures
```rust
struct WikiPage {
    id: i64,
    title: String,
    ns: i32,
    content: String,
    timestamp: String,
}
```

### Main Pipeline
```rust
pub async fn import_dump(pool: &PgPool, path: PathBuf) -> Result<()> {
    let file = File::open(path)?;
    let decoder = bzip2::read::BzDecoder::new(file);
    let mut reader = quick_xml::Reader::from_reader(BufReader::new(decoder));
    
    // Page iterator -> Rayon parallel map -> Batch COPY
    // ...
}
```

## Cleaning Logic
- Strip templates: `{{ ... }}`
- Strip links: `[[File:...]]`, `[[Category:...]]`
- Convert `[[Target|Label]]` to `Label`
- Strip HTML-like tags: `<ref>`, `<div>`, etc.

## Performance Requirements
- **Memory Limit:** 10GB (Buffer size)
- **CPU Utilization:** 90%+ of 20 cores
- **Disk I/O:** Streamed reading, Batched writing
