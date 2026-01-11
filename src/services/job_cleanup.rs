//! Job cleanup background task
//!
//! Automatically deletes old import jobs and associated data based on
//! retention policy configured in JobCleanupConfig.

use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::PgPool;
use tokio::time;

use crate::config::JobCleanupConfig;

/// Background task for cleaning up old import jobs
pub struct JobCleanupTask {
    pool: PgPool,
    config: JobCleanupConfig,
}

impl JobCleanupTask {
    /// Create a new cleanup task
    pub fn new(pool: PgPool, config: JobCleanupConfig) -> Self {
        Self { pool, config }
    }

    /// Run the cleanup task (blocking loop)
    /// This should be spawned in a separate tokio task
    pub async fn run(self) -> Result<()> {
        if !self.config.enabled {
            tracing::info!("Job cleanup is disabled");
            return Ok(());
        }

        let interval_secs = if self.config.interval_seconds > 0 {
            self.config.interval_seconds
        } else {
            self.config.check_interval_hours * 3600
        };

        let mut interval = time::interval(time::Duration::from_secs(interval_secs));

        tracing::info!(
            "Started job cleanup task (retention: {}h, interval: {}s)",
            self.config.retention_hours,
            interval_secs
        );

        loop {
            interval.tick().await;

            if let Err(e) = self.cleanup_old_jobs().await {
                tracing::error!("Job cleanup failed: {}", e);
            }
        }
    }

    /// Delete jobs older than retention period
    pub async fn cleanup_old_jobs(&self) -> Result<u64> {
        let cutoff_time = Utc::now() - Duration::hours(self.config.retention_hours as i64);

        // Find jobs to delete
        // We check both completed_at and created_at (as fallback for jobs that never completed)
        let jobs_to_delete: Vec<(uuid::Uuid,)> = sqlx::query_as(
            r#"
            SELECT id
            FROM import_jobs
            WHERE (completed_at < $1 OR (completed_at IS NULL AND created_at < $1))
            AND status IN ('completed', 'completed_with_errors', 'failed', 'cancelled')
            "#,
        )
        .bind(cutoff_time)
        .fetch_all(&self.pool)
        .await?;

        if jobs_to_delete.is_empty() {
            return Ok(0);
        }

        let count = jobs_to_delete.len() as u64;
        tracing::info!("Found {} jobs to clean up", count);

        for (job_id,) in jobs_to_delete {
            // Delete associated import items first
            let items_deleted = sqlx::query("DELETE FROM import_items WHERE job_id = $1")
                .bind(job_id)
                .execute(&self.pool)
                .await?;

            // Delete the job itself
            sqlx::query("DELETE FROM import_jobs WHERE id = $1")
                .bind(job_id)
                .execute(&self.pool)
                .await?;

            tracing::info!(
                "Cleaned up job {} ({} items deleted)",
                job_id,
                items_deleted.rows_affected()
            );
        }

        tracing::info!("Completed cleanup of {} jobs", count);
        Ok(count)
    }
}

/// Delete jobs older than retention period (standalone function for testing)
pub async fn cleanup_old_jobs(pool: &PgPool, config: &JobCleanupConfig) -> Result<u64> {
    let task = JobCleanupTask::new(pool.clone(), config.clone());
    task.cleanup_old_jobs().await
}

/// Spawn a background cleanup task
///
/// This spawns a tokio task that runs cleanup on the configured interval.
pub fn spawn_cleanup_task(pool: PgPool, config: &JobCleanupConfig) {
    let config = config.clone();
    tokio::spawn(async move {
        let task = JobCleanupTask::new(pool, config);
        if let Err(e) = task.run().await {
            tracing::error!("Job cleanup task failed: {}", e);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cleanup_config_defaults() {
        let config = JobCleanupConfig::default();
        assert!(config.enabled);
        assert_eq!(config.retention_hours, 24);
        assert_eq!(config.check_interval_hours, 1);
    }
}
