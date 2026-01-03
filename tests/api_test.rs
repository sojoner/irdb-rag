use axum::extract::State;
use axum::Json;
use rag_chat::api::{self, AppState, SearchRequest};
use rag_chat::indexer::Embedder;
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_search_api_with_db() {
    // 1. Setup
    // Clear potential conflicting env vars and load test config
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in tests/test.env");

    // Connect to database
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Initialize Embedder
    let embedder = Embedder::new().expect("Failed to create Embedder");
    
    // Initialize AppState
    let log_buffer = Arc::new(Mutex::new(Vec::new()));
    let state = AppState::new(pool.clone(), embedder, log_buffer);

    // 2. Index PDF
    let pdf_path = "/Users/hagentonnies/Workspace/irdb-rag/documents/HumanPrincipals.pdf";
    assert!(std::path::Path::new(pdf_path).exists(), "PDF file not found at {}", pdf_path);

    println!("Indexing PDF: {}", pdf_path);
    // We use the index_path function from indexer module
    rag_chat::indexer::index_path(&pool, &state.embedder, pdf_path)
        .await
        .expect("Failed to index PDF");

    // 3. Test Search
    println!("Testing search...");
    let req = SearchRequest {
        query: "Human Principals".to_string(), 
        limit: 5,
        bm25_weight: 0.5,
        vector_weight: 0.5,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
    };

    let result = api::search(State(state.clone()), Json(req))
        .await
        .expect("Search API call failed");
    
    let search_results = result.0;
    println!("Found {} results", search_results.len());

    // Assertions
    assert!(!search_results.is_empty(), "Expected search results, found none");
    
    // Check if the indexed document is in the results
    // The title might be derived from filename "HumanPrincipals"
    let found = search_results.iter().any(|r| r.title.contains("HumanPrincipals") || r.title.contains("Human Principals"));
    
    // Print titles for debugging if not found
    if !found {
        println!("Results found:");
        for r in &search_results {
            println!("- {}", r.title);
        }
    }
    
    assert!(found, "Indexed document not found in search results");

    // 4. Teardown
    println!("Tearing down...");
    sqlx::query("DELETE FROM documents WHERE source_path = $1")
        .bind(pdf_path)
        .execute(&pool)
        .await
        .expect("Failed to clean up database");
}

#[tokio::test]
async fn test_search_api_syntax_edge_cases() {
    // 1. Setup
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in tests/test.env");

    let pool = PgPoolOptions::new()
        .max_connections(2) // Low connection count for tests
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    let embedder = Embedder::new().expect("Failed to create Embedder");
    let log_buffer = Arc::new(Mutex::new(Vec::new()));
    let state = AppState::new(pool.clone(), embedder, log_buffer);

    // 2. Test Cases
    let test_queries = vec![
        "id:()",
        "id: ()",
        "id:( )",
        "*",
        "id:(*)",
        "",
        "   ",
        "normal query",
    ];

    for query in test_queries {
        println!("Testing API query: '{}'", query);
        let req = SearchRequest {
            query: query.to_string(),
            limit: 5,
            bm25_weight: 0.5,
            vector_weight: 0.5,
            category_id: None,
            date_from: None,
            date_to: None,
            locations: None,
            keywords: None,
        };

        let result = api::search(State(state.clone()), Json(req)).await;
        
        match result {
            Ok(_) => println!("✅ API Query '{}' succeeded", query),
            Err(e) => {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("could not parse query string") {
                    panic!("❌ API Query '{}' caused parsing error: {}", query, error_msg);
                } else {
                    println!("⚠️ API Query '{}' failed with other error: {:?}", query, e);
                }
            }
        }
    }
}
