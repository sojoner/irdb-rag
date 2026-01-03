use axum::extract::State;
use axum::Json;
use rag_chat::api::{self, AppState, SearchRequest};
use rag_chat::indexer::Embedder;
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn test_search_syntax_edge_cases() {
    // Initialize tracing
    let _ = tracing_subscriber::fmt()
        .with_env_filter("info")
        .try_init();

    // 1. Setup
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in tests/test.env");

    let pool = PgPoolOptions::new()
        .max_connections(5)
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
    ];

    for query in test_queries {
        println!("Testing query: '{}'", query);
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
            Ok(_) => println!("Query '{}' succeeded", query),
            Err(e) => {
                println!("Query '{}' failed: {:?}", query, e);
                // We want to fail the test if we hit the specific parsing error
                let error_msg = format!("{:?}", e);
                if error_msg.contains("could not parse query string") {
                    panic!("Query '{}' caused parsing error: {}", query, error_msg);
                }
            }
        }
    }
}
