use axum::extract::State;
use axum::Json;
use rag_chat::api::state::AppState;
use rag_chat::api::handlers;
use rag_chat::domain::dtos::SearchRequest;
use rag_chat::infra::embedder::Embedder;
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
    let leptos_options = leptos::prelude::LeptosOptions::builder()
        .output_name("rag-chat")
        .site_root("target/site")
        .build();
    // Create dummy import job queue (tests don't need it)
    let (import_job_tx, _import_job_rx) = tokio::sync::mpsc::channel(100);

    let state = AppState::new(pool.clone(), embedder, log_buffer, leptos_options, import_job_tx);

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
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: None,
            word_count_max: None,
        };

        let result = handlers::search(State(state.clone()), Json(req)).await;
        
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
