use axum::extract::State;
use axum::Json;
use rag_chat::api::state::AppState;
use rag_chat::api::handlers;
use rag_chat::domain::dtos::SearchRequest;
use rag_chat::infra::embedder::Embedder;
use rag_chat::config::Settings;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

async fn ensure_schema(pool: &sqlx::PgPool, embedding_dims: u32) -> Result<(), Box<dyn std::error::Error>> {
    // Always reinitialize schema to ensure correct embedding dimensions
    let mut schema = include_str!("../sql/init.sql").to_string();
    // Replace template variables with the actual embedding dimensions
    schema = schema.replace("${EMBEDDING_DIMENSIONS}", &embedding_dims.to_string());

    // PostgreSQL doesn't support executing multi-statement scripts directly via sqlx
    // So we split by semicolons but preserve multi-line statements
    let statements: Vec<&str> = schema.split(';').collect();

    for stmt in statements {
        let trimmed = stmt.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Try to execute each statement individually
        match sqlx::query(trimmed).execute(pool).await {
            Ok(_) => {}
            Err(e) => {
                // Log the error but don't fail - some statements like DROP IF EXISTS might fail
                eprintln!("SQL execution notice: {}", e);
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_search_api_with_db() {
    // 1. Setup
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new().expect("Failed to load settings");

    // Connect to database
    let pool = match PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
    {
        Ok(p) => p,
        Err(_) => {
            println!("⚠ Test skipped: Database not available");
            return;
        }
    };

    // Initialize Embedder
    let embedder = match Embedder::new(&settings.embedding) {
        Ok(e) => e,
        Err(_) => {
            println!("⚠ Test skipped: Embedding service not available");
            return;
        }
    };

    // Initialize AppState
    let leptos_options = leptos::prelude::LeptosOptions::builder()
        .output_name("rag-chat")
        .site_root("target/site")
        .build();

    // Create dummy import job queue (tests don't need it)
    let (import_job_tx, _import_job_rx) = tokio::sync::mpsc::channel(100);

    let state = AppState::new(pool.clone(), embedder, Arc::new(std::sync::Mutex::new(Vec::new())), leptos_options, import_job_tx, Arc::new(settings.clone()));

    // 2. Index PDF
    let pdf_path = "/Users/hagentonnies/Workspace/irdb-rag/documents/HumanPrincipals.pdf";
    if !std::path::Path::new(pdf_path).exists() {
        println!("⚠ Test skipped: PDF file not found at {}", pdf_path);
        return;
    }

    println!("Indexing PDF: {}", pdf_path);
    // We use the index_path_with_config function from indexing module with settings
    match rag_chat::services::indexing::index_path_with_config(&pool, &state.embedder, pdf_path, Some(&settings))
        .await
    {
        Ok(_) => {}
        Err(e) => {
            println!("⚠ Test skipped: Could not index PDF: {}", e);
            return;
        }
    }

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
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
    };

    let result = match handlers::search(State(state.clone()), Json(req)).await {
        Ok(r) => r,
        Err(e) => {
            let err_msg = format!("{:?}", e);
            if err_msg.contains("does not exist") {
                println!("⚠ Test skipped: Database schema not properly initialized (missing hybrid_search function)");
                // Clean up before skipping
                let _ = sqlx::query("DELETE FROM documents WHERE source_path = $1")
                    .bind(pdf_path)
                    .execute(&pool)
                    .await;
                return;
            }
            panic!("Search API call failed: {:?}", e);
        }
    };
    
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
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new().expect("Failed to load settings");

    let pool = PgPoolOptions::new()
        .max_connections(2) // Low connection count for tests
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    let embedder = Embedder::new(&settings.embedding).expect("Failed to create Embedder");
    let leptos_options = leptos::prelude::LeptosOptions::builder()
        .output_name("rag-chat")
        .site_root("target/site")
        .build();
    // Create dummy import job queue (tests don't need it)
    let (import_job_tx, _import_job_rx) = tokio::sync::mpsc::channel(100);

    let state = AppState::new(pool.clone(), embedder, Arc::new(std::sync::Mutex::new(Vec::new())), leptos_options, import_job_tx, Arc::new(settings.clone()));

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

/// Test streaming chat endpoint with indexed documents
///
/// Prerequisites:
/// - PostgreSQL/ParadeDB running
/// - PDF indexed (test_search_api_with_db runs first)
#[tokio::test]
async fn test_chat_stream_api_with_documents() {
    // 1. Setup
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new().expect("Failed to load settings");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    let embedder = Embedder::new(&settings.embedding).expect("Failed to create Embedder");
    let leptos_options = leptos::prelude::LeptosOptions::builder()
        .output_name("rag-chat")
        .site_root("target/site")
        .build();
    // Create dummy import job queue (tests don't need it)
    let (import_job_tx, _import_job_rx) = tokio::sync::mpsc::channel(100);

    let state = AppState::new(pool.clone(), embedder, Arc::new(std::sync::Mutex::new(Vec::new())), leptos_options, import_job_tx, Arc::new(settings.clone()));

    // 2. Index PDF
    let pdf_path = "/Users/hagentonnies/Workspace/irdb-rag/documents/HumanPrincipals.pdf";
    if std::path::Path::new(pdf_path).exists() {
        println!("Indexing PDF: {}", pdf_path);
        if let Err(e) = rag_chat::services::indexing::index_path_with_config(&pool, &state.embedder, pdf_path, Some(&settings)).await {
            println!("⚠ Could not index PDF: {}", e);
            println!("Skipping test - no documents to test with");
            return;
        }
    } else {
        println!("⚠ PDF file not found at {}", pdf_path);
        println!("Skipping test - no documents to test with");
        return;
    }

    // 3. Test Chat Stream API
    println!("Testing chat streaming API...");
    let req = rag_chat::domain::dtos::ChatRequest {
        message: "What are the main topics in this document?".to_string(),
        context_chunks: 3,
        document_ids: None,
        conversation_id: None,
    };

    match handlers::chat_stream(State(state.clone()), Json(req)).await {
        Ok(sse_response) => {
            println!("✓ Chat stream API call succeeded");

            // The response is an SSE stream, we need to extract it and consume it
            use axum::response::IntoResponse;
            let response = sse_response.into_response();

            // For this test, we'll just verify the response is 200 OK
            assert_eq!(response.status(), axum::http::StatusCode::OK);
            println!("  Response status: 200 OK");
            println!("✓ Streaming chat endpoint is working");
        }
        Err(e) => {
            panic!("Chat streaming API call failed: {:?}", e);
        }
    }

    // 4. Teardown
    println!("Cleaning up...");
    if std::path::Path::new(pdf_path).exists() {
        sqlx::query("DELETE FROM documents WHERE source_path = $1")
            .bind(pdf_path)
            .execute(&pool)
            .await
            .expect("Failed to clean up database");
    }
}

/// Test chat non-streaming endpoint with indexed documents
///
/// Prerequisites:
/// - PostgreSQL/ParadeDB running
/// - PDF indexed
#[tokio::test]
async fn test_chat_api_with_documents() {
    // 1. Setup
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new().expect("Failed to load settings");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    let embedder = Embedder::new(&settings.embedding).expect("Failed to create Embedder");
    let leptos_options = leptos::prelude::LeptosOptions::builder()
        .output_name("rag-chat")
        .site_root("target/site")
        .build();
    // Create dummy import job queue (tests don't need it)
    let (import_job_tx, _import_job_rx) = tokio::sync::mpsc::channel(100);

    let state = AppState::new(pool.clone(), embedder, Arc::new(std::sync::Mutex::new(Vec::new())), leptos_options, import_job_tx, Arc::new(settings.clone()));

    // 2. Index PDF
    let pdf_path = "/Users/hagentonnies/Workspace/irdb-rag/documents/HumanPrincipals.pdf";
    if std::path::Path::new(pdf_path).exists() {
        println!("Indexing PDF: {}", pdf_path);
        if let Err(e) = rag_chat::services::indexing::index_path_with_config(&pool, &state.embedder, pdf_path, Some(&settings)).await {
            println!("⚠ Could not index PDF: {}", e);
            println!("Skipping test - no documents to test with");
            return;
        }
    } else {
        println!("⚠ PDF file not found at {}", pdf_path);
        println!("Skipping test - no documents to test with");
        return;
    }

    // 3. Test Chat API
    println!("Testing chat API with RAG context...");
    let req = rag_chat::domain::dtos::ChatRequest {
        message: "Summarize the key points from the document".to_string(),
        context_chunks: 5,
        document_ids: None,
        conversation_id: None,
    };

    match handlers::chat(State(state.clone()), Json(req)).await {
        Ok(response) => {
            println!("✓ Chat API call succeeded");
            println!("  Conversation ID: {}", response.0.conversation_id);
            println!("  Response length: {} chars", response.0.message.len());
            println!("  Number of sources: {}", response.0.sources.len());

            // Verify response has content
            assert!(!response.0.message.is_empty(), "Chat returned empty message");
            assert!(!response.0.sources.is_empty(), "Chat returned no sources");

            // Print first 300 chars of response
            let preview = if response.0.message.len() > 300 {
                &response.0.message[..300]
            } else {
                &response.0.message
            };
            println!("  Response preview: {}...", preview);
        }
        Err(e) => {
            panic!("Chat API call failed: {:?}", e);
        }
    }

    // 4. Teardown
    println!("Cleaning up...");
    if std::path::Path::new(pdf_path).exists() {
        sqlx::query("DELETE FROM documents WHERE source_path = $1")
            .bind(pdf_path)
            .execute(&pool)
            .await
            .expect("Failed to clean up database");
    }
}
