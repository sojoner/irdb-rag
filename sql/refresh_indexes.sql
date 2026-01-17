-- ============================================
-- INDEX REFRESH AND OPTIMIZATION SCRIPT
-- ============================================
-- Run this periodically to maintain search performance
-- Usage: docker exec rag-db psql -U rag_user -d rag_chat -f /path/to/refresh_indexes.sql

\echo '=== Starting Index Refresh ==='
\echo ''

-- Clean up dead tuples and update statistics
\echo '[1/5] Running VACUUM ANALYZE on documents...'
VACUUM ANALYZE documents;

\echo '[2/5] Running VACUUM ANALYZE on document_chunks...'
VACUUM ANALYZE document_chunks;

-- Rebuild BM25 indexes for optimal performance
\echo '[3/5] Rebuilding BM25 index on documents...'
REINDEX INDEX documents_search_idx;

\echo '[4/5] Rebuilding BM25 index on chunks...'
REINDEX INDEX chunks_search_idx;

-- Rebuild vector indexes if they exist
\echo '[5/5] Rebuilding HNSW vector indexes...'
REINDEX INDEX CONCURRENTLY IF EXISTS idx_documents_embedding;
REINDEX INDEX CONCURRENTLY IF EXISTS idx_document_chunks_embedding;

\echo ''
\echo '=== Index Refresh Complete ==='
\echo ''

-- Display index statistics
\echo 'Documents BM25 Index Stats:'
SELECT index_name, num_docs, num_deleted, byte_size, mutable
FROM paradedb.index_info('documents_search_idx')
LIMIT 1;

\echo ''
\echo 'Chunks BM25 Index Stats (sample):'
SELECT index_name, num_docs, num_deleted, byte_size, mutable
FROM paradedb.index_info('chunks_search_idx')
LIMIT 3;

\echo ''
\echo 'All Indexes on Documents Table:'
SELECT indexname, pg_size_pretty(pg_relation_size(indexname::regclass)) as size
FROM pg_indexes
WHERE tablename = 'documents'
ORDER BY indexname;

\echo ''
\echo 'All Indexes on Document Chunks Table:'
SELECT indexname, pg_size_pretty(pg_relation_size(indexname::regclass)) as size
FROM pg_indexes
WHERE tablename = 'document_chunks'
ORDER BY indexname;
