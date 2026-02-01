# Wikipedia Import Evaluation & Verification

## Success Criteria

1. **Throughput:**
   - [ ] Import speed > 10,000 pages/sec.
   - [ ] Total time for full `enwiki` dump < 2 hours (estimated for 6M+ pages).

2. **Data Integrity:**
   - [ ] Final row count matches official dump metadata.
   - [ ] Titles and IDs correctly imported.
   - [ ] Plaintext is clean (no raw WikiText templates visible).

3. **Search Quality (BM25):**
   - [ ] Full-text search returns relevant results for "Quantum Mechanics".
   - [ ] Result ranking follows BM25 scoring.

## Test Cases

### 1. Small Dump Test
Run the importer on a 100MB subset of the dump.
- **Expected:** Success in < 30 seconds.
- **Check:** `SELECT count(*) FROM enwiki_pages;`

### 2. Multi-threaded Load
Monitor CPU usage during import.
- **Expected:** At least 16 cores should be at 100%.

### 3. Cleanup Verification
- **Query:** `SELECT content FROM enwiki_pages WHERE title = 'London' LIMIT 1;`
- **Expected:** No `{{ ... }}` or `[[ ... ]]` syntax.

## Recovery
- [ ] Support resuming from a specific page ID if the process crashes.
- [ ] Log failed page IDs to a separate file.
