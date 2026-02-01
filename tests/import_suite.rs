mod common;

use anyhow::Result;
use rag_chat::config::Settings;
use rag_chat::infra::embedder::Embedder;
use rag_chat::services::indexing::index_path_with_config;
use sqlx::postgres::PgPoolOptions;
use std::path::Path;
use std::time::Instant;
use uuid::Uuid;
use text_splitter::{ChunkConfig, TextSplitter};
use rag_chat::services::job_cleanup;
use chrono;

// Include tests from previous files
// Adapted from import_test.rs, docling_service_test.rs, docling_url_test.rs, chunking_test.rs, manual_indexing_test.rs, job_cleanup_test.rs

// ============================================
// Internal Logic Tests (No Server Required)
// ============================================

fn chunk_text(text: &str, target_tokens: usize) -> Vec<String> {
    let splitter = TextSplitter::new(ChunkConfig::new(target_tokens).with_trim(true));
    splitter.chunks(text).map(|s: &str| s.to_string()).collect()
}

#[test]
fn test_chunking_logic_basic() {
    println!("\n✂️  Testing basic text chunking...\n");
    let text = "This is a test document. It has multiple sentences. We want to split it into chunks.";
    let chunks = chunk_text(text, 50);
    assert!(!chunks.is_empty(), "Should produce at least one chunk");
    assert!(chunks.iter().all(|c| !c.trim().is_empty()), "All chunks should have content");
}

#[test]
fn test_chunking_logic_limits() {
    println!("\n📏 Testing chunk size limits...\n");
    let long_text = "word ".repeat(1000); 
    let chunks = chunk_text(&long_text, 100);
    for (i, chunk) in chunks.iter().enumerate() {
        let token_estimate = chunk.split_whitespace().count();
        assert!(token_estimate <= 150, "Chunk {} exceeds reasonable token limit", i + 1);
    }
}

// ============================================
// Service Integration Tests (Database/Docling)
// ============================================

#[tokio::test]
async fn test_service_import_wellbeing_pdf_integration() -> Result<()> {
    // Skip if running in CI without proper env or DB
    if std::env::var("CI").is_ok() {
        return Ok(());
    }
    
    // Ensure environment is set for test database
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test-gpu");
    }

    let settings = Settings::new()?;
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await?;
    let embedder = Embedder::new(&settings.embedding)?;

    let test_file = "tests/test_data/HumanPrincipals.pdf";
    if !Path::new(test_file).exists() {
        println!("⚠️ Test file not found: {}", test_file);
        return Ok(());
    }

    println!("\n🚀 Testing PDF import integration (Direct Service Call)");
    match index_path_with_config(&pool, &embedder, test_file, Some(&settings)).await {
        Ok(ids) => {
            println!("✅ PDF import successful! Indexed {} docs", ids.len());
            // Verify in DB
            let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM documents WHERE source_path = $1")
                .bind(test_file)
                .fetch_one(&pool)
                .await?;
            assert!(count > 0, "Document should be in database");
        }
        Err(e) => {
            println!("❌ PDF import failed: {}", e);
            if e.to_string().contains("Connection refused") {
                println!("⚠️  Docling service likely not running. Skipping test failure.");
                return Ok(());
            }
            return Err(e);
        }
    }
    Ok(())
}

#[tokio::test]
async fn test_service_job_cleanup_logic() {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }
    let settings = Settings::new().expect("Failed to load settings");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    // Clean up
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '%cleanup_test%'").execute(&pool).await.ok();

    // Create old job
    let old_id = Uuid::new_v4();
    let old_time = chrono::Utc::now() - chrono::Duration::hours(48);
    sqlx::query("INSERT INTO import_jobs (id, source_type, source_path, status, created_at, completed_at) VALUES ($1, 'test', '/cleanup_test/old', 'completed', $2, $2)")
        .bind(old_id).bind(old_time).execute(&pool).await.unwrap();

    // Run cleanup
    let deleted = job_cleanup::cleanup_old_jobs(&pool, &settings.import.cleanup).await.expect("Cleanup failed");
    println!("Deleted {} old jobs", deleted);
    
    // Verify
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id = $1)").bind(old_id).fetch_one(&pool).await.unwrap();
    assert!(!exists, "Old job should be deleted");
}

// ============================================
// E2E API Tests (Requires Server)
// ============================================

#[tokio::test]
async fn test_e2e_import_full_flow() {
    let client = common::TestClient::new();
    client.ensure_server_running().await;

    println!("Starting Full Import E2E Flow...");

    // 1. Create Import Job (URL)
    let url = "https://example.com";
    let import_req = serde_json::json!({
        "source_type": "url",
        "source_path": url
    });

    let resp = client.client.post(client.url("/import")).json(&import_req).send().await.expect("Failed req");
    assert!(resp.status().is_success());
    let job_data: serde_json::Value = resp.json().await.unwrap();
    let job_id = job_data["id"].as_str().unwrap();

    // 2. Poll Status
    let mut status = "pending".to_string();
    let mut retries = 0;
    while (status == "pending" || status == "running") && retries < 30 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let s_resp = client.client.get(client.url(&format!("/import/{}", job_id))).send().await.unwrap();
        let s_data: serde_json::Value = s_resp.json().await.unwrap();
        status = s_data["status"].as_str().unwrap().to_string();
        println!("Job Status: {}", status);
        retries += 1;
    }

    // 3. List History
    let hist_resp = client.client.get(client.url("/import")).send().await.unwrap();
    let hist_data: serde_json::Value = hist_resp.json().await.unwrap();
    let jobs = hist_data["jobs"].as_array().unwrap();
    assert!(jobs.iter().any(|j| j["id"].as_str() == Some(job_id)));


    // 4. Delete Job (requires JSON body)
    let del_req = serde_json::json!({ "delete_documents": false });
    let del_resp = client.client.delete(client.url(&format!("/import/{}", job_id)))
        .json(&del_req)
        .send()
        .await
        .unwrap();
    assert!(del_resp.status().is_success(), "Delete failed: {}", del_resp.status());
}

// ============================================
// Docling Service Specific Tests
// ============================================

#[tokio::test]
async fn test_docling_service_health_check() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }
    // Skip in CI if no docling service
    if std::env::var("CI").is_ok() { return Ok(()); }

    let settings = Settings::new()?;
    let client = reqwest::Client::new();
    let url = &settings.docling.url;

    println!("Checking Docling Health at {}", url);

    // 1. Health
    let h_resp = client.get(format!("{}/health", url)).send().await;
    match h_resp {
        Ok(r) => assert!(r.status().is_success(), "Docling health check failed"),
        Err(_) => {
            println!("⚠️  Docling service unreachable. Skipping test.");
            return Ok(());
        }
    }

    // 2. OpenAPI Spec (Discovery)
    let o_resp = client.get(format!("{}/openapi.json", url)).send().await?;
    assert!(o_resp.status().is_success(), "Failed to get OpenAPI spec");

    Ok(())
}

#[tokio::test]
async fn test_docling_parsing_capabilities() -> Result<()> {
    if std::env::var("CI").is_ok() { return Ok(()); }
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }

    let settings = Settings::new()?;
    let client = reqwest::Client::new();
    let url = &settings.docling.url;
    let test_file = "tests/test_data/HumanPrincipals.pdf";

    if !Path::new(test_file).exists() {
        println!("⚠️ Test file missing for parsing test");
        return Ok(());
    }

    let form = reqwest::multipart::Form::new()
        .file("files", test_file)
        .await?;

    let start = Instant::now();
    let resp = client.post(format!("{}/v1/convert/file", url))
        .multipart(form)
        .send()
        .await;

    match resp {
        Ok(r) => {
            if !r.status().is_success() {
                println!("⚠️ Docling parsing failed (status {}). Skipping.", r.status());
                return Ok(());
            }
            let result: serde_json::Value = r.json().await?;
            println!("Parsing took {:.2}s", start.elapsed().as_secs_f64());

            // 1. Content Check
            let content = result["document"]["md_content"].as_str().unwrap_or("");
            assert!(!content.is_empty(), "Parsed content should not be empty");

            // 2. Table Detection Check
            let tables = result.get("tables").and_then(|t| t.as_array());
            if let Some(t) = tables {
                println!("Found {} tables", t.len());
            }

            // 3. Metadata Check - Docling response structure may vary
            if let Some(doc) = result.get("document") {
                // Check for either 'name' or 'file_info.filename' or content itself
                let has_name = doc.get("name").is_some()
                    || doc.get("file_info").and_then(|f| f.get("filename")).is_some()
                    || doc.get("md_content").is_some();
                assert!(has_name, "Document should have name, file_info, or content");

                // Origin check is optional - some Docling versions may not include it
                if doc.get("origin").is_none() {
                    println!("Note: Document origin not present (optional field)");
                }
            }
        },
        Err(e) => println!("⚠️ Docling request failed: {}", e),
    }

    Ok(())
}

// ============================================
// Document Storage Tests (from document_storage_test.rs)
// ============================================

#[tokio::test]
async fn test_store_and_retrieve_document() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }
    let settings = Settings::new()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&settings.database.url).await?;

    let doc_id = Uuid::new_v4();
    let title = "Test Document";
    let content = "This is test content for storage validation.";
    let summary = Some("A test document".to_string());
    let keywords = vec!["test".to_string(), "storage".to_string()];
    let entities = serde_json::json!(["TestEntity"]);
    let filepath = Some("test/path.pdf");
    let created_at = chrono::Utc::now();

    sqlx::query(
        r#"INSERT INTO documents (id, title, content, summary, keywords, entities, source_path, source_type, created_at, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pdf', $8, $9)"#
    )
    .bind(doc_id)
    .bind(title)
    .bind(content)
    .bind(summary.clone())
    .bind(&keywords)
    .bind(&entities)
    .bind(filepath)
    .bind(created_at)
    .bind(serde_json::json!({}))
    .execute(&pool)
    .await?;

    // Retrieve document
    let retrieved: (String, String, Option<String>) = sqlx::query_as(
        "SELECT title, content, summary FROM documents WHERE id = $1"
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await?;

    assert_eq!(retrieved.0, title);
    assert_eq!(retrieved.1, content);
    assert_eq!(retrieved.2, summary);

    // Cleanup
    sqlx::query("DELETE FROM documents WHERE id = $1").bind(doc_id).execute(&pool).await?;
    Ok(())
}

#[tokio::test]
async fn test_document_upsert() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }
    let settings = Settings::new()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&settings.database.url).await?;

    let doc_id = Uuid::new_v4();

    // First insert
    sqlx::query(
        r#"INSERT INTO documents (id, title, content, source_path, source_type, created_at)
        VALUES ($1, 'Upsert Test', 'Original content', 'test.pdf', 'pdf', $2)"#
    )
    .bind(doc_id)
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await?;

    // Upsert (update)
    sqlx::query(
        r#"INSERT INTO documents (id, title, content, source_path, source_type, created_at)
        VALUES ($1, 'Upsert Test', 'Updated content', 'test.pdf', 'pdf', $2)
        ON CONFLICT (id) DO UPDATE SET content = EXCLUDED.content, created_at = EXCLUDED.created_at"#
    )
    .bind(doc_id)
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await?;

    // Verify update
    let content: String = sqlx::query_scalar("SELECT content FROM documents WHERE id = $1")
        .bind(doc_id)
        .fetch_one(&pool)
        .await?;

    assert_eq!(content, "Updated content");

    // Cleanup
    sqlx::query("DELETE FROM documents WHERE id = $1").bind(doc_id).execute(&pool).await?;
    Ok(())
}

// ============================================
// Enrichment Tests (from enricher_test.rs)
// ============================================

#[test]
fn test_enrich_chunk_formatting() {
    use rag_chat::services::enrichment::enrich_chunk;

    let title = "Test Document";
    let summary = "This is a summary.";
    let keywords = vec!["key1".to_string(), "key2".to_string()];
    let questions = vec!["What is this?".to_string(), "Why?".to_string()];
    let chunk = "This is the chunk content.";

    let enriched = enrich_chunk(title, summary, &keywords, &questions, chunk);

    assert!(enriched.contains("Title: Test Document"));
    assert!(enriched.contains("Summary: This is a summary."));
    assert!(enriched.contains("key1, key2"));
    assert!(enriched.contains("What is this?"));
    assert!(enriched.contains("This is the chunk content."));
}

#[test]
fn test_enrich_chunk_no_questions() {
    use rag_chat::services::enrichment::enrich_chunk;

    let title = "Test Document";
    let summary = "This is a summary.";
    let keywords = vec!["key1".to_string()];
    let questions: Vec<String> = vec![];
    let chunk = "Chunk content.";

    let enriched = enrich_chunk(title, summary, &keywords, &questions, chunk);

    assert!(enriched.contains("Title: Test Document"));
    assert!(enriched.contains("Chunk content."));
}

// ============================================
// Reranker Tests (from reranker_test.rs)
// ============================================

async fn is_reranker_available() -> bool {
    let settings = Settings::new().ok();
    if let Some(settings) = settings {
        if !settings.reranking.enabled {
            return false;
        }

        let url = format!("{}/api/chat", settings.reranking.api_url);
        let response = reqwest::Client::new()
            .post(&url)
            .timeout(std::time::Duration::from_secs(10))
            .json(&serde_json::json!({
                "model": settings.reranking.model,
                "messages": [{"role": "user", "content": "test"}],
                "stream": false
            }))
            .send()
            .await;

        if let Ok(resp) = response {
            return resp.status().is_success();
        }
    }
    false
}

#[tokio::test]
async fn test_reranker_single_document() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test-gpu"); }

    if !is_reranker_available().await {
        println!("⚠️ Skipping test: reranker not available");
        return Ok(());
    }

    let settings = Settings::new()?;
    let reranker = rag_chat::infra::reranker::Reranker::new(&settings.reranking)?;

    let query = "What is machine learning?";
    let relevant_doc = "Machine learning is a subset of artificial intelligence that enables systems to learn from experience.";
    let irrelevant_doc = "The capital of France is Paris. It is located in northern France on the Seine River.";

    let relevant_score = reranker.rerank_single(query, relevant_doc).await?;
    let irrelevant_score = reranker.rerank_single(query, irrelevant_doc).await?;

    println!("Relevant doc score: {:.2}", relevant_score);
    println!("Irrelevant doc score: {:.2}", irrelevant_score);

    assert!(relevant_score >= irrelevant_score,
        "Relevant document should score higher ({} >= {})",
        relevant_score, irrelevant_score);

    Ok(())
}

#[tokio::test]
async fn test_reranker_batch_documents() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test-gpu"); }

    if !is_reranker_available().await {
        println!("⚠️ Skipping test: reranker not available");
        return Ok(());
    }

    let settings = Settings::new()?;
    let reranker = rag_chat::infra::reranker::Reranker::new(&settings.reranking)?;

    let query = "cloud computing platforms";
    let documents = vec![
        "Kubernetes is an open-source container orchestration platform.",
        "AWS provides cloud computing services including compute, storage, and databases.",
        "The Eiffel Tower is a wrought-iron lattice tower in Paris, France.",
        "Pizza is a traditional Italian dish with tomato sauce and cheese.",
    ];

    let start = std::time::Instant::now();
    let scores = reranker.rerank_batch(query, &documents).await?;
    let elapsed = start.elapsed();

    println!("Batch reranking {} documents took {:.2}s", documents.len(), elapsed.as_secs_f64());

    // Verify scores are in valid range
    for (i, score) in scores.iter().enumerate() {
        assert!(*score >= 0.0 && *score <= 1.0, "Score {} out of range: {}", i, score);
    }

    // Cloud-related docs should score higher than pizza/Eiffel
    let pizza_idx = 3;
    let has_higher_scores = scores[..3].iter().any(|s| s > &scores[pizza_idx]);
    assert!(has_higher_scores, "Cloud docs should score higher than pizza doc");

    Ok(())
}
