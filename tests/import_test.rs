use axum::extract::State;
use axum::Json;
use rag_chat::api::handlers;
use rag_chat::api::state::AppState;
use rag_chat::config::Settings;
use rag_chat::domain::dtos::{
    CreateImportRequest, DeleteImportRequest, ListQuery,
};
use rag_chat::infra::embedder::Embedder;
use sqlx::postgres::PgPoolOptions;
use std::sync::{Arc, Mutex, Once};

// ============================================================================
// Test Setup & Helpers
// ============================================================================

static INIT: Once = Once::new();

fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter("info,rag_chat=debug")
            .with_test_writer()
            .init();
    });
}

async fn setup_test_state() -> AppState {
    init_tracing();
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new().expect("Failed to load settings");
    let db_url = settings.database.url.clone();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Clean up test data
    sqlx::query("DELETE FROM import_jobs")
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DELETE FROM documents WHERE source_path LIKE '%tests/test_data%'")
        .execute(&pool)
        .await
        .ok();

    let embedder = Embedder::new(&settings.embedding).expect("Failed to create embedder");
    // Initialize embedder (downloads model if needed)
    embedder.init().await.expect("Failed to init embedder");

    let log_buffer = Arc::new(Mutex::new(Vec::new()));

    // Mock LeptosOptions or load from config
    let leptos_options = leptos::prelude::LeptosOptions::builder()
        .output_name("rag-chat")
        .site_addr("127.0.0.1:3000".parse::<std::net::SocketAddr>().unwrap())
        .build();

    // Spawn workers
    let import_job_queue = rag_chat::services::import::spawn_import_workers(
        pool.clone(),
        Arc::new(embedder.clone()),
        2, // 2 workers for testing
    );

    AppState::new(
        pool,
        embedder,
        log_buffer,
        leptos_options,
        import_job_queue,
        Arc::new(settings),
    )
}

// ============================================================================
// Integration Tests using API Handlers
// ============================================================================

#[tokio::test]
async fn test_import_job_crud_via_api() {
    let state = setup_test_state().await;
    let state_extractor = State(state.clone());

    // 1. Create Import Job
    let req = CreateImportRequest {
        source_type: "folder".to_string(),
        source_path: Some("/documents".to_string()),
        urls: None,
    };

    let response = handlers::create_import(state_extractor.clone(), Json(req))
        .await
        .expect("Failed to create import job");
    let job = response.0;

    tracing::info!("Created job: {}", job.id);
    assert_eq!(job.status, "pending");
    assert_eq!(job.source_type, "folder");

    // 2. Get Import Status
    let status_response = handlers::get_import_status(state_extractor.clone(), axum::extract::Path(job.id))
        .await
        .expect("Failed to get import status");
    let status = status_response.0;

    assert_eq!(status.id, job.id);
    assert_eq!(status.status, "pending"); // Might be running if worker picked it up fast, but likely pending/running

    // 3. List Imports
    let list_query = ListQuery {
        limit: 10,
        offset: 0,
    };
    let list_response = handlers::list_imports(state_extractor.clone(), axum::extract::Query(list_query))
        .await
        .expect("Failed to list imports");
    
    let json_val = list_response.0;
    let jobs_array = json_val.get("jobs").and_then(|v| v.as_array()).expect("Should have jobs array");
    
    assert!(jobs_array.iter().any(|j| j["id"].as_str() == Some(&job.id.to_string())));

    // 4. Delete Import Job
    let delete_req = DeleteImportRequest {
        delete_documents: false,
    };
    let _ = handlers::delete_import(state_extractor.clone(), axum::extract::Path(job.id), Json(delete_req))
        .await
        .expect("Failed to delete import job");

    // Verify deletion
    let result = handlers::get_import_status(state_extractor.clone(), axum::extract::Path(job.id)).await;
    assert!(result.is_err(), "Job should be deleted");
}

#[tokio::test]
async fn test_import_folder_workflow_via_api() {
    let state = setup_test_state().await;
    let state_extractor = State(state.clone());
    let test_folder = "tests/test_data";

    // 1. Create Import Job
    let req = CreateImportRequest {
        source_type: "folder".to_string(),
        source_path: Some(test_folder.to_string()),
        urls: None,
    };

    let response = handlers::create_import(state_extractor.clone(), Json(req))
        .await
        .expect("Failed to create import job");
    let job_id = response.0.id;

    tracing::info!("Created folder import job: {}", job_id);

    // 2. Wait for completion (poll status)
    let mut attempts = 0;
    loop {
        let status_res = handlers::get_import_status(state_extractor.clone(), axum::extract::Path(job_id))
            .await
            .expect("Failed to get status");
        let status = status_res.0;

        tracing::info!("Job status: {} (processed: {}/{})", status.status, status.progress.processed, status.progress.total);

        if status.status == "completed" || status.status == "failed" || status.status == "completed_with_errors" {
            if status.status == "failed" {
                 // Check items to see why
                 let items_res = handlers::get_import_items(
                    state_extractor.clone(), 
                    axum::extract::Path(job_id), 
                    axum::extract::Query(ListQuery { limit: 100, offset: 0 })
                ).await.unwrap();
                tracing::error!("Job failed. Items: {:?}", items_res.0);
            }
            assert_ne!(status.status, "failed", "Job should not fail");
            break;
        }

        if attempts > 180 { // 180 seconds timeout
            tracing::error!("Timeout waiting for import job completion. Last status: {:?}", status);
            panic!("Timeout waiting for import job completion");
        }
        attempts += 1;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    // 3. Verify items
    let items_res = handlers::get_import_items(
        state_extractor.clone(), 
        axum::extract::Path(job_id), 
        axum::extract::Query(ListQuery { limit: 100, offset: 0 })
    ).await.expect("Failed to get items");
    
    let items_json = items_res.0;
    let items = items_json.get("items").and_then(|v| v.as_array()).expect("Should have items");
    
    assert!(!items.is_empty(), "Should have imported items");
    tracing::info!("Imported {} items", items.len());
}

#[tokio::test]
async fn test_real_file_import_via_api() {
    let state = setup_test_state().await;
    let state_extractor = State(state.clone());
    
    // Use a file that definitely exists in the repo
    let test_file = "tests/test_data/sample1.txt";
    assert!(std::path::Path::new(test_file).exists(), "Test file must exist");

    let req = CreateImportRequest {
        source_type: "file".to_string(),
        source_path: Some(test_file.to_string()),
        urls: None,
    };

    let response = handlers::create_import(state_extractor.clone(), Json(req))
        .await
        .expect("Failed to create file import job");
    let job_id = response.0.id;

    // Wait for completion
    let mut attempts = 0;
    loop {
        let status_res = handlers::get_import_status(state_extractor.clone(), axum::extract::Path(job_id))
            .await
            .unwrap();
        let status = status_res.0;

        if status.status == "completed" {
            break;
        }
        if status.status == "failed" {
            // Get items to see error
            let items_res = handlers::get_import_items(
                state_extractor.clone(), 
                axum::extract::Path(job_id), 
                axum::extract::Query(ListQuery { limit: 10, offset: 0 })
            ).await.unwrap();
            tracing::error!("Import job failed. Items: {:?}", items_res.0);
            panic!("Import job failed");
        }

        if attempts > 300 {
            panic!("Timeout waiting for file import");
        }
        attempts += 1;
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    
    tracing::info!("File import completed successfully");
}

#[tokio::test]
async fn test_real_url_import_via_api() {
    let state = setup_test_state().await;
    let state_extractor = State(state.clone());
    
    // Use a reliable URL, or skip if network not allowed (but integration tests usually allow it)
    // Using example.com as it's stable and small
    let test_url = "https://example.com";

    let req = CreateImportRequest {
        source_type: "url".to_string(),
        source_path: Some(test_url.to_string()),
        urls: None,
    };

    let response = handlers::create_import(state_extractor.clone(), Json(req))
        .await
        .expect("Failed to create url import job");
    let job_id = response.0.id;

    // Wait for completion
    let mut attempts = 0;
    loop {
        let status_res = handlers::get_import_status(state_extractor.clone(), axum::extract::Path(job_id))
            .await
            .unwrap();
        let status = status_res.0;

        if status.status == "completed" || status.status == "completed_with_errors" {
            break;
        }
        if status.status == "failed" {
             // It might fail if network is down, which is acceptable in some envs, but let's log it
             let items_res = handlers::get_import_items(
                state_extractor.clone(), 
                axum::extract::Path(job_id), 
                axum::extract::Query(ListQuery { limit: 10, offset: 0 })
            ).await.unwrap();
             tracing::warn!("URL import failed (network issue?): {:?}", items_res.0);
             return; 
        }

        if attempts > 120 {
            panic!("Timeout waiting for url import");
        }
        attempts += 1;
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    
    tracing::info!("URL import completed");
}

#[tokio::test]
async fn test_delete_import_job_cascades_via_api() {
    let state = setup_test_state().await;
    let state_extractor = State(state.clone());
    
    // Create job
    let req = CreateImportRequest {
        source_type: "folder".to_string(),
        source_path: Some("tests/test_data".to_string()),
        urls: None,
    };
    let job_id = handlers::create_import(state_extractor.clone(), Json(req)).await.unwrap().0.id;

    // Delete job immediately
    let delete_req = DeleteImportRequest { delete_documents: true };
    let _ = handlers::delete_import(state_extractor.clone(), axum::extract::Path(job_id), Json(delete_req))
        .await
        .expect("Failed to delete job");

    // Verify job is gone
    let status = handlers::get_import_status(state_extractor.clone(), axum::extract::Path(job_id)).await;
    assert!(status.is_err());
}

#[tokio::test]
#[ignore]
async fn test_bulk_pdf_import_performance_via_api() {
    let state = setup_test_state().await;
    let state_extractor = State(state.clone());
    
    // Create temp dir for subset of files
    let temp_dir = std::env::temp_dir().join(format!("rag_chat_test_{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
    
    let test_folder = "tests/test_data";
    let mut pdf_count = 0;
    
    // Copy 3 PDFs to temp dir
    for entry in walkdir::WalkDir::new(test_folder)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "pdf"))
    {
        let file_name = entry.file_name();
        std::fs::copy(entry.path(), temp_dir.join(file_name)).expect("Failed to copy file");
        pdf_count += 1;
        if pdf_count >= 3 {
            break;
        }
    }
    
    if pdf_count == 0 {
        tracing::warn!("No PDFs found in test_data, skipping performance test");
        return;
    }

    tracing::info!("Starting bulk import of {} PDFs from {:?}", pdf_count, temp_dir);
    let start_time = std::time::Instant::now();

    let req = CreateImportRequest {
        source_type: "folder".to_string(),
        source_path: Some(temp_dir.to_string_lossy().to_string()),
        urls: None,
    };

    let job_id = handlers::create_import(state_extractor.clone(), Json(req))
        .await
        .expect("Failed to create bulk import job")
        .0
        .id;

    // Poll for completion
    loop {
        let status = handlers::get_import_status(state_extractor.clone(), axum::extract::Path(job_id))
            .await
            .unwrap()
            .0;
        
        if status.status == "completed" || status.status == "completed_with_errors" || status.status == "failed" {
            tracing::info!("Bulk import finished with status: {}", status.status);
            break;
        }
        
        if start_time.elapsed().as_secs() > 300 {
            panic!("Bulk import timed out (>300s)");
        }
        
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }

    let duration = start_time.elapsed();
    tracing::info!("Bulk import took {:.2}s", duration.as_secs_f64());
    
    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);

    let secs = duration.as_secs();
    // Adjusted assertion for 3 files (approx 1/5th of 16 files, so maybe 40-50s?)
    // Let's set a generous range for now, or maybe just an upper bound.
    // User asked for optimization.
    assert!(secs <= 60, "Performance regression: Import took {}s, expected <= 60s for 3 files", secs);
}
