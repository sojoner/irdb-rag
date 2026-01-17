# Performance Testing Guide

Quick commands to verify performance improvements after deployment.

## 1. Check All Indexes Are Created

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
SELECT tablename, indexname, idx_scan as scans, idx_tup_read as tuples_read
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY tablename, indexname;
"
```

**Expected indexes:**
- `idx_documents_entities_persons`
- `idx_documents_entities_organizations`
- `idx_documents_entities_products`
- `idx_documents_entities_concepts`
- `idx_documents_keywords`
- `idx_documents_locations`
- `idx_documents_created_at`
- `idx_document_chunks_document_id`
- `idx_document_chunks_document_chunk_idx`

---

## 2. Test BM25 Search Speed

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
EXPLAIN ANALYZE
SELECT d.id, d.title, paradedb.score(d.id) as score
FROM documents d
WHERE d.id @@@ 'programming'
LIMIT 10;
"
```

**Expected Performance:**
- Planning Time: < 5 ms (was 46+ seconds)
- Execution Time: < 5 ms (was 46+ seconds)

---

## 3. Test Facet Aggregations

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
-- Test without query (no BM25)
\timing on
SELECT facet_name, COUNT(*) as value_count
FROM get_facet_aggregations(NULL, NULL)
GROUP BY facet_name;
\timing off
"
```

**Expected Performance:** < 5 seconds (depending on document volume)

---

## 4. Test Entity Facet Queries

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
-- Test JSONB index usage
EXPLAIN ANALYZE
SELECT jsonb_array_elements(entities->'persons')::text as person,
       COUNT(*) as count
FROM documents
WHERE entities->'persons' IS NOT NULL
GROUP BY person
LIMIT 20;
"
```

**Check for:** "Index Cond:" in the EXPLAIN output (indicates index is being used)

---

## 5. Test Array Facet Queries

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
-- Test array index usage
EXPLAIN ANALYZE
SELECT keyword, COUNT(*) as count
FROM documents,
     LATERAL UNNEST(keywords) as keyword
WHERE keywords IS NOT NULL
GROUP BY keyword
LIMIT 20;
"
```

**Check for:** "Index Cond:" or "Filter:" in the EXPLAIN output

---

## 6. Test Date Range Queries

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
-- Test date index
EXPLAIN ANALYZE
SELECT COUNT(*)
FROM documents
WHERE created_at BETWEEN NOW() - INTERVAL '30 days' AND NOW();
"
```

**Check for:** "Index Scan" or "Bitmap Index Scan" (not Seq Scan)

---

## 7. Test Document Chunk Retrieval

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
-- Test chunk index
EXPLAIN ANALYZE
SELECT chunk_index, content
FROM document_chunks
WHERE document_id = (SELECT id FROM documents LIMIT 1)
ORDER BY chunk_index;
"
```

**Expected:** Index scan using `idx_document_chunks_document_id`

---

## 8. Full Performance Comparison

Run this script to compare before/after performance:

```bash
cat > /tmp/perf_test.sql << 'EOF'
-- BM25 Search
\echo '=== BM25 Search Performance ==='
\timing on
SELECT COUNT(*) FROM documents WHERE id @@@ 'programming';
\timing off

-- Facet Aggregations
\echo '=== Facet Aggregations Performance ==='
\timing on
SELECT COUNT(*) FROM get_facet_aggregations('programming', NULL);
\timing off

-- Entity Facets
\echo '=== Entity Facet Performance ==='
\timing on
SELECT COUNT(DISTINCT document_id)
FROM (SELECT jsonb_array_elements(entities->'persons')::text, document_id FROM documents) t;
\timing off

-- Keyword Facets
\echo '=== Keyword Facet Performance ==='
\timing on
SELECT COUNT(DISTINCT document_id)
FROM (SELECT keyword, document_id FROM documents, LATERAL UNNEST(keywords) as keyword) t;
\timing off

-- Chunk Retrieval
\echo '=== Chunk Retrieval Performance ==='
\timing on
SELECT COUNT(*) FROM document_chunks WHERE document_id IS NOT NULL;
\timing off

-- Date Range
\echo '=== Date Range Query Performance ==='
\timing on
SELECT COUNT(*) FROM documents WHERE created_at > NOW() - INTERVAL '365 days';
\timing off
EOF

docker cp /tmp/perf_test.sql rag-db:/tmp/perf_test.sql
docker exec rag-db psql -U rag_user -d rag_chat -f /tmp/perf_test.sql
```

---

## 9. ParadeDB Index Health Check

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
SELECT * FROM paradedb.index_info('documents_search_idx');
"
```

**Key metrics:**
- `num_docs`: Should match COUNT(*) FROM documents
- `num_deleted`: Should be low (0 if no deletes)
- `mutable`: Should be true for live indexes

---

## 10. Query Stats Analysis

```bash
docker exec rag-db psql -U rag_user -d rag_chat -c "
-- Show slowest queries
SELECT query, calls, mean_exec_time, max_exec_time
FROM pg_stat_statements
WHERE query NOT LIKE '%pg_stat%'
ORDER BY mean_exec_time DESC
LIMIT 10;
"
```

**Note:** Requires `pg_stat_statements` extension enabled

---

## Troubleshooting

### Indexes Not Being Used
```sql
-- Check if statistics are outdated
ANALYZE documents;
ANALYZE document_chunks;

-- Check index bloat
SELECT schemaname, tablename, ROUND(100.0 * (CASE
  WHEN otta > 0 THEN sml_head_waste + sml_item_waste + big_item_waste
  ELSE 0 END) / dml_total_size, 2) as waste_ratio
FROM pgstattuple_approx('documents');
```

### Slow BM25 Queries
```sql
-- Rebuild index if corrupted
DROP INDEX documents_search_idx;
CREATE INDEX documents_search_idx ON documents
USING bm25 (id, content, title, summary)
WITH (key_field='id');
```

### Memory Issues
```sql
-- Check work memory
SHOW work_mem;
SET work_mem = '256MB';  -- Increase if needed
```

---

## Performance Benchmarking Template

Use this template to track performance over time:

```
Date: ____________________
Database Size: _____ documents, _____ chunks
Action: ____________________

BM25 Search (LIMIT 10):
  - Planning: _____ ms
  - Execution: _____ ms
  - Total: _____ ms

Facet Aggregations:
  - Total time: _____ ms

Entity Facets:
  - Total time: _____ ms

Keyword Facets:
  - Total time: _____ ms

Chunk Retrieval (by document):
  - Total time: _____ ms

Notes: ____________________
```

---

## Automated Monitoring Script

```bash
#!/bin/bash
# Run daily to track performance

echo "$(date): Performance Check"

docker exec rag-db psql -U rag_user -d rag_chat << EOF
\echo '=== Performance Metrics ==='
SELECT NOW() as timestamp,
       (SELECT COUNT(*) FROM documents) as num_documents,
       (SELECT COUNT(*) FROM document_chunks) as num_chunks;

\echo '=== Index Sizes ==='
SELECT schemaname, indexname, pg_size_pretty(pg_relation_size(indexrelid)) as size
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY pg_relation_size(indexrelid) DESC;

\echo '=== Most Used Indexes ==='
SELECT indexname, idx_scan as scans, idx_tup_read as tuples
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC
LIMIT 10;
EOF
```

Save as `monitor_perf.sh` and run with: `chmod +x monitor_perf.sh && ./monitor_perf.sh`
