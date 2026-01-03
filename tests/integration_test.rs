/// Simple integration test: Database connectivity
///
/// Prerequisites:
/// - PostgreSQL/ParadeDB running (docker compose up -d)
/// - DATABASE_URL set in .env or environment

#[tokio::test]

async fn test_database_connection() {
    // Clear potential conflicting env vars and load test config
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    // Connect to database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Simple check: database is accessible
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("Failed to query documents table");

    println!("✓ Database connected. Current document count: {}", result.0);
}

/// Test indexing a local PDF file
///
/// This test should FAIL until the indexing functionality is properly implemented.
///
/// Prerequisites:
/// - PostgreSQL/ParadeDB running (docker compose up -d)
/// - PDF file exists at data/2024-ProjectToProduct.pdf
#[tokio::test]

async fn test_index_local_pdf() {
    // Clear potential conflicting env vars and load test config
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pdf_path = "documents/HumanPrincipals.pdf";

    // Connect to database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Get initial document count
    let initial_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("Failed to query documents table");

    println!("Initial document count: {}", initial_count.0);

    // Index the PDF file using the CLI
    println!("Indexing PDF at {}", pdf_path);
    let status = std::process::Command::new("cargo")
        .args(&["run", "--", "index", "--path", pdf_path])
        .current_dir(std::env::current_dir().expect("Failed to get current directory"))
        .status()
        .expect("Failed to execute cargo run");

    assert!(status.success(), "Indexing command failed");

    // Verify the PDF was indexed
    let final_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("Failed to query documents table");

    println!("Final document count: {}", final_count.0);

    // This assertion will fail until we implement indexing
    assert!(
        final_count.0 > initial_count.0,
        "Expected document count to increase after indexing. Initial: {}, Final: {}",
        initial_count.0,
        final_count.0
    );

    // Verify the specific document exists
    #[derive(sqlx::FromRow)]
    struct Document {
        title: String,
        source_path: Option<String>,
    }

    let doc: Option<Document> = sqlx::query_as(
        "SELECT title, source_path FROM documents WHERE source_path LIKE '%HumanPrincipals.pdf%' LIMIT 1"
    )
    .fetch_optional(&pool)
    .await
    .expect("Failed to query for indexed document");

    match doc {
        Some(d) => {
            println!("✓ Found indexed document: '{}'", d.title);
            println!("  Source: {:?}", d.source_path);
        }
        None => {
            panic!("Document 'HumanPrincipals.pdf' was not found in database after indexing");
        }
    }
}

/// Test Docling pipeline integration
///
/// Validates that Docling API can process a PDF and return structured content.
///
/// Prerequisites:
/// - Docling service running (docker compose up -d)
/// - PDF file exists at data/2024-ProjectToProduct.pdf
#[tokio::test]

async fn test_docling_pipeline() {

    let pdf_path = "documents/HumanPrincipals.pdf";
    let docling_url = "http://localhost:5001";

    // Step 1: Verify PDF exists
    assert!(
        std::path::Path::new(pdf_path).exists(),
        "Test PDF not found at {}", pdf_path
    );
    println!("✓ PDF file found: {}", pdf_path);

    // Step 2: Check Docling health
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(300)) // 5 minute timeout for large PDFs
        .build()
        .expect("Failed to build HTTP client");
    let health_response = client
        .get(&format!("{}/health", docling_url))
        .send()
        .await
        .expect("Failed to connect to Docling service - is it running?");

    assert!(
        health_response.status().is_success(),
        "Docling health check failed"
    );
    println!("✓ Docling service is healthy");

    // Step 3: Upload PDF to Docling for processing
    let file_bytes = std::fs::read(pdf_path).expect("Failed to read PDF file");

    let form = reqwest::multipart::Form::new()
        .part(
            "files",  // API expects "files" (plural)
            reqwest::multipart::Part::bytes(file_bytes)
                .file_name("HumanPrincipals.pdf")
                .mime_str("application/pdf")
                .expect("Invalid MIME type"),
        );

    println!("Sending PDF to Docling for processing...");
    let convert_response = client
        .post(&format!("{}/v1/convert/file", docling_url))
        .multipart(form)
        .send()
        .await
        .expect("Failed to send PDF to Docling");

    assert!(
        convert_response.status().is_success(),
        "Docling conversion failed with status: {}",
        convert_response.status()
    );

    // Step 4: Parse response and validate structure
    let result: serde_json::Value = convert_response
        .json()
        .await
        .expect("Failed to parse Docling response");

    println!("✓ Docling processed PDF successfully");

    // Validate response structure
    assert!(
        result.get("document").is_some(),
        "Docling response missing 'document' field"
    );
    
    let document = &result["document"];
    assert!(
        document.get("md_content").is_some(),
        "Docling response missing 'md_content' field in document object"
    );
    println!("✓ Markdown content extracted");

    let markdown = document["md_content"].as_str().expect("Markdown is not a string");
    assert!(
        !markdown.is_empty(),
        "Docling returned empty markdown"
    );
    println!("  Markdown length: {} chars", markdown.len());

    // Check for metadata
    if let Some(metadata) = result.get("metadata") {
        println!("✓ Metadata extracted: {}", serde_json::to_string_pretty(metadata).unwrap_or_default());
    }

    // Display first 200 chars of markdown
    let preview = if markdown.len() > 200 {
        &markdown[..200]
    } else {
        markdown
    };
    println!("\nMarkdown preview:\n{}\n...", preview);

    println!("\n✓ Docling pipeline validation complete!");
}

/// Test LLM API integration
///
/// Validates that the LLM API is working correctly with the configured model.
///
/// Prerequisites:
/// - LLM_API_URL, LLM_API_KEY, and LLM_MODEL set in tests/test.env
#[tokio::test]
async fn test_llm_api_integration() {
    // Load test config
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let config = rag_chat::domain::models::LLMConfig {
        provider: std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string()),
        model: std::env::var("LLM_MODEL").expect("LLM_MODEL not set in tests/test.env"),
        api_url: std::env::var("LLM_API_URL").expect("LLM_API_URL not set in tests/test.env"),
        api_key: std::env::var("LLM_API_KEY").expect("LLM_API_KEY not set in tests/test.env"),
    };

    println!("Testing LLM API with model: {}", config.model);

    // Test call_llm function
    let system_prompt = "You are a helpful assistant. Answer concisely.";
    let user_prompt = "What is 2+2?";

    match rag_chat::infra::llm::call_llm(&config, system_prompt, user_prompt).await {
        Ok(response) => {
            println!("✓ LLM API call succeeded");
            println!("  Response length: {} chars", response.len());
            println!("  Response: {}", &response[..std::cmp::min(200, response.len())]);
            assert!(!response.is_empty(), "LLM returned empty response");
        }
        Err(e) => {
            panic!("LLM API call failed: {}", e);
        }
    }
}

/// Test LLM streaming API integration
///
/// Validates that the streaming LLM API returns a proper stream of responses.
///
/// Prerequisites:
/// - LLM_API_URL, LLM_API_KEY, and LLM_MODEL set in tests/test.env
#[tokio::test]
async fn test_llm_streaming_api() {
    use futures::stream::StreamExt;

    // Load test config
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let config = rag_chat::domain::models::LLMConfig {
        provider: std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string()),
        model: std::env::var("LLM_MODEL").expect("LLM_MODEL not set in tests/test.env"),
        api_url: std::env::var("LLM_API_URL").expect("LLM_API_URL not set in tests/test.env"),
        api_key: std::env::var("LLM_API_KEY").expect("LLM_API_KEY not set in tests/test.env"),
    };

    println!("Testing LLM streaming API with model: {}", config.model);

    let system_prompt = "You are a helpful assistant. Answer concisely.";
    let user_prompt = "Write a short haiku about AI.";

    match rag_chat::infra::llm::stream_llm(&config, system_prompt, user_prompt).await {
        Ok(mut stream) => {
            println!("✓ LLM streaming API call initiated");

            let mut total_chunks = 0;
            let mut full_response = String::new();

            while let Some(result) = stream.next().await {
                match result {
                    Ok(chunk) => {
                        total_chunks += 1;
                        full_response.push_str(&chunk);
                        if total_chunks <= 3 {
                            println!("  Chunk {}: {} bytes", total_chunks, chunk.len());
                        }
                    }
                    Err(e) => {
                        println!("  Error in stream: {}", e);
                        break;
                    }
                }
            }

            println!("✓ LLM streaming completed");
            println!("  Total chunks: {}", total_chunks);
            println!("  Total response length: {} chars", full_response.len());
            println!("  Response: {}", &full_response[..std::cmp::min(200, full_response.len())]);

            assert!(total_chunks > 0, "Stream returned no chunks");
            assert!(!full_response.is_empty(), "Stream returned empty response");
        }
        Err(e) => {
            panic!("LLM streaming API call failed: {}", e);
        }
    }
}

/// Test chat handler with RAG context
///
/// Validates that the chat handler can generate responses with context from documents.
///
/// Prerequisites:
/// - PostgreSQL/ParadeDB running
/// - Documents indexed in database
/// - LLM API configured
#[tokio::test]
async fn test_chat_with_rag_context() {
    use axum::extract::State;
    use axum::Json;

    // Load test config
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set in tests/test.env");

    // Connect to database
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Check if we have any documents to work with
    let doc_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("Failed to query documents");

    if doc_count.0 == 0 {
        println!("⚠ Skipping chat test: no documents in database");
        println!("  Run test_index_local_pdf first to index documents");
        return;
    }

    println!("Found {} documents in database", doc_count.0);

    // Initialize embedder and state
    let embedder = rag_chat::infra::embedder::Embedder::new()
        .expect("Failed to create Embedder");

    let log_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let leptos_options = leptos::prelude::LeptosOptions::builder()
        .output_name("rag-chat")
        .site_root("target/site")
        .build();

    let state = rag_chat::api::state::AppState::new(
        pool.clone(),
        embedder,
        log_buffer,
        leptos_options,
    );

    // Create a chat request
    let req = rag_chat::domain::dtos::ChatRequest {
        message: "What is the main topic of the documents?".to_string(),
        context_chunks: 3,
        document_ids: None,
        conversation_id: None,
    };

    println!("Sending chat request: '{}'", req.message);

    match rag_chat::api::handlers::chat(State(state), Json(req)).await {
        Ok(response) => {
            println!("✓ Chat API call succeeded");
            println!("  Conversation ID: {}", response.0.conversation_id);
            println!("  Response length: {} chars", response.0.message.len());
            println!("  Sources: {}", response.0.sources.len());
            println!("  Response: {}", &response.0.message[..std::cmp::min(200, response.0.message.len())]);

            assert!(!response.0.message.is_empty(), "Chat returned empty response");
            assert!(!response.0.sources.is_empty(), "Chat returned no sources");
        }
        Err(e) => {
            panic!("Chat API call failed: {:?}", e);
        }
    }
}
