use rag_chat::config::Settings;
use rag_chat::services::job_cleanup;
use sqlx::postgres::PgPoolOptions;
use std::sync::Once;
use uuid::Uuid;

static INIT: Once = Once::new();

fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter("info,rag_chat=debug")
            .with_test_writer()
            .init();
    });
}

#[tokio::test]
async fn test_job_cleanup_deletes_old_jobs() {
    init_tracing();
    if std::env::var("RUN_ENV").is_err() {
        if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }
    }

    let settings = Settings::new().expect("Failed to load settings");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    // Clean up any existing test jobs
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '%cleanup_test%'")
        .execute(&pool)
        .await
        .ok();

    // Create an old job (older than retention period)
    let old_job_id = Uuid::new_v4();
    let old_timestamp = chrono::Utc::now() - chrono::Duration::hours(25); // 25 hours old

    sqlx::query(
        "INSERT INTO import_jobs (id, source_type, source_path, status, created_at, completed_at)
         VALUES ($1, 'test', '/cleanup_test/old', 'completed', $2, $2)"
    )
    .bind(old_job_id)
    .bind(old_timestamp)
    .execute(&pool)
    .await
    .expect("Failed to create old test job");

    // Create a recent job (within retention period)
    let recent_job_id = Uuid::new_v4();
    let recent_timestamp = chrono::Utc::now() - chrono::Duration::hours(1); // 1 hour old

    sqlx::query(
        "INSERT INTO import_jobs (id, source_type, source_path, status, created_at, completed_at)
         VALUES ($1, 'test', '/cleanup_test/recent', 'completed', $2, $2)"
    )
    .bind(recent_job_id)
    .bind(recent_timestamp)
    .execute(&pool)
    .await
    .expect("Failed to create recent test job");

    tracing::info!("Created test jobs: old={}, recent={}", old_job_id, recent_job_id);

    // Verify both jobs exist
    let count_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM import_jobs WHERE source_path LIKE '/cleanup_test/%'"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(count_before, 2, "Should have 2 test jobs before cleanup");

    // Run cleanup
    let start = std::time::Instant::now();
    let deleted = job_cleanup::cleanup_old_jobs(&pool, &settings.import.cleanup)
        .await
        .expect("Job cleanup failed");
    let cleanup_duration = start.elapsed();

    tracing::info!("Cleanup deleted {} jobs in {:?}", deleted, cleanup_duration);

    // Verify old job was deleted
    let old_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id = $1)"
    )
    .bind(old_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(!old_exists, "Old job should be deleted");

    // Verify recent job still exists
    let recent_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id = $1)"
    )
    .bind(recent_job_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(recent_exists, "Recent job should still exist");

    // Verify total count
    let count_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM import_jobs WHERE source_path LIKE '/cleanup_test/%'"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(count_after, 1, "Should have 1 job remaining after cleanup");

    tracing::info!("=== Job Cleanup Metrics ===");
    tracing::info!("Jobs before: {}", count_before);
    tracing::info!("Jobs deleted: {}", deleted);
    tracing::info!("Jobs after: {}", count_after);
    tracing::info!("Cleanup time: {:?}", cleanup_duration);

    // Cleanup test data
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '/cleanup_test/%'")
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_job_cleanup_respects_retention_period() {
    init_tracing();
    if std::env::var("RUN_ENV").is_err() {
        if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }
    }

    let settings = Settings::new().expect("Failed to load settings");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    // Clean up
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '%retention_test%'")
        .execute(&pool)
        .await
        .ok();

    // Create jobs at different ages
    let retention_hours = settings.import.cleanup.retention_hours as i64;
    let test_cases = vec![
        (retention_hours + 1, true),  // Should be deleted
        (retention_hours - 1, false), // Should be kept
        (1, false),                    // Should be kept
    ];

    let mut job_ids = Vec::new();

    for (hours_ago, should_delete) in &test_cases {
        let job_id = Uuid::new_v4();
        let timestamp = chrono::Utc::now() - chrono::Duration::hours(*hours_ago);

        sqlx::query(
            "INSERT INTO import_jobs (id, source_type, source_path, status, created_at, completed_at)
             VALUES ($1, 'test', $2, 'completed', $3, $3)"
        )
        .bind(job_id)
        .bind(format!("/retention_test/{}", hours_ago))
        .bind(timestamp)
        .execute(&pool)
        .await
        .unwrap();

        job_ids.push((job_id, should_delete));
        tracing::info!("Created job {} hours old (should_delete={})", hours_ago, should_delete);
    }

    // Run cleanup
    let deleted = job_cleanup::cleanup_old_jobs(&pool, &settings.import.cleanup)
        .await
        .expect("Cleanup failed");

    tracing::info!("Deleted {} jobs (retention period: {} hours)", deleted, retention_hours);

    // Verify each job
    for (job_id, should_delete) in job_ids {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id = $1)"
        )
        .bind(job_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        if *should_delete {
            assert!(!exists, "Job {} should have been deleted", job_id);
        } else {
            assert!(exists, "Job {} should have been kept", job_id);
        }
    }

    // Cleanup
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '/retention_test/%'")
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_job_cleanup_performance_with_many_jobs() {
    init_tracing();
    if std::env::var("RUN_ENV").is_err() {
        if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }
    }

    let settings = Settings::new().expect("Failed to load settings");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    // Clean up
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '%perf_cleanup_test%'")
        .execute(&pool)
        .await
        .ok();

    // Create 20 old jobs and 20 recent jobs
    let old_count = 20;
    let recent_count = 20;
    let old_timestamp = chrono::Utc::now() - chrono::Duration::hours(48); // Use 48h to be safe
    let recent_timestamp = chrono::Utc::now() - chrono::Duration::hours(1);

    tracing::info!("Creating {} old jobs and {} recent jobs", old_count, recent_count);
    let setup_start = std::time::Instant::now();

    for i in 0..old_count {
        sqlx::query(
            "INSERT INTO import_jobs (id, source_type, source_path, status, created_at, completed_at)
             VALUES ($1, 'test', $2, 'completed', $3, $3)"
        )
        .bind(Uuid::new_v4())
        .bind(format!("/perf_cleanup_test/old/{}", i))
        .bind(old_timestamp)
        .execute(&pool)
        .await
        .unwrap();
    }

    for i in 0..recent_count {
        sqlx::query(
            "INSERT INTO import_jobs (id, source_type, source_path, status, created_at, completed_at)
             VALUES ($1, 'test', $2, 'completed', $3, $3)"
        )
        .bind(Uuid::new_v4())
        .bind(format!("/perf_cleanup_test/recent/{}", i))
        .bind(recent_timestamp)
        .execute(&pool)
        .await
        .unwrap();
    }

    let setup_duration = setup_start.elapsed();
    tracing::info!("Setup completed in {:?}", setup_duration);

    // Verify how many jobs are actually eligible for cleanup
    let eligible_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM import_jobs 
         WHERE (completed_at < $1 OR (completed_at IS NULL AND created_at < $1))
         AND status IN ('completed', 'completed_with_errors', 'failed', 'cancelled')
         AND source_path LIKE '/perf_cleanup_test/%'"
    )
    .bind(chrono::Utc::now() - chrono::Duration::hours(settings.import.cleanup.retention_hours as i64))
    .fetch_one(&pool)
    .await
    .unwrap();
    
    tracing::info!("Eligible jobs for cleanup: {}", eligible_count);

    // Run cleanup
    let cleanup_start = std::time::Instant::now();
    let deleted = job_cleanup::cleanup_old_jobs(&pool, &settings.import.cleanup)
        .await
        .expect("Cleanup failed");
    let cleanup_duration = cleanup_start.elapsed();

    tracing::info!("=== Cleanup Performance Metrics ===");
    tracing::info!("Total jobs created: {}", old_count + recent_count);
    tracing::info!("Jobs deleted: {}", deleted);
    tracing::info!("Cleanup time: {:?}", cleanup_duration);
    tracing::info!("Throughput: {:.2} jobs/sec", deleted as f64 / cleanup_duration.as_secs_f64());

    // Verify counts
    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM import_jobs WHERE source_path LIKE '/perf_cleanup_test/%'"
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(remaining >= recent_count as i64, "Should have at least {} recent jobs remaining", recent_count);
    assert!(deleted > 0, "Should have deleted some old jobs (deleted: {})", deleted);
    assert!(cleanup_duration.as_secs() < 10, "Cleanup should complete within 10 seconds");

    // Cleanup
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '/perf_cleanup_test/%'")
        .execute(&pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_job_cleanup_background_task() {
    init_tracing();
    if std::env::var("RUN_ENV").is_err() {
        if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }
    }

    let settings = Settings::new().expect("Failed to load settings");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await
        .expect("Failed to connect to database");

    // Clean up
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '%background_cleanup_test%'")
        .execute(&pool)
        .await
        .ok();

    // Create an old job
    let old_job_id = Uuid::new_v4();
    let old_timestamp = chrono::Utc::now() - chrono::Duration::hours(25);

    sqlx::query(
        "INSERT INTO import_jobs (id, source_type, source_path, status, created_at, completed_at)
         VALUES ($1, 'test', '/background_cleanup_test/old', 'completed', $2, $2)"
    )
    .bind(old_job_id)
    .bind(old_timestamp)
    .execute(&pool)
    .await
    .unwrap();

    // Spawn background cleanup task with short interval for testing
    let mut test_config = settings.import.cleanup.clone();
    test_config.interval_seconds = 2; // Run every 2 seconds for test

    let cleanup_pool = pool.clone();
    let task = job_cleanup::JobCleanupTask::new(cleanup_pool, test_config);
    let cleanup_handle = tokio::spawn(async move {
        task.run().await
    });

    tracing::info!("Background cleanup task started, waiting for job to be deleted...");

    // Wait up to 10 seconds for the job to be deleted
    let mut attempts = 0;
    let mut job_deleted = false;

    while attempts < 10 {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;

        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id = $1)"
        )
        .bind(old_job_id)
        .fetch_one(&pool)
        .await
        .unwrap();

        if !exists {
            job_deleted = true;
            tracing::info!("Job deleted after {} seconds", attempts + 1);
            break;
        }

        attempts += 1;
    }

    // Cancel the background task
    cleanup_handle.abort();

    assert!(job_deleted, "Background cleanup task should have deleted the old job");

    // Cleanup
    sqlx::query("DELETE FROM import_jobs WHERE source_path LIKE '/background_cleanup_test/%'")
        .execute(&pool)
        .await
        .ok();
}
