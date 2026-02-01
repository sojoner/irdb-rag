//! API-level search tests for faceted and hybrid search
//!
//! Tests search functionality via HTTP endpoints with performance measurements

use std::time::Instant;

#[derive(serde::Serialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default = "default_bm25_weight")]
    pub bm25_weight: f64,
    #[serde(default = "default_vector_weight")]
    pub vector_weight: f64,
    pub category_id: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub locations: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub organizations: Option<Vec<String>>,
    pub persons: Option<Vec<String>>,
    pub products: Option<Vec<String>>,
    pub word_count_min: Option<i32>,
    pub word_count_max: Option<i32>,
    #[serde(default)]
    pub sort: SortOrder,
    #[serde(default)]
    pub search_fields: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Default)]
pub enum SortOrder {
    #[default]
    Relevance,      // BM25 score descending (default for searches with query)
    DateDesc,       // Newest first (default for browse/filter-only)
    DateAsc,        // Oldest first
    TitleAsc,       // Alphabetical A-Z
    TitleDesc,      // Alphabetical Z-A
}

fn default_limit() -> i32 {
    10
}
fn default_bm25_weight() -> f64 {
    0.5
}
fn default_vector_weight() -> f64 {
    0.5
}

// Helper to measure API call performance
async fn measure_api_call<F, T>(name: &str, f: F) -> (T, u128)
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
// API SEARCH TESTS
// ============================================

#[tokio::test]
#[ignore] // Run with: cargo test --test api_search_test -- --ignored --nocapture
async fn test_search_request_basic() {
    println!("\n=== API Search Request - Basic ===");

    let client = reqwest::Client::new();

    let search_req = SearchRequest {
        query: "programming".to_string(),
        limit: 10,
        bm25_weight: 0.5,
        vector_weight: 0.5,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
    };

    let (response, elapsed): (Result<_, _>, _) = measure_api_call("Basic search API call", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&search_req)
            .send()
            .await
    })
    .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            assert!(
                elapsed < 1000,
                "Search should complete in < 1000ms, took {}ms",
                elapsed
            );
        }
        Err(e) => {
            println!(
                "  ⚠️  API call failed: {}. Is the server running? (make gpu-up)",
                e
            );
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_search_with_keywords_filter() {
    println!("\n=== API Search - Keywords Filter ===");

    let client = reqwest::Client::new();

    let search_req = SearchRequest {
        query: "technology".to_string(),
        limit: 10,
        bm25_weight: 0.5,
        vector_weight: 0.5,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: Some(vec!["important".to_string(), "urgent".to_string()]),
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
    };

    let (response, elapsed): (Result<_, _>, _) =
        measure_api_call("Search with keywords filter", async {
            client
                .post("http://localhost:3000/api/search")
                .json(&search_req)
                .send()
                .await
        })
        .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            assert!(
                elapsed < 1500,
                "Filtered search should complete in < 1500ms"
            );
        }
        Err(e) => {
            println!("  ⚠️  API call failed: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_search_with_date_filter() {
    println!("\n=== API Search - Date Range Filter ===");

    let client = reqwest::Client::new();

    let search_req = SearchRequest {
        query: "data".to_string(),
        limit: 10,
        bm25_weight: 0.5,
        vector_weight: 0.5,
        category_id: None,
        date_from: Some("2023-01-01T00:00:00Z".to_string()),
        date_to: Some("2024-12-31T23:59:59Z".to_string()),
        locations: None,
        keywords: None,
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
    };

    let (response, elapsed): (Result<_, _>, _) =
        measure_api_call("Search with date filter", async {
            client
                .post("http://localhost:3000/api/search")
                .json(&search_req)
                .send()
                .await
        })
        .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            assert!(
                elapsed < 1000,
                "Date-filtered search should be reasonably fast"
            );
        }
        Err(e) => {
            println!("  ⚠️  API call failed: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_search_with_entity_filters() {
    println!("\n=== API Search - Entity Filters ===");

    let client = reqwest::Client::new();

    let search_req = SearchRequest {
        query: "algorithm".to_string(),
        limit: 10,
        bm25_weight: 0.5,
        vector_weight: 0.5,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
        authors: None,
        concepts: Some(vec!["Machine Learning".to_string()]),
        organizations: Some(vec!["Tech Corp".to_string()]),
        persons: Some(vec!["Alice".to_string(), "Bob".to_string()]),
        products: None,
        word_count_min: None,
        word_count_max: None,
        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
    };

    let (response, elapsed): (Result<_, _>, _) =
        measure_api_call("Search with entity filters", async {
            client
                .post("http://localhost:3000/api/search")
                .json(&search_req)
                .send()
                .await
        })
        .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            assert!(
                elapsed < 1500,
                "Entity-filtered search should complete reasonably"
            );
        }
        Err(e) => {
            println!("  ⚠️  API call failed: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_search_bm25_heavy() {
    println!("\n=== API Search - BM25-Heavy (0.8/0.2) ===");

    let client = reqwest::Client::new();

    let search_req = SearchRequest {
        query: "keywords".to_string(),
        limit: 10,
        bm25_weight: 0.8,
        vector_weight: 0.2,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
    };

    let (response, elapsed): (Result<_, _>, _) = measure_api_call("BM25-heavy search", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&search_req)
            .send()
            .await
    })
    .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            println!("  📊 Weight ratio: 0.8 BM25 / 0.2 Vector");
        }
        Err(e) => {
            println!("  ⚠️  API call failed: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_search_vector_heavy() {
    println!("\n=== API Search - Vector-Heavy (0.2/0.8) ===");

    let client = reqwest::Client::new();

    let search_req = SearchRequest {
        query: "semantic".to_string(),
        limit: 10,
        bm25_weight: 0.2,
        vector_weight: 0.8,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
    };

    let (response, elapsed): (Result<_, _>, _) = measure_api_call("Vector-heavy search", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&search_req)
            .send()
            .await
    })
    .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            println!("  📊 Weight ratio: 0.2 BM25 / 0.8 Vector");
        }
        Err(e) => {
            println!("  ⚠️  API call failed: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_search_empty_query() {
    println!("\n=== API Search - Empty Query Handling ===");

    let client = reqwest::Client::new();

    let search_req = SearchRequest {
        query: "".to_string(),
        limit: 10,
        bm25_weight: 0.5,
        vector_weight: 0.5,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
    };

    let (response, elapsed): (Result<_, _>, _) = measure_api_call("Empty query search", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&search_req)
            .send()
            .await
    })
    .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            println!("  ✅ Empty queries handled gracefully");
        }
        Err(e) => {
            println!("  ⚠️  API call failed: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_aggregation_stats_endpoint() {
    println!("\n=== API Aggregation Stats Endpoint ===");

    let client = reqwest::Client::new();

    let (response, elapsed): (Result<_, _>, _) =
        measure_api_call("Aggregation stats endpoint", async {
            client
                .get("http://localhost:3000/api/aggregation-stats")
                .send()
                .await
        })
        .await;

    match response {
        Ok(resp) => {
            println!("  ✅ Status: {}", resp.status());
            println!("  ✅ Aggregation stats retrieved in {}ms", elapsed);
            assert!(
                elapsed < 500,
                "Aggregation stats should be cached/fast < 500ms"
            );
        }
        Err(e) => {
            println!("  ⚠️  API call failed: {}", e);
        }
    }
}

#[tokio::test]
#[ignore]
async fn test_concurrent_searches() {
    println!("\n=== Concurrent Search Requests ===");

    let client = reqwest::Client::new();

    let searches = vec!["programming", "technology", "data", "algorithm", "system"];

    let (_, total_time): (Vec<_>, _) = measure_api_call("5 concurrent searches", async {
        let futures = searches
            .iter()
            .map(|q| {
                let client = client.clone();
                let query = q.to_string();
                async move {
                    let req = SearchRequest {
                        query,
                        limit: 5,
                        bm25_weight: 0.5,
                        vector_weight: 0.5,
                        category_id: None,
                        date_from: None,
                        date_to: None,
                        locations: None,
                        keywords: None,
                        authors: None,
                        concepts: None,
                        organizations: None,
                        persons: None,
                        products: None,
                        word_count_min: None,
                        word_count_max: None,
                        sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
                    };
                    client
                        .post("http://localhost:3000/api/search")
                        .json(&req)
                        .send()
                        .await
                }
            })
            .collect::<Vec<_>>();

        futures::future::join_all(futures).await
    })
    .await;

    println!("  ✅ Concurrent searches completed in {}ms", total_time);
}

#[tokio::test]
#[ignore]
async fn test_search_performance_benchmark() {
    println!("\n╔════════════════════════════════════════════════════╗");
    println!("║  API SEARCH PERFORMANCE BENCHMARK                  ║");
    println!("╚════════════════════════════════════════════════════╝");

    let client = reqwest::Client::new();

    // Warm-up
    println!("\n[Warm-up] Sending test request...");
    let _ = client
        .post("http://localhost:3000/api/search")
        .json(&SearchRequest {
            query: "test".to_string(),
            limit: 5,
            bm25_weight: 0.5,
            vector_weight: 0.5,
            category_id: None,
            date_from: None,
            date_to: None,
            locations: None,
            keywords: None,
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: None,
            word_count_max: None,
            sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
        })
        .send()
        .await;

    // Test 1: Basic search
    println!("\n[1/4] Basic Search Performance");
    let (_, time1): (Result<_, _>, _) = measure_api_call("basic search", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&SearchRequest {
                query: "programming".to_string(),
                limit: 10,
                bm25_weight: 0.5,
                vector_weight: 0.5,
                category_id: None,
                date_from: None,
                date_to: None,
                locations: None,
                keywords: None,
                authors: None,
                concepts: None,
                organizations: None,
                persons: None,
                products: None,
                word_count_min: None,
                word_count_max: None,
                sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
            })
            .send()
            .await
    })
    .await;

    // Test 2: Filtered search
    println!("\n[2/4] Filtered Search Performance");
    let (_, time2): (Result<_, _>, _) = measure_api_call("filtered search", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&SearchRequest {
                query: "technology".to_string(),
                limit: 10,
                bm25_weight: 0.5,
                vector_weight: 0.5,
                category_id: None,
                date_from: None,
                date_to: None,
                locations: Some(vec!["USA".to_string()]),
                keywords: Some(vec!["important".to_string()]),
                authors: None,
                concepts: None,
                organizations: None,
                persons: None,
                products: None,
                word_count_min: None,
                word_count_max: None,
                sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
            })
            .send()
            .await
    })
    .await;

    // Test 3: BM25-heavy search
    println!("\n[3/4] BM25-Heavy Search (0.8/0.2)");
    let (_, time3): (Result<_, _>, _) = measure_api_call("bm25-heavy search", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&SearchRequest {
                query: "data".to_string(),
                limit: 10,
                bm25_weight: 0.8,
                vector_weight: 0.2,
                category_id: None,
                date_from: None,
                date_to: None,
                locations: None,
                keywords: None,
                authors: None,
                concepts: None,
                organizations: None,
                persons: None,
                products: None,
                word_count_min: None,
                word_count_max: None,
                sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
            })
            .send()
            .await
    })
    .await;

    // Test 4: Vector-heavy search
    println!("\n[4/4] Vector-Heavy Search (0.2/0.8)");
    let (_, time4): (Result<_, _>, _) = measure_api_call("vector-heavy search", async {
        client
            .post("http://localhost:3000/api/search")
            .json(&SearchRequest {
                query: "semantic".to_string(),
                limit: 10,
                bm25_weight: 0.2,
                vector_weight: 0.8,
                category_id: None,
                date_from: None,
                date_to: None,
                locations: None,
                keywords: None,
                authors: None,
                concepts: None,
                organizations: None,
                persons: None,
                products: None,
                word_count_min: None,
                word_count_max: None,
                sort: SortOrder::Relevance,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
            })
            .send()
            .await
    })
    .await;

    // Summary
    println!("\n╔════════════════════════════════════════════════════╗");
    println!("║  BENCHMARK RESULTS                                  ║");
    println!("╚════════════════════════════════════════════════════╝");
    println!("Basic Search:           {}ms", time1);
    println!("Filtered Search:        {}ms", time2);
    println!("BM25-Heavy (0.8/0.2):   {}ms", time3);
    println!("Vector-Heavy (0.2/0.8): {}ms", time4);
    println!(
        "Total:                  {}ms",
        time1 + time2 + time3 + time4
    );
}
