/// Import Service - Document import with retry/skip logic and resilience
///
/// This service handles:
/// - File discovery from folders, URLs, and uploads
/// - Error classification (transient vs permanent)
/// - Automatic retry with exponential backoff
/// - Job and item tracking in database
/// - Integration with existing indexing pipeline
use std::time::Duration;
use std::path::PathBuf;
use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;
use sqlx::PgPool;

use crate::domain::models::{ImportJob, ImportItem, ErrorType};

// ============================================================================
// Re-export config from config module (TOML-based)
// ============================================================================

pub use crate::config::ImportConfig;

/// Extension trait for ImportConfig to support from_env() for backward compatibility
impl ImportConfig {
    /// Load ImportConfig from settings (for backward compatibility)
    pub fn from_env() -> Self {
        match crate::config::Settings::new() {
            Ok(settings) => settings.import,
            Err(e) => {
                tracing::error!("Failed to load settings for ImportConfig::from_env: {}", e);
                Self {
                    workers: 2,
                    max_retries: 3,
                    retry_base_delay_ms: 1000,
                    retry_max_delay_ms: 30000,
                }
            }
        }
    }
}

// ============================================================================
// Retry Strategy
// ============================================================================

/// Calculate retry delay with exponential backoff and jitter
pub fn calculate_retry_delay(attempt: u32, config: &ImportConfig) -> Duration {
    use std::time::SystemTime;

    let base = config.retry_base_delay_ms as f64;
    let delay = base * 2.0_f64.powi(attempt as i32);
    let capped = delay.min(config.retry_max_delay_ms as f64);

    // Add 10% jitter using system time for deterministic pseudo-randomness
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as f64;
    let jitter = capped * 0.1 * (nanos % 1.0);
    Duration::from_millis((capped + jitter) as u64)
}

/// Determine if an error is transient (retry) or permanent (skip)
pub fn classify_error(error_msg: &str) -> ErrorType {
    ErrorType::classify(error_msg)
}

// ============================================================================
// Job Management
// ============================================================================

pub struct ImportJobRunner {
    pub config: ImportConfig,
}

impl ImportJobRunner {
    pub fn new(config: ImportConfig) -> Self {
        Self { config }
    }

    /// Create a new import job
    pub async fn create_job(
        &self,
        pool: &PgPool,
        source_type: &str,
        source_path: Option<&str>,
    ) -> Result<Uuid> {
        let job_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO import_jobs (id, status, source_type, source_path)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(job_id)
        .bind("pending")
        .bind(source_type)
        .bind(source_path)
        .execute(pool)
        .await?;

        tracing::info!("Created import job: {} ({})", job_id, source_type);
        Ok(job_id)
    }

    /// Get job status and progress
    pub async fn get_job(&self, pool: &PgPool, job_id: Uuid) -> Result<ImportJob> {
        let job = sqlx::query_as::<_, ImportJob>(
            "SELECT * FROM import_jobs WHERE id = $1"
        )
        .bind(job_id)
        .fetch_one(pool)
        .await?;

        Ok(job)
    }

    /// Update job status
    pub async fn update_job_status(
        &self,
        pool: &PgPool,
        job_id: Uuid,
        status: &str,
    ) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE import_jobs
            SET status = $1, started_at = COALESCE(started_at, $2)
            WHERE id = $3
            "#,
        )
        .bind(status)
        .bind(if status == "running" { Some(now) } else { None })
        .bind(job_id)
        .execute(pool)
        .await?;

        tracing::debug!("Updated job {} status to: {}", job_id, status);
        Ok(())
    }

    /// Complete job with final status
    pub async fn complete_job(
        &self,
        pool: &PgPool,
        job_id: Uuid,
        status: &str,
        error_msg: Option<&str>,
    ) -> Result<()> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE import_jobs
            SET status = $1, completed_at = $2, error_message = $3
            WHERE id = $4
            "#,
        )
        .bind(status)
        .bind(now)
        .bind(error_msg)
        .bind(job_id)
        .execute(pool)
        .await?;

        tracing::info!(
            "Completed job {} with status: {}{}",
            job_id,
            status,
            error_msg.map(|e| format!(" (error: {})", e)).unwrap_or_default()
        );
        Ok(())
    }

    /// Update job progress counters
    pub async fn update_job_progress(
        &self,
        pool: &PgPool,
        job_id: Uuid,
        total_items: i32,
        processed_items: i32,
        failed_items: i32,
        skipped_items: i32,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE import_jobs
            SET total_items = $1, processed_items = $2, failed_items = $3, skipped_items = $4
            WHERE id = $5
            "#,
        )
        .bind(total_items)
        .bind(processed_items)
        .bind(failed_items)
        .bind(skipped_items)
        .bind(job_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// List all import jobs
    pub async fn list_jobs(
        &self,
        pool: &PgPool,
        limit: i32,
        offset: i32,
    ) -> Result<(Vec<ImportJob>, i64)> {
        let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM import_jobs")
            .fetch_one(pool)
            .await?;

        let jobs = sqlx::query_as::<_, ImportJob>(
            "SELECT * FROM import_jobs ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((jobs, total.0))
    }

    /// Delete an import job and all associated items (CASCADE)
    /// Optionally delete associated documents if delete_documents is true
    pub async fn delete_job(
        &self,
        pool: &PgPool,
        job_id: Uuid,
        delete_documents: bool,
    ) -> Result<u64> {
        // If delete_documents is true, find and delete all documents linked to this job's items
        if delete_documents {
            // Get all document_ids from import_items for this job
            let document_ids: Vec<(Option<Uuid>,)> = sqlx::query_as(
                "SELECT DISTINCT document_id FROM import_items WHERE job_id = $1 AND document_id IS NOT NULL"
            )
            .bind(job_id)
            .fetch_all(pool)
            .await?;

            // Delete documents and their related data (functional composition with filter_map)
            let deletion_tasks: Vec<_> = document_ids
                .into_iter()
                .filter_map(|(doc_id_opt,)| doc_id_opt)
                .collect();

            for doc_id in deletion_tasks {
                let _ = sqlx::query("DELETE FROM chunks WHERE document_id = $1")
                    .bind(doc_id)
                    .execute(pool)
                    .await;
                let _ = sqlx::query("DELETE FROM documents WHERE id = $1")
                    .bind(doc_id)
                    .execute(pool)
                    .await;
                tracing::debug!("Deleted document and chunks: {}", doc_id);
            }
        }

        // Delete the import job (items will be cascaded due to ON DELETE CASCADE)
        let result = sqlx::query("DELETE FROM import_jobs WHERE id = $1")
            .bind(job_id)
            .execute(pool)
            .await?;

        tracing::info!(
            "Deleted import job {} (delete_documents={}): {} rows affected",
            job_id,
            delete_documents,
            result.rows_affected()
        );

        Ok(result.rows_affected())
    }
}

// ============================================================================
// Item Management
// ============================================================================

pub struct ImportItemManager;

impl ImportItemManager {
    /// Create import items for a job
    pub async fn create_items(
        &self,
        pool: &PgPool,
        job_id: Uuid,
        source_paths: Vec<&str>,
    ) -> Result<Vec<Uuid>> {
        let mut item_ids = vec![];

        for path in source_paths {
            let item_id = Uuid::new_v4();

            sqlx::query(
                r#"
                INSERT INTO import_items (id, job_id, source_path, status)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(item_id)
            .bind(job_id)
            .bind(path)
            .bind("pending")
            .execute(pool)
            .await?;

            item_ids.push(item_id);
        }

        tracing::info!("Created {} import items for job {}", item_ids.len(), job_id);
        Ok(item_ids)
    }

    /// Get item by ID
    pub async fn get_item(&self, pool: &PgPool, item_id: Uuid) -> Result<ImportItem> {
        let item = sqlx::query_as::<_, ImportItem>(
            "SELECT * FROM import_items WHERE id = $1"
        )
        .bind(item_id)
        .fetch_one(pool)
        .await?;

        Ok(item)
    }

    /// Get all items for a job
    pub async fn get_job_items(
        &self,
        pool: &PgPool,
        job_id: Uuid,
        limit: i32,
        offset: i32,
    ) -> Result<(Vec<ImportItem>, i64)> {
        let total: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM import_items WHERE job_id = $1"
        )
        .bind(job_id)
        .fetch_one(pool)
        .await?;

        let items = sqlx::query_as::<_, ImportItem>(
            r#"
            SELECT * FROM import_items
            WHERE job_id = $1
            ORDER BY created_at DESC
            LIMIT $2 OFFSET $3
            "#
        )
        .bind(job_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?;

        Ok((items, total.0))
    }

    /// Get items by status (e.g., pending, failed)
    pub async fn get_items_by_status(
        &self,
        pool: &PgPool,
        job_id: Uuid,
        status: &str,
    ) -> Result<Vec<ImportItem>> {
        let items = sqlx::query_as::<_, ImportItem>(
            "SELECT * FROM import_items WHERE job_id = $1 AND status = $2 ORDER BY created_at"
        )
        .bind(job_id)
        .bind(status)
        .fetch_all(pool)
        .await?;

        Ok(items)
    }

    /// Update item status
    pub async fn update_item_status(
        &self,
        pool: &PgPool,
        item_id: Uuid,
        status: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE import_items
            SET status = $1, processed_at = CASE WHEN $1 != 'pending' THEN NOW() ELSE processed_at END
            WHERE id = $2
            "#,
        )
        .bind(status)
        .bind(item_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Record item error and classify it
    pub async fn record_item_error(
        &self,
        pool: &PgPool,
        item_id: Uuid,
        error_msg: &str,
    ) -> Result<ErrorType> {
        let error_type = classify_error(error_msg);

        sqlx::query(
            r#"
            UPDATE import_items
            SET error_message = $1, error_type = $2, status = $3
            WHERE id = $4
            "#,
        )
        .bind(error_msg)
        .bind(error_type.as_str())
        .bind("failed")
        .bind(item_id)
        .execute(pool)
        .await?;

        tracing::warn!("Recorded item error: {} ({:?})", item_id, error_type);
        Ok(error_type)
    }

    /// Increment retry count
    pub async fn increment_retry_count(
        &self,
        pool: &PgPool,
        item_id: Uuid,
    ) -> Result<i32> {
        let result: (i32,) = sqlx::query_as(
            r#"
            UPDATE import_items
            SET retry_count = retry_count + 1, status = $1
            WHERE id = $2
            RETURNING retry_count
            "#,
        )
        .bind("pending")
        .bind(item_id)
        .fetch_one(pool)
        .await?;

        Ok(result.0)
    }

    /// Mark item as completed with optional document ID
    pub async fn mark_completed(
        &self,
        pool: &PgPool,
        item_id: Uuid,
        document_id: Option<Uuid>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE import_items
            SET status = $1, document_id = $2, processed_at = NOW()
            WHERE id = $3
            "#,
        )
        .bind("completed")
        .bind(document_id)
        .bind(item_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Mark item as failed with retry count and error
    pub async fn mark_failed(
        &self,
        pool: &PgPool,
        item_id: Uuid,
        retry_count: i32,
        error_msg: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE import_items
            SET status = $1, retry_count = $2, error_message = $3, error_type = $4
            WHERE id = $5
            "#,
        )
        .bind("failed")
        .bind(retry_count)
        .bind(error_msg)
        .bind(ErrorType::Transient.as_str())
        .bind(item_id)
        .execute(pool)
        .await?;

        Ok(())
    }

    /// Mark item as skipped
    pub async fn mark_skipped(
        &self,
        pool: &PgPool,
        item_id: Uuid,
        reason: &str,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE import_items
            SET status = $1, error_message = $2, error_type = $3, processed_at = NOW()
            WHERE id = $4
            "#,
        )
        .bind("skipped")
        .bind(reason)
        .bind(ErrorType::Permanent.as_str())
        .bind(item_id)
        .execute(pool)
        .await?;

        tracing::info!("Skipped item: {} ({})", item_id, reason);
        Ok(())
    }
}

// ============================================================================
// File Discovery
// ============================================================================

/// Discover files in a folder
pub fn discover_files(folder: &str) -> Result<Vec<PathBuf>> {
    use walkdir::WalkDir;

    let mut files = vec![];

    for entry in WalkDir::new(folder)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
    {
        let path = entry.path().to_path_buf();
        // Filter for document types
        if is_indexable_file(&path) {
            files.push(path);
        }
    }

    // Sort by file size (quick wins first)
    files.sort_by_key(|p| {
        std::fs::metadata(p)
            .map(|m| m.len())
            .unwrap_or(u64::MAX)
    });

    tracing::info!("Discovered {} indexable files in {}", files.len(), folder);
    Ok(files)
}

/// Check if file should be indexed based on extension
fn is_indexable_file(path: &std::path::Path) -> bool {
    const INDEXABLE_EXTENSIONS: &[&str] = &[
        "pdf", "docx", "pptx", "xlsx",
        "html", "htm", "md", "txt",
        "png", "jpg", "jpeg", "tiff", "tif",
        "c", "cpp", "rs", "py", "js", "ts", "go", "java",
    ];

    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| INDEXABLE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

// ============================================================================
// Error Handling (Functional)
// ============================================================================

/// Handle item processing error with functional error classification
/// Pure decision logic extracted from main loop
async fn handle_item_error(
    stats: &mut ProcessingStats,
    pool: &PgPool,
    item_mgr: &ImportItemManager,
    item: &ImportItem,
    error_type: crate::domain::models::ErrorType,
    error_msg: &str,
    config: &ImportConfig,
) -> Result<()> {
    match error_type {
        crate::domain::models::ErrorType::Transient => {
            if item.retry_count < config.max_retries as i32 {
                let delay = calculate_retry_delay(item.retry_count as u32, config);
                tracing::warn!(
                    "Item {} failed with transient error (attempt {}): {}. Will retry in {:?}",
                    item.id, item.retry_count + 1, error_msg, delay
                );
                item_mgr.mark_failed(pool, item.id, item.retry_count + 1, error_msg).await?;
                tokio::time::sleep(delay).await;
                sqlx::query("UPDATE import_items SET status = 'pending' WHERE id = $1")
                    .bind(item.id)
                    .execute(pool)
                    .await?;
            } else {
                tracing::error!(
                    "Item {} failed after {} retries: {}",
                    item.id, item.retry_count, error_msg
                );
                item_mgr.mark_failed(pool, item.id, item.retry_count + 1, error_msg).await?;
                stats.failed += 1;
            }
        }
        crate::domain::models::ErrorType::Permanent => {
            tracing::warn!("Item {} skipped due to permanent error: {}", item.id, error_msg);
            item_mgr.mark_skipped(pool, item.id, error_msg).await?;
            stats.skipped += 1;
        }
    }
    Ok(())
}

#[derive(Default)]
struct ProcessingStats {
    completed: i32,
    failed: i32,
    skipped: i32,
}

// ============================================================================
// Background Processor
// ============================================================================

/// Process pending import items for a job
/// This function picks up pending items and processes them through the indexing pipeline
pub async fn process_import_job(
    pool: &PgPool,
    embedder: &std::sync::Arc<crate::infra::embedder::Embedder>,
    job_id: Uuid,
) -> Result<()> {
    use crate::services::indexing;

    let settings = crate::config::Settings::new()?;
    let config = settings.import.clone();
    let runner = ImportJobRunner::new(config.clone());
    let item_mgr = ImportItemManager;

    tracing::info!("Starting background processing for job: {}", job_id);

    // Update job status to running
    runner.update_job_status(pool, job_id, "running").await?;

    // Get all pending items
    let items: Vec<ImportItem> = sqlx::query_as(
        "SELECT * FROM import_items WHERE job_id = $1 AND status = 'pending' ORDER BY created_at"
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    if items.is_empty() {
        tracing::info!("No pending items to process for job {}", job_id);
        runner.complete_job(pool, job_id, "completed", None).await?;
        return Ok(());
    }

    let total_items = items.len();
    tracing::info!("Processing {} items for job {}", total_items, job_id);

    let stats = {
        let mut stats = ProcessingStats::default();

        for item in items {
            tracing::info!("Processing item: {} ({})", item.id, item.source_path);
            item_mgr.update_item_status(pool, item.id, "processing").await?;

            let is_url = item.source_path.starts_with("http://") || item.source_path.starts_with("https://");
            let result = if is_url {
                indexing::index_url(pool, embedder, &item.source_path).await
            } else {
                indexing::index_path(pool, embedder, &item.source_path).await
            };

            match result {
                Ok(_) => {
                    item_mgr.mark_completed(pool, item.id, None).await?;
                    stats.completed += 1;
                    tracing::info!("Successfully processed item: {}", item.id);
                }
                Err(e) => {
                    let error_msg = e.to_string();

                    // Check if this is a duplicate skip (not an error)
                    if error_msg.contains("already indexed") || error_msg.contains("duplicate content") {
                        item_mgr.mark_skipped(pool, item.id, "Duplicate document").await?;
                        stats.skipped += 1;
                        tracing::info!("Skipped duplicate item: {}", item.id);
                    } else {
                        let error_type = classify_error(&error_msg);
                        handle_item_error(&mut stats, pool, &item_mgr, &item, error_type, &error_msg, &config).await?;
                    }
                }
            }
        }

        stats
    };

    // Update job progress
    let total = total_items as i32;
    let processed = stats.completed + stats.failed + stats.skipped;
    runner.update_job_progress(pool, job_id, total, processed, stats.failed, stats.skipped).await?;

    // Complete job
    let final_status = if stats.failed == 0 && stats.skipped == 0 {
        "completed"
    } else if stats.completed == 0 {
        "failed"
    } else {
        "completed_with_errors"
    };

    runner.complete_job(pool, job_id, final_status, None).await?;

    tracing::info!(
        "Job {} processing complete: {} completed, {} failed, {} skipped",
        job_id, stats.completed, stats.failed, stats.skipped
    );

    Ok(())
}

/// Start processing an import job in the background
/// This spawns a tokio task that processes the job asynchronously
pub fn spawn_import_processor(
    pool: PgPool,
    embedder: std::sync::Arc<crate::infra::embedder::Embedder>,
    job_id: Uuid,
) {
    tokio::spawn(async move {
        if let Err(e) = process_import_job(&pool, &embedder, job_id).await {
            tracing::error!("Background import processor failed for job {}: {}", job_id, e);

            // Try to mark job as failed
            if let Ok(settings) = crate::config::Settings::new() {
                let config = settings.import;
                let runner = ImportJobRunner::new(config);
                if let Err(update_err) = runner.complete_job(&pool, job_id, "failed", Some(&e.to_string())).await {
                    tracing::error!("Failed to update job status: {}", update_err);
                }
            } else {
                tracing::error!("Failed to load settings for job status update");
            }
        }
    });
}

/// Spawn import job workers that listen to the job queue channel
/// Returns the sender side of the channel for submitting jobs
pub fn spawn_import_workers(
    pool: PgPool,
    embedder: std::sync::Arc<crate::infra::embedder::Embedder>,
    num_workers: usize,
) -> tokio::sync::mpsc::Sender<Uuid> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Uuid>(100);
    let rx = std::sync::Arc::new(tokio::sync::Mutex::new(rx));

    tracing::info!("Starting {} import job workers", num_workers);

    for worker_id in 0..num_workers {
        let pool = pool.clone();
        let embedder = embedder.clone();
        let rx = rx.clone();

        tokio::spawn(async move {
            tracing::info!("Import worker {} started", worker_id);

            loop {
                // Wait for a job from the queue
                let job_id = {
                    let mut rx_guard = rx.lock().await;
                    rx_guard.recv().await
                };

                match job_id {
                    Some(job_id) => {
                        tracing::info!("Worker {} received job {}", worker_id, job_id);

                        // Process the job
                        if let Err(e) = process_import_job(&pool, &embedder, job_id).await {
                            tracing::error!("Worker {} failed to process job {}: {}", worker_id, job_id, e);

                            // Try to mark job as failed
                            if let Ok(settings) = crate::config::Settings::new() {
                                let config = settings.import;
                                let runner = ImportJobRunner::new(config);
                                if let Err(update_err) = runner.complete_job(&pool, job_id, "failed", Some(&e.to_string())).await {
                                    tracing::error!("Worker {} failed to update job status: {}", worker_id, update_err);
                                }
                            } else {
                                tracing::error!("Worker {} failed to load settings", worker_id);
                            }
                        } else {
                            tracing::info!("Worker {} completed job {}", worker_id, job_id);
                        }
                    }
                    None => {
                        // Channel closed, worker should exit
                        tracing::info!("Worker {} shutting down (channel closed)", worker_id);
                        break;
                    }
                }
            }
        });
    }

    tx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_delay_exponential() {
        let config = ImportConfig {
            workers: 2,
            max_retries: 3,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 30000,
        };

        let delay_0 = calculate_retry_delay(0, &config);
        let delay_1 = calculate_retry_delay(1, &config);
        let delay_2 = calculate_retry_delay(2, &config);

        // Each should be roughly 2x the previous (with jitter)
        assert!(delay_0.as_millis() >= 900 && delay_0.as_millis() <= 1100); // ~1000ms ±10%
        assert!(delay_1.as_millis() >= 1800 && delay_1.as_millis() <= 2200); // ~2000ms ±10%
        assert!(delay_2.as_millis() >= 3600 && delay_2.as_millis() <= 4400); // ~4000ms ±10%
    }

    #[test]
    fn test_error_classification() {
        let transient = classify_error("connection timeout");
        assert_eq!(transient, ErrorType::Transient);

        let transient = classify_error("HTTP 503 Service Unavailable");
        assert_eq!(transient, ErrorType::Transient);

        let permanent = classify_error("file not found");
        assert_eq!(permanent, ErrorType::Permanent);

        let permanent = classify_error("unsupported format");
        assert_eq!(permanent, ErrorType::Permanent);
    }

    #[test]
    fn test_is_indexable_file() {
        assert!(is_indexable_file(&PathBuf::from("document.pdf")));
        assert!(is_indexable_file(&PathBuf::from("document.docx")));
        assert!(is_indexable_file(&PathBuf::from("code.rs")));
        assert!(is_indexable_file(&PathBuf::from("image.png")));
        assert!(!is_indexable_file(&PathBuf::from("document.exe")));
        assert!(!is_indexable_file(&PathBuf::from("script.sh")));
    }
}
