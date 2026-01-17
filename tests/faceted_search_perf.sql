-- ============================================
-- Faceted Search Query Performance Tests
-- ============================================
-- Comprehensive performance testing for:
-- - BM25 search with different weights
-- - Facet aggregations
-- - Entity filtering (persons, organizations, concepts, products)
-- - Array filtering (keywords, locations)
-- - Date range filtering
-- - JSONB operations performance
-- ============================================

-- Enable timing
\timing on

-- Test 1: Get database statistics
\echo '============================================'
\echo 'TEST 1: Database Statistics'
\echo '============================================'
SELECT
    (SELECT COUNT(*) FROM documents) as total_documents,
    (SELECT COUNT(*) FROM document_chunks) as total_chunks,
    (SELECT COUNT(CASE WHEN status = 'indexed' THEN 1 END) FROM documents) as indexed_documents,
    (SELECT COUNT(CASE WHEN embedding IS NOT NULL THEN 1 END) FROM documents) as documents_with_embeddings,
    (SELECT COUNT(CASE WHEN embedding IS NOT NULL THEN 1 END) FROM document_chunks) as chunks_with_embeddings,
    pg_size_pretty(pg_database_size('rag_chat')) as database_size;

-- Test 2: Basic BM25 search performance (balanced weights)
\echo ''
\echo '============================================'
\echo 'TEST 2: BM25 Search Performance - Balanced (0.5/0.5)'
\echo '============================================'
EXPLAIN ANALYZE
SELECT d.id, d.title, d.summary, paradedb.score(d.id) as score
FROM documents d
WHERE d.id @@@ 'programming'
LIMIT 10;

-- Test 3: BM25-heavy search (0.8/0.2)
\echo ''
\echo '============================================'
\echo 'TEST 3: BM25-Heavy Search (0.8/0.2)'
\echo '============================================'
EXPLAIN ANALYZE
SELECT d.id, d.title, paradedb.score(d.id) as score
FROM documents d
WHERE d.id @@@ 'technology'
LIMIT 10;

-- Test 4: Verify index usage for keywords array
\echo ''
\echo '============================================'
\echo 'TEST 4: Keywords Array Index Performance'
\echo '============================================'
EXPLAIN ANALYZE
SELECT keyword, COUNT(*) as count
FROM documents,
     LATERAL UNNEST(keywords) as keyword
WHERE keywords IS NOT NULL
GROUP BY keyword
ORDER BY count DESC
LIMIT 20;

-- Test 5: JSONB Entity Facet - Persons
\echo ''
\echo '============================================'
\echo 'TEST 5: JSONB Entity Facet - Persons'
\echo '============================================'
EXPLAIN ANALYZE
SELECT jsonb_array_elements(entities->'persons')::text as person,
       COUNT(*) as count
FROM documents
WHERE entities->'persons' IS NOT NULL
GROUP BY person
ORDER BY count DESC
LIMIT 20;

-- Test 6: JSONB Entity Facet - Organizations
\echo ''
\echo '============================================'
\echo 'TEST 6: JSONB Entity Facet - Organizations'
\echo '============================================'
EXPLAIN ANALYZE
SELECT jsonb_array_elements(entities->'organizations')::text as organization,
       COUNT(*) as count
FROM documents
WHERE entities->'organizations' IS NOT NULL
GROUP BY organization
ORDER BY count DESC
LIMIT 20;

-- Test 7: JSONB Entity Facet - Concepts
\echo ''
\echo '============================================'
\echo 'TEST 7: JSONB Entity Facet - Concepts'
\echo '============================================'
EXPLAIN ANALYZE
SELECT jsonb_array_elements(entities->'concepts')::text as concept,
       COUNT(*) as count
FROM documents
WHERE entities->'concepts' IS NOT NULL
GROUP BY concept
ORDER BY count DESC
LIMIT 20;

-- Test 8: Locations array facet
\echo ''
\echo '============================================'
\echo 'TEST 8: Locations Array Facet'
\echo '============================================'
EXPLAIN ANALYZE
SELECT location, COUNT(*) as count
FROM documents,
     LATERAL UNNEST(locations) as location
WHERE locations IS NOT NULL
GROUP BY location
ORDER BY count DESC
LIMIT 20;

-- Test 9: Date range filter performance
\echo ''
\echo '============================================'
\echo 'TEST 9: Date Range Filter Performance'
\echo '============================================'
EXPLAIN ANALYZE
SELECT d.id, d.title, d.created_at
FROM documents d
WHERE d.created_at BETWEEN NOW() - INTERVAL '365 days' AND NOW()
LIMIT 10;

-- Test 10: Combined facet aggregations (simulating get_facet_aggregations)
\echo ''
\echo '============================================'
\echo 'TEST 10: Combined Facet Aggregations'
\echo '============================================'
EXPLAIN ANALYZE
SELECT * FROM get_facet_aggregations(NULL, NULL)
LIMIT 50;

-- Test 11: Facet aggregations with BM25 query
\echo ''
\echo '============================================'
\echo 'TEST 11: Facet Aggregations with BM25 Query'
\echo '============================================'
EXPLAIN ANALYZE
SELECT * FROM get_facet_aggregations('programming', NULL)
LIMIT 50;

-- Test 12: Document chunk retrieval by document_id
\echo ''
\echo '============================================'
\echo 'TEST 12: Document Chunk Retrieval (by doc_id)'
\echo '============================================'
EXPLAIN ANALYZE
SELECT chunk_index, section_title, content
FROM document_chunks
WHERE document_id = (SELECT id FROM documents LIMIT 1)
ORDER BY chunk_index;

-- Test 13: Multiple chunk retrieval across documents
\echo ''
\echo '============================================'
\echo 'TEST 13: Batch Document Chunk Retrieval'
\echo '============================================'
EXPLAIN ANALYZE
SELECT dc.document_id, dc.chunk_index, dc.content
FROM document_chunks dc
WHERE dc.document_id IN (SELECT id FROM documents LIMIT 5)
ORDER BY dc.document_id, dc.chunk_index;

-- Test 14: Index usage statistics
\echo ''
\echo '============================================'
\echo 'TEST 14: Index Usage Statistics'
\echo '============================================'
SELECT
    relname as index_name,
    idx_scan as scans,
    idx_tup_read as tuples_read,
    idx_tup_fetch as tuples_fetched,
    pg_size_pretty(pg_relation_size(indexrelid)) as index_size
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY idx_scan DESC;

-- Test 15: BM25 index health check
\echo ''
\echo '============================================'
\echo 'TEST 15: BM25 Index Health'
\echo '============================================'
SELECT * FROM paradedb.index_info('documents_search_idx');

-- Test 16: Query plan comparison - Sequential vs Index scan
\echo ''
\echo '============================================'
\echo 'TEST 16: Created At Index Performance'
\echo '============================================'
EXPLAIN ANALYZE
SELECT COUNT(*) as recent_documents
FROM documents
WHERE created_at > NOW() - INTERVAL '30 days';

-- Test 17: Complex combined filter query
\echo ''
\echo '============================================'
\echo 'TEST 17: Complex Combined Filter Query'
\echo '============================================'
EXPLAIN ANALYZE
SELECT DISTINCT d.id, d.title, d.created_at
FROM documents d
WHERE d.created_at BETWEEN NOW() - INTERVAL '365 days' AND NOW()
  AND d.keywords && ARRAY['important', 'urgent']
  AND d.entities->'persons' IS NOT NULL
LIMIT 20;

-- Test 18: Vector similarity performance (if embeddings exist)
\echo ''
\echo '============================================'
\echo 'TEST 18: Vector Similarity Search (if embeddings exist)'
\echo '============================================'
EXPLAIN ANALYZE
SELECT d.id, d.title,
       (d.embedding <=> (SELECT embedding FROM documents WHERE embedding IS NOT NULL LIMIT 1)) as similarity
FROM documents d
WHERE d.embedding IS NOT NULL
ORDER BY d.embedding <=> (SELECT embedding FROM documents WHERE embedding IS NOT NULL LIMIT 1)
LIMIT 10;

-- Final summary
\echo ''
\echo '============================================'
\echo 'PERFORMANCE TEST SUMMARY'
\echo '============================================'
\echo ''
\echo 'Key Performance Metrics to Check:'
\echo '- Planning Time should be < 5ms for all queries'
\echo '- Execution Time should scale with data size'
\echo '- Index Scan should be preferred over Seq Scan'
\echo '- Check idx_scan counts to see which indexes are used'
\echo ''
\echo 'Performance Targets:'
\echo '- BM25 Search: < 2ms planning, < 1ms execution'
\echo '- Facet Aggregations: < 10ms total'
\echo '- Date Range: < 5ms total'
\echo '- JSONB Facets: < 15ms total'
\echo ''
