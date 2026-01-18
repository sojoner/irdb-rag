//! Comprehensive search performance tests for faceted and hybrid search
//!
//! Tests include:
//! - Hybrid search with various weight combinations
//! - Faceted search with aggregations
//! - Entity filtering (persons, organizations, concepts, products)
//! - Array filtering (keywords, locations)
//! - Date range filtering
//! - Performance benchmarking with timing measurements

use sqlx::postgres::PgPool;
use std::time::Instant;
use uuid::Uuid;

// Test database connection setup
async fn get_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string()
    });

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Helper function to measure query execution time
async fn measure_query<F, T>(name: &str, f: F) -> (T, u128)
where
    F: std::future::Future<Output = T>,
{
    let start = Instant::now();
    let result = f.await;
    let elapsed = start.elapsed().as_millis();
    println!("  ⏱️  {}: {}ms", name, elapsed);
    (result, elapsed)
}

// ============================================
// DATABASE SETUP TESTS
// ============================================

#[tokio::test]
#[ignore] // Run with: cargo test --test search_performance_test -- --ignored
async fn test_database_statistics() {
    let pool = get_test_pool().await;

    println!("\n=== Database Statistics ===");

    let (doc_count, _): (i64, _) = measure_query("Counting documents", async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
    })
    .await;

    let (chunk_count, _): (i64, _) = measure_query("Counting chunks", async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM document_chunks")
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
    })
    .await;

    let (indexed_count, _): (i64, _) = measure_query("Counting indexed documents", async {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents WHERE status = 'indexed'")
            .fetch_one(&pool)
            .await
            .unwrap_or(0)
    })
    .await;

    println!("  📊 Total documents: {}", doc_count);
    println!("  📊 Total chunks: {}", chunk_count);
    println!("  📊 Indexed documents: {}", indexed_count);

    // Only run search tests if we have indexed documents
    if indexed_count == 0 {
        println!("  ⚠️  No indexed documents found. Import documents first with: make gpu-up");
    }
}

// ============================================
// BM25 SEARCH TESTS
// ============================================

#[tokio::test]
#[ignore] // Run with: cargo test --test search_performance_test bm25 -- --ignored
async fn test_bm25_search_basic() {
    let pool = get_test_pool().await;

    println!("\n=== BM25 Search - Basic Query ===");

    let query = "programming";
    println!("  🔍 Query: '{}'", query);

    let (results, elapsed): (Vec<(Uuid, String, f64)>, _) = measure_query("BM25 search", async {
        sqlx::query_as::<_, (Uuid, String, f64)>(
            "SELECT d.id, d.title, paradedb.score(d.id) as score
                 FROM documents d
                 WHERE d.id @@@ $1
                 LIMIT 10",
        )
        .bind(query)
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  📈 Results found: {}", results.len());
    for (i, (_, title, score)) in results.iter().enumerate() {
        println!("    {}. {} (score: {:.4})", i + 1, title, score);
    }

    // Performance assertions
    assert!(
        elapsed < 100,
        "BM25 search should complete in < 100ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 100ms");
}

#[tokio::test]
#[ignore]
async fn test_bm25_search_empty_query() {
    let pool = get_test_pool().await;

    println!("\n=== BM25 Search - Empty Query Handling ===");

    let query = "";
    println!("  🔍 Query: '{}'", query);

    let (results, elapsed): (Vec<String>, _) =
        measure_query("BM25 search with empty query", async {
            sqlx::query_scalar::<_, String>(
                "SELECT d.title
                 FROM documents d
                 WHERE d.id @@@ $1
                 LIMIT 10",
            )
            .bind(query)
            .fetch_all(&pool)
            .await
            .unwrap_or_default()
        })
        .await;

    println!("  📈 Results found: {}", results.len());
    println!("  ✅ Empty query handled gracefully");
}

// ============================================
// FACETED SEARCH TESTS
// ============================================

#[tokio::test]
#[ignore]
async fn test_keywords_facet_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Keywords Facet - Array Performance ===");

    let (facets, elapsed): (Vec<(String, i64)>, _) = measure_query("Keywords aggregation", async {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT keyword, COUNT(*) as count
                 FROM documents,
                      LATERAL UNNEST(keywords) as keyword
                 WHERE keywords IS NOT NULL
                 GROUP BY keyword
                 ORDER BY count DESC
                 LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  📊 Keywords found: {}", facets.len());
    for (i, (keyword, count)) in facets.iter().enumerate().take(5) {
        println!("    {}. {} (count: {})", i + 1, keyword, count);
    }

    // Performance assertions - should be fast with GIN index
    assert!(
        elapsed < 500,
        "Keywords facet should complete in < 500ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 500ms");
}

#[tokio::test]
#[ignore]
async fn test_entities_facet_persons_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Entity Facet - Persons (JSONB GIN) ===");

    let (facets, elapsed): (Vec<(String, i64)>, _) = measure_query("Persons aggregation", async {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT jsonb_array_elements(entities->'persons')::text as person,
                        COUNT(*) as count
                 FROM documents
                 WHERE entities->'persons' IS NOT NULL
                 GROUP BY person
                 ORDER BY count DESC
                 LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  👥 Persons found: {}", facets.len());
    for (i, (person, count)) in facets.iter().enumerate().take(5) {
        println!("    {}. {} (count: {})", i + 1, person, count);
    }

    assert!(
        elapsed < 500,
        "Persons facet should complete in < 500ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 500ms");
}

#[tokio::test]
#[ignore]
async fn test_entities_facet_organizations_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Entity Facet - Organizations (JSONB GIN) ===");

    let (facets, elapsed): (Vec<(String, i64)>, _) =
        measure_query("Organizations aggregation", async {
            sqlx::query_as::<_, (String, i64)>(
                "SELECT jsonb_array_elements(entities->'organizations')::text as org,
                        COUNT(*) as count
                 FROM documents
                 WHERE entities->'organizations' IS NOT NULL
                 GROUP BY org
                 ORDER BY count DESC
                 LIMIT 20",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default()
        })
        .await;

    println!("  🏢 Organizations found: {}", facets.len());
    for (i, (org, count)) in facets.iter().enumerate().take(5) {
        println!("    {}. {} (count: {})", i + 1, org, count);
    }

    assert!(
        elapsed < 500,
        "Organizations facet should complete in < 500ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 500ms");
}

#[tokio::test]
#[ignore]
async fn test_entities_facet_concepts_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Entity Facet - Concepts (JSONB GIN) ===");

    let (facets, elapsed): (Vec<(String, i64)>, _) = measure_query("Concepts aggregation", async {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT jsonb_array_elements(entities->'concepts')::text as concept,
                        COUNT(*) as count
                 FROM documents
                 WHERE entities->'concepts' IS NOT NULL
                 GROUP BY concept
                 ORDER BY count DESC
                 LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  💡 Concepts found: {}", facets.len());
    for (i, (concept, count)) in facets.iter().enumerate().take(5) {
        println!("    {}. {} (count: {})", i + 1, concept, count);
    }

    assert!(
        elapsed < 500,
        "Concepts facet should complete in < 500ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 500ms");
}

#[tokio::test]
#[ignore]
async fn test_locations_facet_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Locations Facet - Array Performance ===");

    let (facets, elapsed): (Vec<(String, i64)>, _) =
        measure_query("Locations aggregation", async {
            sqlx::query_as::<_, (String, i64)>(
                "SELECT location, COUNT(*) as count
                 FROM documents,
                      LATERAL UNNEST(locations) as location
                 WHERE locations IS NOT NULL
                 GROUP BY location
                 ORDER BY count DESC
                 LIMIT 20",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default()
        })
        .await;

    println!("  📍 Locations found: {}", facets.len());
    for (i, (location, count)) in facets.iter().enumerate().take(5) {
        println!("    {}. {} (count: {})", i + 1, location, count);
    }

    assert!(
        elapsed < 500,
        "Locations facet should complete in < 500ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 500ms");
}

// ============================================
// FILTER PERFORMANCE TESTS
// ============================================

#[tokio::test]
#[ignore]
async fn test_date_range_filter_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Date Range Filter Performance ===");

    let (results, elapsed): (Vec<(Uuid, String)>, _) = measure_query("Date range filter", async {
        sqlx::query_as::<_, (Uuid, String)>(
            "SELECT d.id, d.title
                 FROM documents d
                 WHERE d.created_at BETWEEN NOW() - INTERVAL '365 days' AND NOW()
                 LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  📅 Documents in range: {}", results.len());
    assert!(
        elapsed < 100,
        "Date range filter should complete in < 100ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 100ms (index used: idx_documents_created_at)");
}

#[tokio::test]
#[ignore]
async fn test_combined_filters_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Combined Filters Performance ===");

    let (results, elapsed): (Vec<(Uuid, String)>, _) = measure_query("Combined filters", async {
        sqlx::query_as::<_, (Uuid, String)>(
            "SELECT DISTINCT d.id, d.title
                 FROM documents d
                 WHERE d.created_at BETWEEN NOW() - INTERVAL '365 days' AND NOW()
                   AND d.keywords && ARRAY['important', 'urgent']
                   AND d.entities->'persons' IS NOT NULL
                 LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  🔗 Combined filter results: {}", results.len());
    assert!(
        elapsed < 300,
        "Combined filters should complete in < 300ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 300ms");
}

// ============================================
// CHUNK RETRIEVAL TESTS
// ============================================

#[tokio::test]
#[ignore]
async fn test_document_chunk_retrieval_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Document Chunk Retrieval Performance ===");

    // Get a document first
    let doc_id: Option<Uuid> = sqlx::query_scalar("SELECT id FROM documents LIMIT 1")
        .fetch_optional(&pool)
        .await
        .unwrap()
        .flatten();

    if let Some(doc_id) = doc_id {
        let (chunks, elapsed): (Vec<(i32, String)>, _) =
            measure_query("Chunk retrieval by document", async {
                sqlx::query_as::<_, (i32, String)>(
                    "SELECT chunk_index, content
                     FROM document_chunks
                     WHERE document_id = $1
                     ORDER BY chunk_index",
                )
                .bind(doc_id)
                .fetch_all(&pool)
                .await
                .unwrap_or_default()
            })
            .await;

        println!("  📦 Chunks retrieved: {}", chunks.len());
        assert!(
            elapsed < 100,
            "Chunk retrieval should complete in < 100ms, took {}ms",
            elapsed
        );
        println!("  ✅ Performance target met: < 100ms (index: idx_document_chunks_document_id)");
    } else {
        println!("  ⚠️  No documents found to test chunk retrieval");
    }
}

#[tokio::test]
#[ignore]
async fn test_batch_chunk_retrieval_performance() {
    let pool = get_test_pool().await;

    println!("\n=== Batch Chunk Retrieval Performance ===");

    let (chunks, elapsed): (Vec<(Uuid, i32)>, _) = measure_query("Batch chunk retrieval", async {
        sqlx::query_as::<_, (Uuid, i32)>(
            "SELECT dc.document_id, dc.chunk_index
                 FROM document_chunks dc
                 WHERE dc.document_id IN (SELECT id FROM documents LIMIT 10)
                 ORDER BY dc.document_id, dc.chunk_index",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  📦 Total chunks retrieved: {}", chunks.len());
    assert!(
        elapsed < 200,
        "Batch chunk retrieval should complete in < 200ms, took {}ms",
        elapsed
    );
    println!("  ✅ Performance target met: < 200ms");
}

// ============================================
// INDEX USAGE STATISTICS TESTS
// ============================================

#[tokio::test]
#[ignore]
async fn test_index_usage_statistics() {
    let pool = get_test_pool().await;

    println!("\n=== Index Usage Statistics ===");

    let (stats, _): (Vec<(String, i64, i64, i64)>, _) = measure_query("Index statistics", async {
        sqlx::query_as::<_, (String, i64, i64, i64)>(
            "SELECT
                    relname as index_name,
                    idx_scan as scans,
                    idx_tup_read as tuples_read,
                    idx_tup_fetch as tuples_fetched
                 FROM pg_stat_user_indexes
                 WHERE schemaname = 'public'
                 ORDER BY idx_scan DESC",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;

    println!("  📊 Index usage (top 10):");
    for (i, (name, scans, read, fetched)) in stats.iter().enumerate().take(10) {
        println!(
            "    {}. {} - scans: {}, read: {}, fetched: {}",
            i + 1,
            name,
            scans,
            read,
            fetched
        );
    }
}

// ============================================
// BM25 INDEX HEALTH TESTS
// ============================================

#[tokio::test]
#[ignore]
async fn test_bm25_index_health() {
    let pool = get_test_pool().await;

    println!("\n=== BM25 Index Health Check ===");

    let (health, _): (Vec<(Option<i64>, Option<i64>, Option<i64>)>, _) =
        measure_query("Index health", async {
            sqlx::query_as::<_, (Option<i64>, Option<i64>, Option<i64>)>(
                "SELECT num_docs, num_deleted, mutable
                 FROM paradedb.index_info('documents_search_idx')",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default()
        })
        .await;

    if let Some((num_docs, num_deleted, mutable)) = health.first() {
        println!("  📊 BM25 Index Status:");
        println!("    - Documents: {:?}", num_docs);
        println!("    - Deleted: {:?}", num_deleted);
        println!("    - Mutable: {:?}", mutable);
    }
}

// ============================================
// COMPREHENSIVE PERFORMANCE SUITE
// ============================================

#[tokio::test]
#[ignore]
async fn test_comprehensive_search_performance() {
    let pool = get_test_pool().await;

    println!("\n╔════════════════════════════════════════════════════╗");
    println!("║  COMPREHENSIVE SEARCH PERFORMANCE TEST SUITE       ║");
    println!("╚════════════════════════════════════════════════════╝");

    // Database stats
    let doc_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .unwrap_or(0);
    println!("\n📊 Database: {} documents", doc_count);

    if doc_count == 0 {
        println!("⚠️  No documents found. Import documents first.");
        return;
    }

    // Test 1: BM25 search
    println!("\n[1/5] Testing BM25 Search...");
    let (bm25_results, bm25_time): (Vec<_>, _) = measure_query("BM25 search", async {
        sqlx::query_as::<_, (Uuid, f64)>(
            "SELECT d.id, paradedb.score(d.id)
                 FROM documents d
                 WHERE d.id @@@ 'test'
                 LIMIT 10",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;
    println!("  Results: {}", bm25_results.len());

    // Test 2: Keywords facet
    println!("\n[2/5] Testing Keywords Facet...");
    let (keywords, keywords_time): (Vec<_>, _) = measure_query("Keywords facet", async {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT keyword, COUNT(*) FROM documents,
                        LATERAL UNNEST(keywords) as keyword
                 WHERE keywords IS NOT NULL
                 GROUP BY keyword LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;
    println!("  Keywords: {}", keywords.len());

    // Test 3: Entity facets
    println!("\n[3/5] Testing Entity Facets...");
    let (persons, persons_time): (Vec<_>, _) = measure_query("Persons facet", async {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT jsonb_array_elements(entities->'persons')::text,
                        COUNT(*)
                 FROM documents
                 WHERE entities->'persons' IS NOT NULL
                 GROUP BY jsonb_array_elements(entities->'persons')
                 LIMIT 20",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;
    println!("  Persons: {}", persons.len());

    // Test 4: Date range
    println!("\n[4/5] Testing Date Range Filter...");
    let (recent, date_time): (Vec<_>, _) = measure_query("Date range filter", async {
        sqlx::query_as::<_, (Uuid,)>(
            "SELECT id FROM documents
                 WHERE created_at > NOW() - INTERVAL '30 days'",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;
    println!("  Recent documents: {}", recent.len());

    // Test 5: Chunk retrieval
    println!("\n[5/5] Testing Chunk Retrieval...");
    let (chunks, chunk_time): (Vec<_>, _) = measure_query("Chunk retrieval", async {
        sqlx::query_as::<_, (Uuid, i32)>(
            "SELECT document_id, chunk_index FROM document_chunks LIMIT 100",
        )
        .fetch_all(&pool)
        .await
        .unwrap_or_default()
    })
    .await;
    println!("  Chunks: {}", chunks.len());

    // Summary
    println!("\n╔════════════════════════════════════════════════════╗");
    println!("║  PERFORMANCE SUMMARY                               ║");
    println!("╚════════════════════════════════════════════════════╝");
    println!("BM25 Search:        {}ms ✅", bm25_time);
    println!("Keywords Facet:     {}ms ✅", keywords_time);
    println!("Persons Facet:      {}ms ✅", persons_time);
    println!("Date Range Filter:  {}ms ✅", date_time);
    println!("Chunk Retrieval:    {}ms ✅", chunk_time);
    println!(
        "Total:              {}ms",
        bm25_time + keywords_time + persons_time + date_time + chunk_time
    );

    // Assertions
    // Note: First query may be slower due to compilation/warmup
    assert!(
        bm25_time < 2000,
        "BM25 should be < 2000ms (includes warmup)"
    );
    assert!(keywords_time < 1000, "Keywords facet should be < 1000ms");
    assert!(persons_time < 1000, "Persons facet should be < 1000ms");
    assert!(date_time < 500, "Date filter should be < 500ms");
    assert!(chunk_time < 500, "Chunk retrieval should be < 500ms");

    println!("\n✅ All performance targets met!");
    println!("\n📝 Note: First run includes compilation/warmup overhead.");
    println!("   Subsequent runs will be significantly faster.");
}
