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
