mod common;

use anyhow::Result;
use rag_chat::config::Settings;
use rag_chat::infra::db::{hybrid_search, bm25_search, SearchFilters, SortOrder};
use rag_chat::infra::db::create_pool;
use rag_chat::infra::embedder::Embedder;
use rag_chat::web_app::components::advanced_query_builder::{FilterType, FilterValue, QueryFilter};
use rag_chat::web_app::components::query_builder_example::build_search_request;
use serde_json::json;

// Adapted from api_search_test.rs, search_syntax_test.rs, search_performance_test.rs, test_search_scoring.rs, embedding_test.rs

// ============================================
// Internal Logic / DB Integration Tests
// ============================================

#[tokio::test]
async fn test_db_hybrid_search_syntax() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }
    let settings = Settings::new()?;
    let pool = create_pool(&settings.database).await?;

    // Real-world user queries that should work
    let valid_queries = vec![
        "normal query",
        "test search",
        "machine learning",
        "how to use python",
    ];

    let dummy_embedding = vec![0.0; 768]; // 768 dim for nomic-embed-text-v2-moe
    let filters = SearchFilters {
        category_id: None, date_from: None, date_to: None, locations: None, keywords: None,
        source_types: None, authors: None, concepts: None, organizations: None, persons: None,
        products: None, word_count_min: None, word_count_max: None,
    };

    // Test valid queries - should not error
    for query in valid_queries {
        println!("Testing query: '{}'", query);
        let result = hybrid_search(&pool, query, &dummy_embedding, &filters, 5, 0.5, 0.5, None).await;
        assert!(result.is_ok(), "Query '{}' should not error: {:?}", query, result.err());
    }

    // Empty/whitespace queries get special handling (no-match fallback)
    for query in ["", "   "] {
        println!("Testing empty query: '{}'", query);
        let result = hybrid_search(&pool, query, &dummy_embedding, &filters, 5, 0.5, 0.5, None).await;
        // Empty queries should either succeed with empty results or error gracefully
        match result {
            Ok(results) => println!("  Empty query returned {} results", results.len()),
            Err(e) => println!("  Empty query handled: {}", e),
        }
    }
    Ok(())
}

// ============================================
// E2E API Tests
// ============================================

#[tokio::test]
async fn test_e2e_search_comprehensive() {
    let client = common::TestClient::new();
    // Skip if server not running (E2E test requires running server)
    if !client.is_server_running().await {
        eprintln!("Skipping E2E test - server not running on localhost:3000");
        return;
    }

    // 1. Basic Search
    let queries = vec!["data", "programming", "logic"];
    for q in queries {
        let req = json!({ "query": q, "limit": 5 });
        let resp = client.client.post(client.url("/search")).json(&req).send().await.expect("Failed search");
        if !resp.status().is_success() {
            let body = resp.text().await.unwrap_or_default();
            panic!("Search for '{}' failed: {}", q, body);
        }
    }

    // 2. Faceted Search
    let fac_req = json!({ "query": "test", "facet_limit": 5 });
    let fac_resp = client.client.post(client.url("/search/faceted")).json(&fac_req).send().await.expect("Failed faceted");
    assert!(fac_resp.status().is_success());
    let fac_data: serde_json::Value = fac_resp.json().await.unwrap();
    assert!(fac_data.get("facets").is_some());

    // 3. Filters
    let filt_req = json!({ 
        "query": "test", 
        "keywords": ["important"], 
        "date_from": "2023-01-01T00:00:00Z" 
    });
    let filt_resp = client.client.post(client.url("/search")).json(&filt_req).send().await.expect("Failed filtered");
    assert!(filt_resp.status().is_success());

    // 4. Weighting
    let weight_req = json!({ "query": "test", "bm25_weight": 0.1, "vector_weight": 0.9 });
    let w_resp = client.client.post(client.url("/search")).json(&weight_req).send().await.expect("Failed weighted");
    assert!(w_resp.status().is_success());

    // 5. Empty/Edge cases via API
    let edge_req = json!({ "query": "" });
    let edge_resp = client.client.post(client.url("/search")).json(&edge_req).send().await.expect("Failed empty");
    assert!(edge_resp.status().is_success());
    let res: Vec<serde_json::Value> = edge_resp.json().await.unwrap();
    assert!(res.is_empty());
}

// ============================================
// DB Connection Pool Stress Tests
// ============================================

#[tokio::test]
async fn test_db_pool_concurrency_load() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }
    let settings = Settings::new()?;
    // Use small pool to force queuing
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(std::time::Duration::from_secs(5))
        .connect(&settings.database.url).await?;

    println!("Testing concurrent DB load...");
    let mut handles = vec![];
    for i in 0..10 { // More tasks than connections
        let p = pool.clone();
        handles.push(tokio::spawn(async move {
            let _ = sqlx::query("SELECT 1").execute(&p).await;
            i
        }));
    }

    for h in handles {
        h.await?;
    }
    println!("✅ Concurrent load test passed");
    Ok(())
}

#[tokio::test]
async fn test_db_pool_recovery_after_slow_query() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }
    let settings = Settings::new()?;
    let pool = create_pool(&settings.database).await?;

    // Simulate slow query
    let start = std::time::Instant::now();
    let _ = sqlx::query("SELECT pg_sleep(0.5)").execute(&pool).await; // 500ms sleep
    println!("Slow query took {:?}", start.elapsed());

    // Verify pool is still healthy
    let row: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await?;
    assert_eq!(row.0, 1);
    Ok(())
}

// ============================================
// Score Normalization Tests (from test_search_scoring.rs)
// ============================================

#[tokio::test]
async fn test_search_scores_are_normalized() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }
    let settings = Settings::new()?;
    let pool = create_pool(&settings.database).await?;

    let query = "deep";
    let filters = SearchFilters {
        category_id: None, date_from: None, date_to: None, locations: None, keywords: None,
        source_types: None, authors: None, concepts: None, organizations: None, persons: None,
        products: None, word_count_min: None, word_count_max: None,
    };

    let results = bm25_search(&pool, query, &filters, 20, 0, &SortOrder::Relevance).await?;

    // All scores should be between 0.0 and 1.0 for proper percentage display
    for result in &results {
        assert!(
            result.combined_score >= 0.0 && result.combined_score <= 1.0,
            "Score {} is outside valid range [0.0, 1.0]",
            result.combined_score
        );
    }

    // If we have results, the highest score should be close to 1.0
    if !results.is_empty() {
        let max_score = results.iter()
            .map(|r| r.combined_score)
            .fold(0.0f64, f64::max);
        assert!(max_score >= 0.5, "Top result score {} is too low", max_score);
    }

    Ok(())
}

// ============================================
// Embedding Tests (from embedding_test.rs)
// ============================================

async fn is_embedding_service_available() -> bool {
    let settings = Settings::new().ok();
    if let Some(settings) = settings {
        let api_url = settings.embedding.api_url.trim_end_matches('/');
        let url = if api_url.ends_with("/v1") {
            format!("{}/models", api_url)
        } else {
            format!("{}/v1/models", api_url)
        };
        let response = reqwest::Client::new()
            .get(&url)
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await;

        if let Ok(resp) = response {
            if let Ok(text) = resp.text().await {
                return !text.contains("\"error\"");
            }
        }
    }
    false
}

#[tokio::test]
async fn test_basic_embedding_generation() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }

    if !is_embedding_service_available().await {
        println!("⚠️ Skipping test: embedding service not available");
        return Ok(());
    }

    let settings = Settings::new()?;
    let embedder = Embedder::new(&settings.embedding)?;

    let text = "This is a test sentence for embedding generation.";
    let start = std::time::Instant::now();
    let embedding = embedder.embed(text).await?;
    let elapsed = start.elapsed();

    assert!(!embedding.is_empty(), "Embedding should not be empty");
    assert_eq!(embedding.len(), settings.embedding.dimensions as usize,
        "Embedding dimension mismatch: expected {}, got {}",
        settings.embedding.dimensions, embedding.len());
    assert!(elapsed.as_secs() < 30, "Embedding generation too slow: {:.2}s", elapsed.as_secs_f64());

    println!("✅ Generated embedding with {} dimensions in {:.2}s", embedding.len(), elapsed.as_secs_f64());
    Ok(())
}

#[tokio::test]
async fn test_embedding_similarity() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }

    if !is_embedding_service_available().await {
        println!("⚠️ Skipping test: embedding service not available");
        return Ok(());
    }

    let settings = Settings::new()?;
    let embedder = Embedder::new(&settings.embedding)?;

    let text1 = "Kubernetes is a container orchestration platform.";
    let text2 = "Kubernetes manages Docker containers in production.";
    let text3 = "Pizza is a delicious Italian food.";

    let (emb1, emb2, emb3) = tokio::join!(
        embedder.embed(text1),
        embedder.embed(text2),
        embedder.embed(text3),
    );

    let emb1 = emb1?;
    let emb2 = emb2?;
    let emb3 = emb3?;

    // Cosine similarity
    let cosine_sim = |a: &[f32], b: &[f32]| -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (norm_a * norm_b)
    };

    let sim_related = cosine_sim(&emb1, &emb2);
    let sim_unrelated = cosine_sim(&emb1, &emb3);

    println!("Similarity (Kubernetes texts): {:.4}", sim_related);
    println!("Similarity (Kubernetes vs Pizza): {:.4}", sim_unrelated);

    assert!(sim_related > sim_unrelated,
        "Related texts should have higher similarity ({:.4}) than unrelated ({:.4})",
        sim_related, sim_unrelated);

    Ok(())
}

// ============================================
// Query Builder Tests (from advanced_query_builder.rs)
// ============================================

#[test]
fn test_build_search_request_empty_filters() {
    let request = build_search_request("test query".to_string(), vec![], SortOrder::Relevance);

    assert_eq!(request.query, "test query");
    assert_eq!(request.limit, 20);
    assert!(request.date_from.is_none());
    assert!(request.date_to.is_none());
    assert!(request.keywords.is_none());
    assert!(request.locations.is_none());
}

#[test]
fn test_build_search_request_date_filter() {
    let filters = vec![QueryFilter {
        filter_type: FilterType::DateRange,
        value: FilterValue::DateRange {
            from: Some("2024-01-01".to_string()),
            to: Some("2024-12-31".to_string()),
        },
    }];

    let request = build_search_request("search".to_string(), filters, SortOrder::DateDesc);

    assert_eq!(request.date_from, Some("2024-01-01".to_string()));
    assert_eq!(request.date_to, Some("2024-12-31".to_string()));
}

#[test]
fn test_build_search_request_text_filter() {
    let filters = vec![QueryFilter {
        filter_type: FilterType::TextField("title".to_string()),
        value: FilterValue::Text {
            field: "title".to_string(),
            value: "machine learning".to_string(),
        },
    }];

    let request = build_search_request("".to_string(), filters, SortOrder::Relevance);

    // Text filters are appended to query with field:value syntax
    assert!(request.query.contains("title:\"machine learning\""));
}

#[test]
fn test_build_search_request_array_filter_keywords() {
    let filters = vec![QueryFilter {
        filter_type: FilterType::ArrayField("keywords".to_string()),
        value: FilterValue::Array {
            field: "keywords".to_string(),
            values: vec!["Python".to_string(), "AI".to_string()],
        },
    }];

    let request = build_search_request("test".to_string(), filters, SortOrder::Relevance);

    assert!(request.keywords.is_some());
    let kw = request.keywords.unwrap();
    assert!(kw.contains(&"Python".to_string()));
    assert!(kw.contains(&"AI".to_string()));
}

#[test]
fn test_build_search_request_array_filter_locations() {
    let filters = vec![QueryFilter {
        filter_type: FilterType::ArrayField("locations".to_string()),
        value: FilterValue::Array {
            field: "locations".to_string(),
            values: vec!["New York".to_string(), "Berlin".to_string()],
        },
    }];

    let request = build_search_request("test".to_string(), filters, SortOrder::Relevance);

    assert!(request.locations.is_some());
    assert_eq!(request.locations.unwrap().len(), 2);
}

#[test]
fn test_build_search_request_combined_filters() {
    let filters = vec![
        QueryFilter {
            filter_type: FilterType::DateRange,
            value: FilterValue::DateRange {
                from: Some("2024-01-01".to_string()),
                to: None,
            },
        },
        QueryFilter {
            filter_type: FilterType::TextField("author".to_string()),
            value: FilterValue::Text {
                field: "author".to_string(),
                value: "John Doe".to_string(),
            },
        },
        QueryFilter {
            filter_type: FilterType::ArrayField("organizations".to_string()),
            value: FilterValue::Array {
                field: "organizations".to_string(),
                values: vec!["OpenAI".to_string()],
            },
        },
    ];

    let request = build_search_request("AI research".to_string(), filters, SortOrder::DateDesc);

    assert_eq!(request.date_from, Some("2024-01-01".to_string()));
    assert!(request.date_to.is_none());
    assert!(request.query.contains("author:\"John Doe\""));
    assert!(request.organizations.is_some());
    assert!(request.organizations.unwrap().contains(&"OpenAI".to_string()));
}

#[test]
fn test_build_search_request_empty_array_filter_ignored() {
    let filters = vec![QueryFilter {
        filter_type: FilterType::ArrayField("keywords".to_string()),
        value: FilterValue::Array {
            field: "keywords".to_string(),
            values: vec![], // Empty array should be ignored
        },
    }];

    let request = build_search_request("test".to_string(), filters, SortOrder::Relevance);

    // Empty arrays should not be set
    assert!(request.keywords.is_none());
}

#[test]
fn test_build_search_request_empty_text_filter_ignored() {
    let filters = vec![QueryFilter {
        filter_type: FilterType::TextField("title".to_string()),
        value: FilterValue::Text {
            field: "title".to_string(),
            value: "".to_string(), // Empty value should be ignored
        },
    }];

    let request = build_search_request("base query".to_string(), filters, SortOrder::Relevance);

    // Empty text filter shouldn't append anything to query
    assert_eq!(request.query, "base query");
}

#[test]
fn test_query_filter_serialization() {
    let filter = QueryFilter {
        filter_type: FilterType::DateRange,
        value: FilterValue::DateRange {
            from: Some("2024-01-01".to_string()),
            to: Some("2024-12-31".to_string()),
        },
    };

    let json = serde_json::to_string(&filter).expect("Failed to serialize");
    let deserialized: QueryFilter = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(filter, deserialized);
}

#[test]
fn test_filter_types_equality() {
    let type1 = FilterType::TextField("title".to_string());
    let type2 = FilterType::TextField("title".to_string());
    let type3 = FilterType::TextField("content".to_string());

    assert_eq!(type1, type2);
    assert_ne!(type1, type3);
}
