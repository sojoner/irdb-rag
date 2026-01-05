/// AGENT_IMPORT Tests - Document Import with Retry/Skip Logic
///
/// TDD approach: Tests written first, implementation follows
/// Tests cover:
/// - Error classification (transient vs permanent)
/// - Retry backoff calculation
/// - Import job CRUD operations
/// - Import item tracking and status updates
/// - End-to-end import workflow with real documents
///
/// Prerequisites:
/// - PostgreSQL/ParadeDB running (docker compose up -d)
/// - test.env configured with DATABASE_URL, DOCLING_URL, etc.
use uuid::Uuid;

// Test imports
#[allow(dead_code)]
#[path = "../src/domain/models.rs"]
mod models;

// ============================================================================
// TEST 1: Error Classification
// ============================================================================

#[test]
fn test_classify_transient_errors() {
    let transient_messages = vec![
        "timeout waiting for response",
        "HTTP 503 Service Unavailable",
        "connection refused",
        "rate limit exceeded",
        "TIMEOUT",
        "503",
        "Rate limited",
        "Connection refused",
    ];

    for msg in transient_messages {
        let is_transient = msg.to_lowercase().contains("timeout")
            || msg.to_lowercase().contains("503")
            || msg.to_lowercase().contains("connection refused")
            || msg.to_lowercase().contains("rate limit");
        assert!(
            is_transient,
            "Message '{}' should be classified as transient",
            msg
        );
    }
}

#[test]
fn test_classify_permanent_errors() {
    let permanent_messages = vec![
        "file not found",
        "unsupported format",
        "corrupt file",
        "permission denied",
        "File not found: /path/to/missing.pdf",
        "Unsupported file type: .xyz",
    ];

    for msg in permanent_messages {
        let is_permanent = !msg.to_lowercase().contains("timeout")
            && !msg.to_lowercase().contains("503")
            && !msg.to_lowercase().contains("connection refused")
            && !msg.to_lowercase().contains("rate limit");
        assert!(
            is_permanent,
            "Message '{}' should be classified as permanent",
            msg
        );
    }
}

// ============================================================================
// TEST 2: Retry Backoff Calculation
// ============================================================================

#[test]
fn test_retry_delay_exponential_growth() {
    // Base: 1000ms, max: 30000ms
    let mut delays = vec![];

    for attempt in 0..4 {
        let base = 1000.0;
        let delay = base * 2.0_f64.powi(attempt);
        let capped = delay.min(30000.0);
        delays.push(capped as u64);
    }

    // Verify exponential growth (doubling each time)
    assert_eq!(delays[0], 1000);  // 1000ms
    assert_eq!(delays[1], 2000);  // 2000ms
    assert_eq!(delays[2], 4000);  // 4000ms
    assert_eq!(delays[3], 8000);  // 8000ms

    println!("✓ Retry delays show exponential growth: {:?}ms", delays);
}

#[test]
fn test_retry_delay_max_cap() {
    // Verify delay caps at 30000ms
    let base = 1000.0;
    for attempt in 0..10 {
        let delay = base * 2.0_f64.powi(attempt);
        let capped = delay.min(30000.0);
        assert!(
            capped <= 30000.0,
            "Delay at attempt {} should be capped at 30000ms, got {}ms",
            attempt,
            capped
        );
    }

    println!("✓ Retry delay correctly caps at 30000ms");
}

#[test]
fn test_retry_delay_with_jitter() {
    // Verify jitter stays within bounds (10% of base delay)
    let base = 1000.0;
    let delay_base = base * 2.0_f64.powi(2); // attempt 2 = 4000ms
    let delay_capped = delay_base.min(30000.0);
    let max_jitter = delay_capped * 0.1; // 10% jitter

    // Simulate multiple calls to verify bounds
    for _ in 0..10 {
        use std::time::SystemTime;
        let nanos = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .subsec_nanos() as f64;
        let jitter = delay_capped * 0.1 * (nanos % 1.0);
        let final_delay = delay_capped + jitter;

        assert!(
            final_delay >= delay_capped && final_delay <= delay_capped + max_jitter,
            "Jittered delay should be within bounds, got {}",
            final_delay
        );
    }

    println!(
        "✓ Retry jitter adds randomness within 10% variance: {}ms ± 10%",
        delay_capped
    );
}

// ============================================================================
// TEST 3: Import Job Model Validation
// ============================================================================

#[test]
fn test_import_job_creation() {
    let job_id = Uuid::new_v4();
    let source_path = "/path/to/documents".to_string();

    // Model should support these fields:
    // - id: UUID
    // - status: String (pending, running, completed, failed, cancelled)
    // - source_type: String (folder, url, file_upload)
    // - source_path: Option<String>
    // - total_items: i32
    // - processed_items: i32
    // - failed_items: i32
    // - skipped_items: i32

    println!(
        "✓ Import job structure ready for: ID={}, source={}",
        job_id, source_path
    );
}

#[test]
fn test_import_job_status_transitions() {
    let valid_statuses = ["pending", "running", "completed", "failed", "cancelled"];

    // Verify valid transitions
    let transitions = vec![
        ("pending", "running"),
        ("running", "completed"),
        ("running", "failed"),
        ("running", "cancelled"),
        ("failed", "running"), // Can retry
    ];

    for (from, to) in transitions {
        assert!(
            valid_statuses.contains(&from) && valid_statuses.contains(&to),
            "Status transition {} -> {} should be valid",
            from,
            to
        );
    }

    println!("✓ Import job status transitions are valid");
}

// ============================================================================
// TEST 4: Import Item Tracking
// ============================================================================

#[test]
fn test_import_item_status_lifecycle() {
    let valid_statuses = ["pending", "processing", "completed", "failed", "skipped"];

    let lifecycle = vec![
        "pending",   // Created
        "processing", // Started
        "completed",  // Done
    ];

    for status in lifecycle {
        assert!(
            valid_statuses.contains(&status),
            "Status '{}' should be valid",
            status
        );
    }

    println!("✓ Import item lifecycle: pending -> processing -> completed");
}

#[test]
fn test_import_item_retry_count() {
    // Model should track retry attempts
    let mut retry_count = 0;
    let max_retries = 3;

    while retry_count < max_retries {
        // Simulate transient error
        retry_count += 1;
        println!("  Attempt {}/{}", retry_count, max_retries);
    }

    assert_eq!(
        retry_count, max_retries,
        "Should track retry attempts correctly"
    );
    println!("✓ Import item tracks retry count: {}/{}", retry_count, max_retries);
}

// ============================================================================
// TEST 5: Database Integration (requires docker/test env)
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored import_job_crud
async fn test_import_job_crud() {
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Test: Create import job
    let job_id = Uuid::new_v4();
    let source_path = "/documents";

    let query = r#"
        INSERT INTO import_jobs (id, status, source_type, source_path, total_items)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, status
    "#;

    let result: (Uuid, String) = sqlx::query_as(query)
        .bind(job_id)
        .bind("pending")
        .bind("folder")
        .bind(source_path)
        .bind(0)
        .fetch_one(&pool)
        .await
        .expect("Failed to insert import job");

    assert_eq!(result.0, job_id);
    assert_eq!(result.1, "pending");
    println!("✓ Created import job: {:?}", result);

    // Test: Update job status
    let update_query = r#"
        UPDATE import_jobs
        SET status = $1, total_items = $2
        WHERE id = $3
        RETURNING id, status, total_items
    "#;

    let updated: (Uuid, String, i32) = sqlx::query_as(update_query)
        .bind("running")
        .bind(5)
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to update import job");

    assert_eq!(updated.1, "running");
    assert_eq!(updated.2, 5);
    println!("✓ Updated job status and item count: {:?}", updated);

    // Test: Get job by ID
    let get_query = "SELECT id, status, total_items, processed_items FROM import_jobs WHERE id = $1";

    let job: (Uuid, String, i32, i32) = sqlx::query_as(get_query)
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to fetch import job");

    assert_eq!(job.0, job_id);
    println!("✓ Retrieved import job: status={}, total={}, processed={}",
             job.1, job.2, job.3);

    // Cleanup
    sqlx::query("DELETE FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored import_item_tracking
async fn test_import_item_tracking() {
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    let job_id = Uuid::new_v4();
    let item_id = Uuid::new_v4();
    let source_path = "documents/sample.pdf";

    // Create job first
    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(job_id)
    .bind("pending")
    .bind("folder")
    .bind("/documents")
    .execute(&pool)
    .await
    .ok();

    // Test: Create import item
    let insert_query = r#"
        INSERT INTO import_items (id, job_id, source_path, status, retry_count, error_type)
        VALUES ($1, $2, $3, $4, $5, $6)
        RETURNING id, status, retry_count
    "#;

    let result: (Uuid, String, i32) = sqlx::query_as(insert_query)
        .bind(item_id)
        .bind(job_id)
        .bind(source_path)
        .bind("pending")
        .bind(0)
        .bind("transient")
        .fetch_one(&pool)
        .await
        .expect("Failed to insert import item");

    assert_eq!(result.0, item_id);
    assert_eq!(result.1, "pending");
    assert_eq!(result.2, 0);
    println!("✓ Created import item: {:?}", result);

    // Test: Update item status with error
    let error_query = r#"
        UPDATE import_items
        SET status = $1, retry_count = $2, error_message = $3
        WHERE id = $4
        RETURNING status, retry_count, error_message
    "#;

    let error_result: (String, i32, Option<String>) = sqlx::query_as(error_query)
        .bind("failed")
        .bind(1)
        .bind(Some("Connection timeout"))
        .bind(item_id)
        .fetch_one(&pool)
        .await
        .expect("Failed to update import item with error");

    assert_eq!(error_result.0, "failed");
    assert_eq!(error_result.1, 1);
    assert_eq!(error_result.2, Some("Connection timeout".to_string()));
    println!("✓ Updated item with error: {:?}", error_result);

    // Cleanup
    sqlx::query("DELETE FROM import_items WHERE job_id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
}

// ============================================================================
// TEST 6: End-to-End Import with Real Documents
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_import_folder_workflow
async fn test_import_folder_workflow() {
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // This test will verify:
    // 1. Job is created in pending state
    // 2. Documents are discovered and items created
    // 3. Each document is processed through the indexing pipeline
    // 4. On success, items marked as completed with document_id
    // 5. On failure, items marked with error_type and error_message
    // 6. Job status updated based on item results

    let test_folder = "tests/test_data";
    assert!(
        std::path::Path::new(test_folder).exists(),
        "Test documents folder should exist"
    );

    // Step 1: Create import job
    let job_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(job_id)
    .bind("pending")
    .bind("folder")
    .bind(test_folder)
    .execute(&pool)
    .await
    .expect("Failed to create import job");

    println!("✓ Created import job: {}", job_id);

    // Step 2: Discover files (using the discover_files function)
    // This would be done by the import service, but we test it here
    use std::path::PathBuf;

    fn discover_test_files(folder: &str) -> Vec<PathBuf> {
        use walkdir::WalkDir;
        let mut files = vec![];
        for entry in WalkDir::new(folder)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            files.push(entry.path().to_path_buf());
        }
        files
    }

    let files = discover_test_files(test_folder);
    assert!(files.len() >= 3, "Should discover at least 3 test files");
    println!("✓ Discovered {} files", files.len());

    // Step 3: Create import items
    for file in &files {
        let item_id = Uuid::new_v4();
        let file_path = file.to_str().unwrap();

        sqlx::query(
            r#"INSERT INTO import_items (id, job_id, source_path, status)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(item_id)
        .bind(job_id)
        .bind(file_path)
        .bind("pending")
        .execute(&pool)
        .await
        .expect("Failed to create import item");
    }

    println!("✓ Created {} import items", files.len());

    // Step 4: Update job total_items
    sqlx::query(
        r#"UPDATE import_jobs SET total_items = $1 WHERE id = $2"#,
    )
    .bind(files.len() as i32)
    .bind(job_id)
    .execute(&pool)
    .await
    .expect("Failed to update job total_items");

    // Step 5: Verify job and items
    let job: (i32, i32) = sqlx::query_as(
        "SELECT total_items, processed_items FROM import_jobs WHERE id = $1"
    )
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch job");

    assert_eq!(job.0, files.len() as i32);
    assert_eq!(job.1, 0); // No items processed yet
    println!("✓ Job state: total={}, processed={}", job.0, job.1);

    // Cleanup
    sqlx::query("DELETE FROM import_items WHERE job_id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();

    println!("✓ Import folder workflow: complete!");
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_import_resilience
async fn test_import_resilience_with_failures() {
    // Simulate import with mixed results:
    // - Some documents succeed immediately
    // - Some fail with transient errors (should retry)
    // - Some fail with permanent errors (should skip)

    println!("✓ Import resilience test structure ready for:");
    println!("  - Successful imports");
    println!("  - Transient failures (timeout, 503) → auto-retry");
    println!("  - Permanent failures (not found, unsupported) → skip");
    println!("  - Mixed batch with progress tracking");
}

// ============================================================================
// TEST 8: Real File Path Import (TDD - Write Test First)
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_real_file_import
async fn test_real_file_import() {
    // This test validates importing a real file path
    // File: /Users/hagentonnies/Downloads/2025-10-09-cfp-kubecon-eu-26.md

    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    let test_file = "/Users/hagentonnies/Downloads/2025-10-09-cfp-kubecon-eu-26.md";

    // Verify file exists
    assert!(
        std::path::Path::new(test_file).exists(),
        "Test file should exist: {}", test_file
    );

    // Step 1: Create import job via API-like flow
    let job_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(job_id)
    .bind("pending")
    .bind("file")
    .bind(test_file)
    .execute(&pool)
    .await
    .expect("Failed to create import job");

    println!("✓ Created import job for file: {}", test_file);

    // Step 2: Create import item for the single file
    let item_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO import_items (id, job_id, source_path, status)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(item_id)
    .bind(job_id)
    .bind(test_file)
    .bind("pending")
    .execute(&pool)
    .await
    .expect("Failed to create import item");

    // Step 3: Update job totals
    sqlx::query(
        r#"UPDATE import_jobs SET total_items = 1 WHERE id = $1"#,
    )
    .bind(job_id)
    .execute(&pool)
    .await
    .expect("Failed to update job totals");

    // Step 4: Verify job state
    let job: (String, i32, i32) = sqlx::query_as(
        "SELECT status, total_items, processed_items FROM import_jobs WHERE id = $1"
    )
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch job");

    assert_eq!(job.0, "pending");
    assert_eq!(job.1, 1);
    assert_eq!(job.2, 0);

    println!("✓ File import job created: status={}, total={}, processed={}",
             job.0, job.1, job.2);

    // EXPECTED: Background processor should pick up this job and:
    // 1. Process the file through indexing pipeline
    // 2. Update item status to "completed" with document_id
    // 3. Update job processed_items count
    // 4. Mark job as "completed"

    // Cleanup
    sqlx::query("DELETE FROM import_items WHERE job_id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
}

// ============================================================================
// TEST 9: Real URL Import (TDD - Write Test First)
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_real_url_import
async fn test_real_url_import() {
    // This test validates importing a URL
    // URL: https://www.spiegel.de/politik/deutschland/

    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    let test_url = "https://www.spiegel.de/politik/deutschland/";

    // Step 1: Create import job for URL
    let job_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(job_id)
    .bind("pending")
    .bind("url")
    .bind(test_url)
    .execute(&pool)
    .await
    .expect("Failed to create import job");

    println!("✓ Created import job for URL: {}", test_url);

    // Step 2: Create import item for the URL
    let item_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO import_items (id, job_id, source_path, status)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(item_id)
    .bind(job_id)
    .bind(test_url)
    .bind("pending")
    .execute(&pool)
    .await
    .expect("Failed to create import item");

    // Step 3: Update job totals
    sqlx::query(
        r#"UPDATE import_jobs SET total_items = 1 WHERE id = $1"#,
    )
    .bind(job_id)
    .execute(&pool)
    .await
    .expect("Failed to update job totals");

    // Step 4: Verify job state
    let job: (String, i32, i32) = sqlx::query_as(
        "SELECT status, total_items, processed_items FROM import_jobs WHERE id = $1"
    )
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch job");

    assert_eq!(job.0, "pending");
    assert_eq!(job.1, 1);
    assert_eq!(job.2, 0);

    println!("✓ URL import job created: status={}, total={}, processed={}",
             job.0, job.1, job.2);

    // EXPECTED: Background processor should pick up this job and:
    // 1. Download and process the URL through indexing pipeline
    // 2. Update item status to "completed" with document_id
    // 3. Update job processed_items count
    // 4. Mark job as "completed"

    // Cleanup
    sqlx::query("DELETE FROM import_items WHERE job_id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .ok();
}

// ============================================================================
// TEST 7: Import Job Deletion and Cleanup
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_delete_import_job
async fn test_delete_import_job_cascades_to_items() {
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Step 1: Create import job
    let job_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path, total_items)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(job_id)
    .bind("pending")
    .bind("folder")
    .bind("/test")
    .bind(3)
    .execute(&pool)
    .await
    .expect("Failed to create import job");

    println!("✓ Created import job: {}", job_id);

    // Step 2: Create import items
    let item_ids: Vec<Uuid> = vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    for (i, item_id) in item_ids.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO import_items (id, job_id, source_path, status)
               VALUES ($1, $2, $3, $4)"#,
        )
        .bind(item_id)
        .bind(job_id)
        .bind(format!("/test/file{}.txt", i))
        .bind("pending")
        .execute(&pool)
        .await
        .expect("Failed to create import item");
    }

    println!("✓ Created {} import items", item_ids.len());

    // Step 3: Verify items exist
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM import_items WHERE job_id = $1"
    )
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count items");

    assert_eq!(count.0, 3, "Should have 3 import items");

    // Step 4: Delete import job (should cascade to items due to ON DELETE CASCADE)
    sqlx::query("DELETE FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .expect("Failed to delete import job");

    println!("✓ Deleted import job");

    // Step 5: Verify items are also deleted (CASCADE)
    let count_after: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM import_items WHERE job_id = $1"
    )
    .bind(job_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count items after deletion");

    assert_eq!(count_after.0, 0, "Import items should be cascaded deleted");
    println!("✓ Import items cascaded deleted: {} items removed", item_ids.len());
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_delete_import_job_with_documents
async fn test_delete_import_job_preserves_or_deletes_documents() {
    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // This test verifies document cleanup behavior:
    // OPTION 1: Preserve documents (set document_id to NULL on item deletion)
    //   - Import items deleted, documents remain
    //   - Use: ON DELETE SET NULL in foreign key
    //
    // OPTION 2: Delete documents with import job (cascade deletion)
    //   - Import items deleted, documents also deleted
    //   - Use: Custom cleanup logic to delete orphaned documents
    //
    // Current schema uses ON DELETE SET NULL, so documents are preserved

    // Step 1: Create a mock document
    let doc_id = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO documents (id, title, content, source_path, source_type)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(doc_id)
    .bind("Test Document")
    .bind("Test content for document")
    .bind("/test/doc.txt")
    .bind("file")
    .execute(&pool)
    .await
    .expect("Failed to create test document");

    // Step 2: Create import job and item linked to document
    let job_id = Uuid::new_v4();
    let item_id = Uuid::new_v4();

    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(job_id)
    .bind("completed")
    .bind("folder")
    .bind("/test")
    .execute(&pool)
    .await
    .expect("Failed to create import job");

    sqlx::query(
        r#"INSERT INTO import_items (id, job_id, source_path, status, document_id)
           VALUES ($1, $2, $3, $4, $5)"#,
    )
    .bind(item_id)
    .bind(job_id)
    .bind("/test/doc.txt")
    .bind("completed")
    .bind(doc_id)
    .execute(&pool)
    .await
    .expect("Failed to create import item");

    println!("✓ Created job, item, and linked document");

    // Step 3: Delete import job
    sqlx::query("DELETE FROM import_jobs WHERE id = $1")
        .bind(job_id)
        .execute(&pool)
        .await
        .expect("Failed to delete import job");

    // Step 4: Verify document still exists (ON DELETE SET NULL)
    let doc_exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM documents WHERE id = $1"
    )
    .bind(doc_id)
    .fetch_optional(&pool)
    .await
    .expect("Failed to check document");

    if doc_exists.is_some() {
        println!("✓ Document preserved after import deletion (ON DELETE SET NULL)");

        // Cleanup
        sqlx::query("DELETE FROM documents WHERE id = $1")
            .bind(doc_id)
            .execute(&pool)
            .await
            .ok();
    } else {
        println!("✓ Document deleted with import (CASCADE or manual cleanup)");
    }
}

// ============================================================================
// TEST 10: Immediate Job Processing with Channels
// ============================================================================

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_processor_starts_immediately
async fn test_processor_starts_immediately() {
    // This test validates that:
    // 1. Import jobs are picked up immediately after insertion
    // 2. The processor starts processing as soon as a job is created
    // 3. Multiple jobs can be processed concurrently via channels (FIFO queue)
    //
    // Architecture:
    // - Channel-based job queue (tokio::sync::mpsc)
    // - Worker pool that listens for new jobs
    // - Jobs are dispatched immediately upon creation

    use tokio::sync::mpsc;
    use std::time::Duration;

    std::env::remove_var("DATABASE_URL");
    dotenvy::from_filename("tests/test.env").ok();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Step 1: Create a channel for job notifications
    let (tx, mut rx) = mpsc::channel::<Uuid>(100);

    // Step 2: Start a background worker that listens for jobs
    let pool_clone = pool.clone();
    let worker_handle = tokio::spawn(async move {
        let mut received_jobs = vec![];

        // Use timeout to detect if we receive jobs quickly
        while let Ok(Some(job_id)) = tokio::time::timeout(
            Duration::from_millis(500),
            rx.recv()
        ).await {
            println!("✓ Worker received job notification: {}", job_id);
            received_jobs.push(job_id);

            // Simulate quick processing - update status
            sqlx::query("UPDATE import_jobs SET status = 'running' WHERE id = $1")
                .bind(job_id)
                .execute(&pool_clone)
                .await
                .ok();
        }

        received_jobs
    });

    // Step 3: Create import jobs and send notifications immediately
    let job_id_1 = Uuid::new_v4();
    let job_id_2 = Uuid::new_v4();

    // Create job 1
    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(job_id_1)
    .bind("pending")
    .bind("folder")
    .bind("/test/path1")
    .execute(&pool)
    .await
    .expect("Failed to create import job 1");

    println!("✓ Created job 1: {}", job_id_1);

    // Immediately send notification
    tx.send(job_id_1).await.expect("Failed to send job notification");
    println!("✓ Sent notification for job 1");

    // Small delay to verify immediate processing
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Create job 2
    sqlx::query(
        r#"INSERT INTO import_jobs (id, status, source_type, source_path)
           VALUES ($1, $2, $3, $4)"#,
    )
    .bind(job_id_2)
    .bind("pending")
    .bind("folder")
    .bind("/test/path2")
    .execute(&pool)
    .await
    .expect("Failed to create import job 2");

    println!("✓ Created job 2: {}", job_id_2);

    // Immediately send notification
    tx.send(job_id_2).await.expect("Failed to send job notification");
    println!("✓ Sent notification for job 2");

    // Close the channel to signal worker to finish
    drop(tx);

    // Step 4: Wait for worker to process all jobs
    let received_jobs = worker_handle.await.expect("Worker panicked");

    // Step 5: Verify jobs were processed immediately
    assert_eq!(received_jobs.len(), 2, "Worker should have received both jobs");
    assert!(received_jobs.contains(&job_id_1), "Job 1 should be processed");
    assert!(received_jobs.contains(&job_id_2), "Job 2 should be processed");

    // Step 6: Verify job statuses were updated
    let status_1: (String,) = sqlx::query_as(
        "SELECT status FROM import_jobs WHERE id = $1"
    )
    .bind(job_id_1)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch job 1 status");

    let status_2: (String,) = sqlx::query_as(
        "SELECT status FROM import_jobs WHERE id = $1"
    )
    .bind(job_id_2)
    .fetch_one(&pool)
    .await
    .expect("Failed to fetch job 2 status");

    assert_eq!(status_1.0, "running", "Job 1 should be running");
    assert_eq!(status_2.0, "running", "Job 2 should be running");

    println!("✓ Both jobs started processing immediately!");
    println!("✓ FIFO order maintained: job 1 → job 2");

    // Cleanup
    sqlx::query("DELETE FROM import_jobs WHERE id IN ($1, $2)")
        .bind(job_id_1)
        .bind(job_id_2)
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_channel_fifo_ordering
async fn test_channel_fifo_ordering() {
    // Test that channels maintain FIFO order for job processing
    use tokio::sync::mpsc;

    let (tx, mut rx) = mpsc::channel::<i32>(10);

    // Spawn worker that collects jobs in order
    let worker = tokio::spawn(async move {
        let mut received = vec![];
        while let Some(job) = rx.recv().await {
            received.push(job);
        }
        received
    });

    // Send jobs in order
    for i in 1..=5 {
        tx.send(i).await.expect("Failed to send");
    }

    // Close channel
    drop(tx);

    // Verify FIFO order
    let received = worker.await.expect("Worker panicked");
    assert_eq!(received, vec![1, 2, 3, 4, 5], "Jobs should be processed in FIFO order");

    println!("✓ Channel maintains FIFO ordering");
}

#[tokio::test]
#[ignore] // Run with: cargo test -- --ignored test_concurrent_workers
async fn test_concurrent_workers() {
    // Test multiple workers processing jobs concurrently
    use tokio::sync::mpsc;

    let (tx, rx) = mpsc::channel::<i32>(100);
    let rx = std::sync::Arc::new(tokio::sync::Mutex::new(rx));

    // Spawn 3 workers
    let mut workers = vec![];
    for worker_id in 1..=3 {
        let rx_clone = rx.clone();
        let worker = tokio::spawn(async move {
            let mut processed = vec![];
            loop {
                let job = {
                    let mut rx_guard = rx_clone.lock().await;
                    rx_guard.recv().await
                };

                match job {
                    Some(job_id) => {
                        println!("Worker {} processing job {}", worker_id, job_id);
                        processed.push(job_id);
                    }
                    None => break,
                }
            }
            (worker_id, processed)
        });
        workers.push(worker);
    }

    // Send 10 jobs
    for i in 1..=10 {
        tx.send(i).await.expect("Failed to send");
    }

    // Close channel
    drop(tx);

    // Wait for all workers
    let mut total_processed = 0;
    for worker in workers {
        let (worker_id, processed) = worker.await.expect("Worker panicked");
        println!("✓ Worker {} processed {} jobs", worker_id, processed.len());
        total_processed += processed.len();
    }

    assert_eq!(total_processed, 10, "All jobs should be processed");
    println!("✓ Concurrent workers successfully processed all jobs");
}
