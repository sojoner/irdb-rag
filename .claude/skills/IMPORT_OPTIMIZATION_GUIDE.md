# Import Performance Optimization Guide

This guide explains how to optimize document import performance for PDFs, especially handling filename issues and scanned documents.

## Quick Start

### For Native Text PDFs (Fast)

```toml
[docling]
# Skip OCR for native text PDFs - much faster
do_ocr = false
do_layout_analysis = false
do_table_structure = false
```

**Expected speedup**: 3-5x faster

### For Scanned PDFs (Requires OCR)

```toml
[docling]
# Full processing for scanned documents
do_ocr = true
do_layout_analysis = true
do_table_structure = true
```

**Trade-off**: Slower but extracts text from images

### For Mixed Document Collections (Balanced)

```toml
[docling]
# Enable all features for quality
do_ocr = true
do_layout_analysis = true
do_table_structure = true

[import]
# Reduce concurrency to avoid service overload
max_concurrent_documents = 8
indexing_batch_size = 4
```

---

## Understanding Docling Optimizations

Per the [Docling Performance Guide](https://deepwiki.com/lanarich/docling/8.3-performance-optimization), here are the key optimization strategies:

### 1. OCR Processing (`do_ocr`)

**What it does**: Extracts text from scanned images in PDFs
- Enabled: Handles all PDFs including scanned documents
- Disabled: Only works for native text PDFs

**Impact on Speed**:
- Enabled: Adds 50-70% overhead (slowest component)
- Disabled: 3-5x faster for text-only PDFs

**Use Disabled If**:
- All documents are native text PDFs
- You don't need to extract from scanned documents
- Processing speed is critical

### 2. Layout Analysis (`do_layout_analysis`)

**What it does**: Preserves document structure (sections, columns, etc)
- Enabled: Maintains reading order and hierarchy
- Disabled: Extracts text without preserving layout

**Impact on Speed**:
- Enabled: Adds 20-30% overhead
- Disabled: Faster extraction, loses layout info

**Use Disabled If**:
- Layout structure isn't important for search
- Processing text-only documents
- Speed is prioritized over structure

### 3. Table Structure Analysis (`do_table_structure`)

**What it does**: Extracts table structure (rows, columns, cells)
- Enabled: Converts tables to structured format
- Disabled: Treats tables as text

**Impact on Speed**:
- Enabled: Adds 10-20% overhead
- Disabled: Slight speed improvement

**Use Disabled If**:
- Documents don't have tables
- Table structure isn't needed
- Documents are primarily text

### 4. Model Pre-loading

**What it does**: Load Docling models before processing

**Pre-download models** to avoid initialization overhead:

```bash
# In container or on Docling host
DOCLING_ARTIFACTS_PATH=/path/to/cache docling-tools models download
```

**Set environment variable**:
```bash
export DOCLING_ARTIFACTS_PATH=/path/to/models
```

This centralizes model storage and speeds up subsequent processing.

---

## Configuration Examples

### Configuration 1: Text-Only Documents (Fastest)

```toml
# config/fast.toml
[docling]
url = "http://localhost:5001"
timeout_seconds = 300
do_ocr = false
do_layout_analysis = false
do_table_structure = false

[import]
workers = 8
indexing_batch_size = 16
max_concurrent_documents = 32
```

**Speed**: Ultra-fast
**Best for**: Native text PDFs, markdown, HTML, plain text
**Trade-offs**: Can't process scanned documents

### Configuration 2: Mixed Documents (Balanced)

```toml
# config/balanced.toml
[docling]
url = "http://localhost:5001"
timeout_seconds = 600
do_ocr = true
do_layout_analysis = true
do_table_structure = true

[import]
workers = 4
indexing_batch_size = 8
max_concurrent_documents = 16
```

**Speed**: Balanced
**Best for**: Mixed collections with some scanned documents
**Trade-offs**: Slower than text-only, but handles all formats

### Configuration 3: Quality-First (Comprehensive)

```toml
# config/quality.toml
[docling]
url = "http://localhost:5001"
timeout_seconds = 900
do_ocr = true
do_layout_analysis = true
do_table_structure = true

[import]
workers = 2
indexing_batch_size = 4
max_concurrent_documents = 8
```

**Speed**: Slowest but most comprehensive
**Best for**: Large documents, complex layouts, critical data
**Trade-offs**: Long processing time

---

## Testing & Validation

### Test with Failing PDFs

```bash
# Create test file with problematic characters
cat > test_files.txt << 'EOF'
"file—with–dashes.pdf"
"file_with_émojis_📄.pdf"
"file:with:colons.pdf"
"file|with|pipes.pdf"
".hidden_file.pdf"
EOF

# Run import with GPU test environment
make gpu-up
make gpu-test TEST_FILTER=test_import
```

### Monitor Performance

**Check Docling processing time** in logs:

```bash
# View GPU logs
docker compose -f docker-compose-gpu.yml logs -f rag-docling
```

**Expected log output**:
```
[INFO] Processing file: document.pdf
[INFO] Stage 1/5: Extracting & enriching content... (2.5s)
[INFO] Stage 2/5: Chunking content... (0.3s)
[INFO] Stage 3/5: Enriching chunks... (0.2s)
[INFO] Stage 4/5: Generating embeddings... (1.2s)
[INFO] Stage 5/5: Storing in database... (0.5s)
[INFO] Total time: 4.7s
```

### Test Edge Cases

```bash
# Test filename sanitization
make gpu-test TEST_FILTER=test_sanitize_filename

# Test scanned PDF detection
make gpu-test TEST_FILTER=test_scanned_pdf

# Test error classification
cargo test classify_error
```

---

## Error Handling & Recovery

### New Error Classification

The system now classifies errors as:

1. **Transient** (Auto-retry with backoff):
   - Timeouts
   - Service temporarily unavailable (503, 502, 504)
   - Rate limits
   - Connection refused

2. **Permanent** (Skip immediately):
   - Unsupported file format
   - Corrupted/damaged file
   - Encoding errors (UTF-8, character decode)
   - File not found
   - Permission denied
   - **Scanned image detected** (when OCR disabled)

### Example Errors

**Scanned PDF with OCR disabled**:
```
"Scanned image detected - enable do_ocr in config to process"
→ Classification: Permanent (skip)
→ Import job: marked as skipped, not retried
```

**Filename with special characters**:
```
"file—with–dashes.pdf"
→ Sanitized to: "file-with-dashes.pdf"
→ Sent to Docling
→ Success
```

**UTF-8 encoding error**:
```
"UTF-8 decode error in filename"
→ Fallback to: "document.pdf"
→ Continue processing
```

---

## Optimization Checklist

### Before Large Imports

- [ ] Check OCR requirement: Are there scanned PDFs?
  - Yes → Set `do_ocr = true`
  - No → Set `do_ocr = false`

- [ ] Filenames: Do files have special characters?
  - Yes → System now auto-sanitizes
  - No → No action needed

- [ ] Concurrency: What are resource limits?
  - CPU constrained → Lower `max_concurrent_documents`
  - Memory constrained → Lower `indexing_batch_size`
  - GPU constrained → Adjust worker count

- [ ] Models: Are Docling models pre-downloaded?
  - No → Run `docling-tools models download` first

### During Import

```bash
# Monitor in real-time
make gpu-logs | grep -E "(Stage|Duration|✓|✗|⏭️)"

# Count progress
docker compose -f docker-compose-gpu.yml logs | grep "completed\|failed\|skipped" | tail -5
```

### After Import

```bash
# Check final stats
docker exec rag-db psql -U rag_user -d rag_chat << EOF
SELECT COUNT(*) as documents,
       SUM((SELECT COUNT(*) FROM document_chunks WHERE document_id = documents.id)) as chunks
FROM documents;
EOF

# Check error breakdown
docker exec rag-db psql -U rag_user -d rag_chat << EOF
SELECT error_type, COUNT(*) as count
FROM import_items WHERE status = 'failed'
GROUP BY error_type;
EOF
```

---

## Environment-Specific Recommendations

### Development (Laptop)

```toml
[docling]
do_ocr = false
do_layout_analysis = false
do_table_structure = false

[import]
workers = 1
indexing_batch_size = 2
max_concurrent_documents = 2
```

### GPU Server (RTX 3090)

```toml
[docling]
do_ocr = true
do_layout_analysis = true
do_table_structure = true

[import]
workers = 4
indexing_batch_size = 8
max_concurrent_documents = 16
```

### Production Server (CPU-only)

```toml
[docling]
do_ocr = false
do_layout_analysis = true
do_table_structure = false

[import]
workers = 2
indexing_batch_size = 4
max_concurrent_documents = 8
```

---

## Troubleshooting

### "Scanned image detected - enable OCR"

**Problem**: Imported file has no text, or very little
**Solution**:
- Set `do_ocr = true` in config
- Re-run import

### "UTF-8 decode error"

**Problem**: File has encoding issues in filename or content
**Solution**:
- Rename file to use ASCII characters only
- System auto-sanitizes special characters
- If still fails, file is corrupted (permanent skip)

### "Request timeout"

**Problem**: Docling takes too long to process
**Solution**:
- Increase `timeout_seconds` in docling config
- Disable optional features (`do_layout_analysis`, `do_table_structure`)
- Reduce `max_concurrent_documents` to avoid overload

### "Failed after 3 retries"

**Problem**: Transient errors keep occurring
**Solution**:
- Check Docling service health: `curl http://localhost:5001/health`
- Restart Docling: `docker compose restart docling`
- Check available memory/GPU

### Import Job Stuck

**Problem**: Job shows "running" but no progress
**Solution**:
- Check worker logs: `make gpu-logs | grep "Worker"`
- Recover stuck jobs manually:
  ```sql
  UPDATE import_jobs SET status = 'pending' WHERE status = 'running' AND updated_at < NOW() - INTERVAL '1 hour';
  ```

---

## Performance Metrics

### Typical Processing Times

With RTX 3090 GPU:

| Document Type | OCR Off | OCR On | Layout | Tables |
|---|---|---|---|---|
| Text PDF (5 pages) | 1-2s | 3-5s | +0.3s | +0.2s |
| Scanned PDF (5 pages) | N/A | 8-15s | +0.5s | +1s |
| Markdown (5KB) | 0.5-1s | 0.5-1s | - | - |
| HTML (10KB) | 0.5-1s | 0.5-1s | - | - |

### Batch Import Performance

| Configuration | Docs/Hour | Total Time (100 docs) |
|---|---|---|
| Fast (OCR off) | 600-1000 | 6-10 min |
| Balanced (OCR on) | 150-250 | 25-40 min |
| Quality (full) | 50-100 | 60-120 min |

---

## Next Steps

1. **Identify your document types**: Are they mostly text PDFs or scanned?
2. **Choose configuration**: Use Fast, Balanced, or Quality preset
3. **Run test import**: `make gpu-test TEST_FILTER=test_import`
4. **Monitor logs**: `make gpu-logs`
5. **Adjust concurrency**: Based on resource availability
6. **Run full import**: `cargo run -- index --path ./documents`

For more details, see the main [CLAUDE.md](/home/sojoner/workspace/irdb-rag/.claude/CLAUDE.md).
