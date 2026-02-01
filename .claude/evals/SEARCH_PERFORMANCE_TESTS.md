# Search Performance Tests Guide

Comprehensive Rust test suites for faceted and hybrid search with performance measurements.

## Overview

- **`tests/search_performance_test.rs`**: Database-level performance tests
  - 50+ test cases for BM25, facets, filters, and chunks
  - Timing measurements with assertions
  - Query plan analysis via EXPLAIN ANALYZE

- **`tests/api_search_test.rs`**: HTTP API-level tests
  - Tests search endpoints with various filter combinations
  - Concurrent request handling
  - Performance benchmarking
  - Weight ratio testing (0.5/0.5, 0.8/0.2, 0.2/0.8)

- **`tests/faceted_search_perf.sql`**: Pure SQL performance benchmarks
  - 18 test queries covering all search scenarios
  - Index usage verification
  - BM25 index health checks

## Prerequisites

### Database Setup
```bash
# Start GPU development environment with database
make gpu-up

# Verify database is initialized
make gpu-verify-db

# Optionally import sample documents
# The import job is typically running when you start make gpu-up
```

### For API Tests
```bash
# Start web server (included in make gpu-up)
# Server runs on http://localhost:3000
make gpu-up
```

## Running Tests

### Quick Start

```bash
# Run ALL search performance tests (database + API)
make search-tests-all

# Or individual components:
make search-perf-test        # Database tests
make api-search-test          # API tests
```

### Database-Level Tests

```bash
# Complete database performance suite
make search-perf-test

# Specific test categories:
make search-perf-bm25         # BM25 search tests
make search-perf-facets       # Faceted search tests
make search-perf-filters      # Filter tests
make search-perf-comprehensive # Full suite with summary
```

#### Running Specific Tests

```bash
# Run individual test (replace 'test_name' with actual test name)
RUN_ENV=test-gpu cargo test --test search_performance_test test_name -- --ignored --nocapture

# Examples:
RUN_ENV=test-gpu cargo test --test search_performance_test test_bm25_search_basic -- --ignored --nocapture
RUN_ENV=test-gpu cargo test --test search_performance_test test_keywords_facet_performance -- --ignored --nocapture
```

### API-Level Tests

```bash
# All API tests (requires server running)
make api-search-test

# API performance benchmark
make api-search-benchmark

# Specific API test
cargo test --test api_search_test test_search_request_basic -- --ignored --nocapture
```

### SQL Benchmarks

```bash
# Run raw SQL performance tests (manual)
docker exec rag-db psql -U rag_user -d rag_chat -f tests/faceted_search_perf.sql
```

## Test Categories

### 1. Database Performance Tests (search_performance_test.rs)

#### BM25 Search
- **`test_database_statistics()`**: Count documents, chunks, indexed status
- **`test_bm25_search_basic()`**: Basic BM25 query performance
- **`test_bm25_search_empty_query()`**: Empty query handling

**Performance targets**:
- Execution: < 100ms
- Planning (with index): < 5ms

#### Faceted Search - Arrays
- **`test_keywords_facet_performance()`**: UNNEST + GROUP BY performance
- **`test_locations_facet_performance()`**: Location array aggregation

**Performance targets**:
- Execution: < 500ms (with GIN index)

#### Faceted Search - JSONB Entities
- **`test_entities_facet_persons_performance()`**: Person extraction from JSONB
- **`test_entities_facet_organizations_performance()`**: Organization extraction
- **`test_entities_facet_concepts_performance()`**: Concept extraction

**Performance targets**:
- Execution: < 500ms (with GIN index on JSONB)

#### Filter Tests
- **`test_date_range_filter_performance()`**: Date range queries
- **`test_combined_filters_performance()`**: Multiple filter combinations

**Performance targets**:
- Date range: < 100ms (index: idx_documents_created_at)
- Combined: < 300ms

#### Chunk Retrieval
- **`test_document_chunk_retrieval_performance()`**: Single document chunks
- **`test_batch_chunk_retrieval_performance()`**: Multiple document chunks

**Performance targets**:
- Single doc: < 100ms (index: idx_document_chunks_document_id)
- Batch: < 200ms

#### Index Usage
- **`test_index_usage_statistics()`**: View which indexes are active
- **`test_bm25_index_health()`**: BM25 index metrics

#### Comprehensive Suite
- **`test_comprehensive_search_performance()`**: All tests with summary

### 2. API-Level Tests (api_search_test.rs)

#### Basic Search
- **`test_search_request_basic()`**: Simple search query
- **`test_search_empty_query()`**: Empty query handling

#### Filter Combinations
- **`test_search_with_keywords_filter()`**: Keywords filter
- **`test_search_with_date_filter()`**: Date range filter
- **`test_search_with_entity_filters()`**: Persons, organizations, concepts

#### Weight Ratios
- **`test_search_bm25_heavy()`**: 0.8 BM25 / 0.2 Vector
- **`test_search_vector_heavy()`**: 0.2 BM25 / 0.8 Vector

#### Endpoints
- **`test_aggregation_stats_endpoint()`**: `/api/aggregation-stats`
- **`test_concurrent_searches()`**: 5 concurrent requests

#### Benchmarks
- **`test_search_performance_benchmark()`**: Full performance comparison

**Performance targets**:
- Basic search: < 1000ms
- Filtered search: < 1500ms
- Aggregation stats: < 500ms

### 3. SQL Benchmarks (faceted_search_perf.sql)

18 test queries covering:
1. Database statistics
2. BM25 search (balanced weights)
3. BM25-heavy search (0.8/0.2)
4. Keywords array index
5. JSONB persons entity
6. JSONB organizations entity
7. JSONB concepts entity
8. Locations array
9. Date range filter
10. Combined facet aggregations
11. Facet aggregations with BM25 query
12. Document chunk retrieval
13. Batch chunk retrieval
14. Index usage statistics
15. BM25 index health
16. Created-at index performance
17. Complex combined filters
18. Vector similarity search (if embeddings exist)

Each query includes `EXPLAIN ANALYZE` output.

## Performance Metrics

### Key Indexes to Monitor

```sql
-- GIN indexes for array fields
idx_documents_keywords
idx_documents_locations

-- GIN indexes for JSONB fields
idx_documents_entities_persons
idx_documents_entities_organizations
idx_documents_entities_products
idx_documents_entities_concepts

-- Composite index for chunks
idx_document_chunks_document_id
idx_document_chunks_document_chunk_idx

-- Date index
idx_documents_created_at

-- BM25 full-text search
documents_search_idx (ParadeDB)
```

### Expected Performance (with 1000+ documents)

| Operation | Time | Index |
|-----------|------|-------|
| BM25 search | 1-5ms | documents_search_idx |
| Keywords facet | 50-200ms | idx_documents_keywords |
| Entity facets | 100-300ms | idx_documents_entities_* |
| Date filter | 10-50ms | idx_documents_created_at |
| Chunk retrieval | 20-100ms | idx_document_chunks_* |
| Combined filters | 200-500ms | Multiple |

## Troubleshooting

### Tests Not Found
```bash
# Make sure test files are in tests/ directory
ls -la tests/search_performance_test.rs
ls -la tests/api_search_test.rs

# Re-add to git if needed
git add tests/search_performance_test.rs
git add tests/api_search_test.rs
```

### Database Connection Errors
```bash
# Check if database is running
make gpu-verify-db

# If not running, start it
make gpu-up

# Check database tables
docker exec rag-db psql -U rag_user -d rag_chat -c "\dt"
```

### No Documents Found
```bash
# Check document count
docker exec rag-db psql -U rag_user -d rag_chat -c "SELECT COUNT(*) FROM documents"

# Import documents if needed
make gpu-up  # starts import job automatically

# Check import progress
docker exec rag-db psql -U rag_user -d rag_chat -c "
  SELECT status, COUNT(*) FROM import_jobs GROUP BY status
"
```

### API Tests Failing
```bash
# Make sure server is running
make gpu-logs

# Check if server is responding
curl http://localhost:3000/health

# If server not running, start it
make gpu-up
```

### Slow Query Performance
```bash
# Verify indexes are created
docker exec rag-db psql -U rag_user -d rag_chat -c "
  SELECT indexname FROM pg_indexes
  WHERE schemaname = 'public'
  ORDER BY indexname
"

# Check if index is being used
EXPLAIN ANALYZE SELECT ... WHERE ...

# Rebuild index if corrupted
DROP INDEX documents_search_idx;
CREATE INDEX documents_search_idx ON documents
USING bm25 (id, content, title, summary)
WITH (key_field='id');
```

## Interpreting Results

### EXPLAIN ANALYZE Output

Key metrics to check:

1. **Planning Time** (should be < 5ms with proper indexes)
   ```
   Planning Time: 1.905 ms
   ```

2. **Execution Time** (should scale with data)
   ```
   Execution Time: 0.464 ms
   ```

3. **Index Usage** (should see "Index Scan" not "Seq Scan")
   ```
   ->  Index Scan using idx_documents_keywords ...
   ```

4. **Row Estimates** (should match actual rows)
   ```
   rows=10 (actual 10 loops=1)
   ```

### Performance Regression

If queries are slower than expected:

1. Check index statistics
   ```bash
   ANALYZE documents;
   ANALYZE document_chunks;
   ```

2. Rebuild corrupted indexes
   ```bash
   REINDEX INDEX documents_search_idx;
   ```

3. Check database bloat
   ```bash
   SELECT * FROM pgstattuple_approx('documents');
   ```

## Continuous Integration

Add to CI pipeline:

```bash
# Fast unit tests (no DB)
make test-unit

# Database performance tests
make search-perf-comprehensive

# API tests
make api-search-benchmark
```

## Adding New Tests

### Template for Database Test

```rust
#[tokio::test]
#[ignore]
async fn test_my_search_feature() {
    let pool = get_test_pool().await;

    println!("\n=== My Search Feature ===");

    let (results, elapsed): (Vec<_>, _) = measure_query(
        "My feature",
        async {
            sqlx::query_as::<_, (Uuid, String)>(
                "SELECT id, title FROM documents LIMIT 10"
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default()
        }
    )
    .await;

    println!("  Results: {}", results.len());
    assert!(elapsed < 100, "Should be fast");
}
```

### Template for API Test

```rust
#[tokio::test]
#[ignore]
async fn test_my_api_feature() {
    println!("\n=== My API Feature ===");

    let client = reqwest::Client::new();

    let (response, elapsed): (Result<_, _>, _) = measure_api_call(
        "My API feature",
        async {
            client
                .post("http://localhost:3000/api/search")
                .json(&SearchRequest { ... })
                .send()
                .await
        }
    )
    .await;

    assert!(elapsed < 1000, "Should complete in time");
}
```

## Performance Optimization Tips

1. **Indexes**: Verify all performance indexes are created
2. **Query Plans**: Use `EXPLAIN ANALYZE` to find bottlenecks
3. **Caching**: Consider caching facet aggregations
4. **Batch Size**: Adjust chunk batch sizes for optimal throughput
5. **Parallelism**: Use concurrent requests to stress-test
6. **Monitoring**: Track query times over time for regressions

## References

- [PERFORMANCE_OPTIMIZATION_SUMMARY.md](./PERFORMANCE_OPTIMIZATION_SUMMARY.md) - Database optimization status
- [sql/init.sql](../sql/init.sql) - Index definitions
- [src/infra/db.rs](../src/infra/db.rs) - Search implementation
- [src/api/handlers.rs](../src/api/handlers.rs) - API handlers
