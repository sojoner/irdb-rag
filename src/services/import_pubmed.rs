use anyhow::Result;
use sqlx::PgPool;
use std::path::PathBuf;
use uuid::Uuid;
use serde_json::Value;
use crate::services::import::{ImportJobRunner, ImportConfig};

#[derive(Debug, Clone)]
pub struct PubMedPaper {
    pub title: String,
    pub abstract_text: String,
    pub authors: Vec<String>,
    pub journal: Option<String>,
    pub doi: Option<String>,
    pub publish_time: Option<String>,
}

pub async fn import_pubmed(
    pool: &PgPool,
    job_id: Uuid,
    path: PathBuf,
) -> Result<()> {
    let config = ImportConfig::from_env();
    let runner = ImportJobRunner::new(config);

    tracing::info!("Starting PubMed import from {:?}", path);
    runner.update_job_status(pool, job_id, "running").await?;

    let mut total = 0;
    let mut processed = 0;
    let mut failed = 0;

    runner.update_job_progress(pool, job_id, 200000, 0, 0, 0).await?;

    let walker = walkdir::WalkDir::new(&path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| ext == "json")
        });

    let mut batch = Vec::new();
    for entry in walker {
        match std::fs::read_to_string(entry.path()) {
            Ok(content) => match parse_json_paper(&content) {
                Ok(paper) => {
                    batch.push(paper);
                    total += 1;

                    if batch.len() >= 100 {
                        match batch_insert_pubmed(pool, job_id, &batch).await {
                            Ok(_) => processed += batch.len(),
                            Err(e) => {
                                tracing::warn!("Batch insert failed: {}", e);
                                failed += batch.len();
                            }
                        }

                        if let Err(e) = runner
                            .update_job_progress(
                                pool,
                                job_id,
                                200000,
                                processed as i32,
                                failed as i32,
                                0,
                            )
                            .await
                        {
                            tracing::error!("Failed to update progress: {}", e);
                        }

                        batch.clear();
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to parse paper from {:?}: {}", entry.path(), e);
                    failed += 1;
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read file {:?}: {}", entry.path(), e);
                failed += 1;
            }
        }
    }

    if !batch.is_empty() {
        match batch_insert_pubmed(pool, job_id, &batch).await {
            Ok(_) => processed += batch.len(),
            Err(e) => {
                tracing::warn!("Final batch insert failed: {}", e);
                failed += batch.len();
            }
        }
    }

    runner.complete_job(pool, job_id, "completed", None).await?;
    tracing::info!(
        "PubMed import completed: total={}, processed={}, failed={}",
        total,
        processed,
        failed
    );

    Ok(())
}

fn parse_json_paper(json_str: &str) -> Result<PubMedPaper> {
    let json: Value = serde_json::from_str(json_str)?;

    let title = json["title"]
        .as_str()
        .unwrap_or("Untitled")
        .to_string();

    let abstract_text = json["abstract"]
        .as_str()
        .or_else(|| json["body_text"].as_array().and_then(|arr| {
            arr.iter()
                .find_map(|item| item.get("text").and_then(|v| v.as_str()))
        }))
        .unwrap_or("")
        .to_string();

    let authors = json["authors"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|author| {
                    author
                        .get("name")
                        .and_then(|n| n.as_str())
                        .map(|s| s.to_string())
                })
                .collect()
        })
        .unwrap_or_default();

    let journal = json["journal"]
        .get("name")
        .and_then(|v| v.as_str())
        .or_else(|| json["metadata"].get("name").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let doi = json["doi"]
        .as_str()
        .or_else(|| json["metadata"].get("doi").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    let publish_time = json["publish_time"]
        .as_str()
        .or_else(|| json["metadata"].get("publish_time").and_then(|v| v.as_str()))
        .map(|s| s.to_string());

    Ok(PubMedPaper {
        title,
        abstract_text,
        authors,
        journal,
        doi,
        publish_time,
    })
}

async fn batch_insert_pubmed(pool: &PgPool, _job_id: Uuid, batch: &[PubMedPaper]) -> Result<()> {
    let mut tx = pool.begin().await?;

    for paper in batch {
        let doc_id = Uuid::new_v4();

        let content = format!(
            "{}\n\n{}",
            paper.title,
            paper.abstract_text
        );

        sqlx::query(
            r#"
            INSERT INTO documents (id, title, content, status, source_type, metadata)
            VALUES ($1, $2, $3, $4, $5, $6)
            "#,
        )
        .bind(doc_id)
        .bind(&paper.title)
        .bind(&content)
        .bind("pending")
        .bind("pubmed")
        .bind(serde_json::json!({
            "authors": paper.authors,
            "journal": paper.journal,
            "doi": paper.doi,
            "publish_time": paper.publish_time,
        }))
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
    fn test_parse_minimal_json() {
        let json = r#"{"title": "Test Paper"}"#;
        let paper = parse_json_paper(json).unwrap();
        assert_eq!(paper.title, "Test Paper");
        assert!(paper.abstract_text.is_empty());
    }

    #[test]
    fn test_parse_full_json() {
        let json = r#"{
            "title": "COVID-19 Study",
            "abstract": "This study investigates COVID-19",
            "authors": [{"name": "John Doe"}, {"name": "Jane Smith"}],
            "journal": {"name": "Nature"},
            "doi": "10.1234/test",
            "publish_time": "2021-01-01"
        }"#;
        let paper = parse_json_paper(json).unwrap();
        assert_eq!(paper.title, "COVID-19 Study");
        assert_eq!(paper.abstract_text, "This study investigates COVID-19");
        assert_eq!(paper.authors.len(), 2);
        assert_eq!(paper.journal, Some("Nature".to_string()));
    }
}
