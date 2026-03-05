use anyhow::Result;
use sqlx::PgPool;
use std::path::PathBuf;
use uuid::Uuid;
use walkdir::WalkDir;
use crate::services::import::{ImportJobRunner, ImportConfig};

pub async fn import_gutenberg(
    pool: &PgPool,
    job_id: Uuid,
    path: PathBuf,
) -> Result<()> {
    let config = ImportConfig::from_env();
    let runner = ImportJobRunner::new(config);

    tracing::info!("Starting Gutenberg import from {:?}", path);
    runner.update_job_status(pool, job_id, "running").await?;

    let mut total = 0;
    let mut processed = 0;
    let mut failed = 0;

    runner.update_job_progress(pool, job_id, 70000, 0, 0, 0).await?;

    let walker = WalkDir::new(&path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let path = e.path();
            path.extension().is_some_and(|ext| {
                matches!(ext.to_str(), Some("txt") | Some("html"))
            })
        });

    let mut batch = Vec::new();
    for entry in walker {
        if let Ok(content) = std::fs::read_to_string(entry.path()) {
            let filename = entry.file_name().to_string_lossy().to_string();
            let (title, author) = extract_metadata(&filename);

            batch.push((title, author, content));
            total += 1;

            if batch.len() >= 50 {
                match batch_insert_gutenberg(pool, job_id, &batch).await {
                    Ok(_) => processed += batch.len(),
                    Err(e) => {
                        tracing::warn!("Batch insert failed: {}", e);
                        failed += batch.len();
                    }
                }

                if let Err(e) = runner
                    .update_job_progress(pool, job_id, 70000, processed as i32, failed as i32, 0)
                    .await
                {
                    tracing::error!("Failed to update progress: {}", e);
                }

                batch.clear();
            }
        }
    }

    if !batch.is_empty() {
        match batch_insert_gutenberg(pool, job_id, &batch).await {
            Ok(_) => processed += batch.len(),
            Err(e) => {
                tracing::warn!("Final batch insert failed: {}", e);
                failed += batch.len();
            }
        }
    }

    runner.complete_job(pool, job_id, "completed", None).await?;
    tracing::info!(
        "Gutenberg import completed: total={}, processed={}, failed={}",
        total,
        processed,
        failed
    );

    Ok(())
}

fn extract_metadata(filename: &str) -> (String, String) {
    let without_ext = filename.trim_end_matches(".txt").trim_end_matches(".html");
    let parts: Vec<&str> = without_ext.split('_').collect();

    let title = if parts.len() >= 2 {
        parts[..parts.len() - 1].join(" ")
    } else {
        without_ext.to_string()
    };

    let author = parts.last().unwrap_or(&"Unknown").to_string();

    (title, author)
}

async fn batch_insert_gutenberg(
    pool: &PgPool,
    _job_id: Uuid,
    batch: &[(String, String, String)],
) -> Result<()> {
    let mut tx = pool.begin().await?;

    for (title, author, content) in batch {
        let doc_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO documents (id, title, content, status, source_type, author)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(doc_id)
        .bind(title)
        .bind(content)
        .bind("pending")
        .bind("gutenberg")
        .bind(author)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_metadata() {
        let (title, author) = extract_metadata("The_Great_Gatsby_F_Scott_Fitzgerald.txt");
        assert_eq!(title, "The Great Gatsby F Scott");
        assert_eq!(author, "Fitzgerald");
    }

    #[test]
    fn test_extract_metadata_simple() {
        let (title, author) = extract_metadata("Pride_Prejudice_Jane_Austen.txt");
        assert_eq!(title, "Pride Prejudice Jane");
        assert_eq!(author, "Austen");
    }

    #[test]
    fn test_extract_metadata_single_word() {
        let (title, author) = extract_metadata("test.txt");
        assert_eq!(title, "test");
        assert_eq!(author, "test");
    }
}
