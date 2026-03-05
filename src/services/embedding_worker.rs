//! Background Embedding Worker
//!
//! Asynchronously generates vector embeddings for documents with 'pending' status.
//! Processes documents that were indexed in BM25-only mode (skip_embedding=true).

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::infra::{embedder::Embedder, db_utils::embedding_to_string};
use crate::services::enrichment;

/// Statistics from background embedding processing
#[derive(Debug, Clone)]
pub struct ProcessingStats {
    pub documents_processed: u32,
    pub embeddings_generated: u32,
    pub failures: u32,
}

/// Process pending documents and generate embeddings asynchronously
///
/// Queries for documents with status='pending' and NULL embeddings,
/// then re-enriches and embeds them in batches.
pub async fn process_pending_embeddings(
    pool: &PgPool,
    embedder: &Embedder,
    limit: usize,
    batch_size: usize,
) -> Result<ProcessingStats> {
    info!("🔄 Starting background embedding processor (limit: {}, batch_size: {})", limit, batch_size);
    
    let mut stats = ProcessingStats {
        documents_processed: 0,
        embeddings_generated: 0,
        failures: 0,
    };

    // Query pending documents
    let pending_docs = sqlx::query_as::<_, (Uuid, String, serde_json::Value, serde_json::Value)>(
        r#"
        SELECT id, title, keywords, entities
        FROM documents
        WHERE status = 'pending' AND embedding IS NULL
        ORDER BY created_at ASC
        LIMIT $1
        "#
    )
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .context("Failed to fetch pending documents")?;

    info!("📋 Found {} documents pending embeddings", pending_docs.len());

    for (doc_id, title, keywords_val, entities_val) in pending_docs {
        info!("  ├─ Processing document: {}", title);

        // Extract keywords
        let keywords: Vec<String> = keywords_val
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Extract questions from entities
        let questions: Vec<String> = entities_val
            .get("questions")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        // Get document chunks
        let chunks = sqlx::query_as::<_, (Uuid, String)>(
            "SELECT id, content FROM document_chunks WHERE document_id = $1 ORDER BY sequence_num ASC"
        )
        .bind(doc_id)
        .fetch_all(pool)
        .await
        .context("Failed to fetch chunks")?;

        if chunks.is_empty() {
            warn!("    ⚠️  No chunks found for document {}", doc_id);
            stats.failures += 1;
            continue;
        }

        // Re-enrich chunks and prepare texts for embedding
        let mut texts_to_embed: Vec<String> = Vec::new();
        let mut chunk_ids: Vec<Uuid> = Vec::new();

        // Add document embedding (use first chunk as representative)
        let first_chunk_text = &chunks[0].1;
        let enriched_doc = enrichment::enrich_chunk(
            &title,
            "",  // Summary not available in this context
            &keywords,
            &questions,
            first_chunk_text,
        );
        texts_to_embed.push(enriched_doc);

        // Add chunk embeddings
        for (chunk_id, chunk_content) in &chunks {
            let enriched_chunk = enrichment::enrich_chunk(
                &title,
                "",
                &keywords,
                &questions,
                chunk_content,
            );
            texts_to_embed.push(enriched_chunk);
            chunk_ids.push(*chunk_id);
        }

        // Generate embeddings with retry logic
        let embeddings = match embed_with_retry(embedder, &texts_to_embed).await {
            Ok(embs) => embs,
            Err(e) => {
                error!("    ✗ Failed to embed after retries: {}", e);
                // Mark document as failed
                let _ = sqlx::query("UPDATE documents SET status = 'failed' WHERE id = $1")
                    .bind(doc_id)
                    .execute(pool)
                    .await;
                stats.failures += 1;
                continue;
            }
        };

        // Verify we got the right number of embeddings
        if embeddings.len() != texts_to_embed.len() {
            error!(
                "    ✗ Embedding count mismatch: expected {}, got {}",
                texts_to_embed.len(),
                embeddings.len()
            );
            stats.failures += 1;
            continue;
        }

        // Update database in transaction
        match update_embeddings_transaction(pool, doc_id, &chunk_ids, &embeddings).await {
            Ok(_) => {
                info!(
                    "    ✓ Updated embeddings for {} chunks + document",
                    chunk_ids.len()
                );
                stats.documents_processed += 1;
                stats.embeddings_generated += embeddings.len() as u32;
            }
            Err(e) => {
                error!("    ✗ Failed to update database: {}", e);
                stats.failures += 1;
            }
        }
    }

    info!(
        "✅ Embedding processing complete: {} docs processed, {} embeddings generated, {} failures",
        stats.documents_processed, stats.embeddings_generated, stats.failures
    );

    Ok(stats)
}

/// Embed texts with retry logic for transient failures
async fn embed_with_retry(
    embedder: &Embedder,
    texts: &[String],
) -> Result<Vec<Vec<f32>>> {
    const MAX_RETRIES: u32 = 3;
    let mut attempt = 0;
    let mut backoff = Duration::from_millis(100);

    loop {
        match embedder.embed_batch(texts.iter().map(|s| s.as_str()).collect()).await {
            Ok(embeddings) => return Ok(embeddings),
            Err(e) => {
                attempt += 1;
                let error_msg = e.to_string();

                // Classify error as transient or permanent
                let is_transient = error_msg.contains("timeout")
                    || error_msg.contains("Connection")
                    || error_msg.contains("429")
                    || error_msg.contains("rate limit");

                if is_transient && attempt < MAX_RETRIES {
                    warn!(
                        "Transient embedding error (attempt {}/{}): {}",
                        attempt, MAX_RETRIES, error_msg
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = Duration::from_millis(backoff.as_millis() as u64 * 2);
                } else if !is_transient {
                    return Err(anyhow::anyhow!("Permanent embedding error: {}", error_msg));
                } else {
                    return Err(anyhow::anyhow!(
                        "Embedding failed after {} retries: {}",
                        MAX_RETRIES,
                        error_msg
                    ));
                }
            }
        }
    }
}

/// Update document and chunk embeddings in a transaction
async fn update_embeddings_transaction(
    pool: &PgPool,
    doc_id: Uuid,
    chunk_ids: &[Uuid],
    embeddings: &[Vec<f32>],
) -> Result<()> {
    let mut tx = pool.begin().await?;

    // Update document embedding (first embedding)
    let doc_embedding_str = embedding_to_string(&embeddings[0]);
    sqlx::query(
        "UPDATE documents SET embedding = $1, status = 'completed' WHERE id = $2"
    )
    .bind(doc_embedding_str)
    .bind(doc_id)
    .execute(&mut *tx)
    .await?;

    // Update chunk embeddings (remaining embeddings)
    for (idx, chunk_id) in chunk_ids.iter().enumerate() {
        let chunk_embedding_str = embedding_to_string(&embeddings[idx + 1]);
        sqlx::query(
            "UPDATE document_chunks SET embedding = $1, status = 'completed' WHERE id = $2"
        )
        .bind(chunk_embedding_str)
        .bind(chunk_id)
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
    fn test_embedding_to_string() {
        let embedding = vec![0.1, 0.2, 0.3];
        let result = embedding_to_string(&embedding);
        assert!(result.contains("0.1"));
        assert!(result.contains("0.2"));
        assert!(result.contains("0.3"));
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
    }

    #[test]
    fn test_processing_stats_creation() {
        let stats = ProcessingStats {
            documents_processed: 10,
            embeddings_generated: 100,
            failures: 2,
        };
        assert_eq!(stats.documents_processed, 10);
        assert_eq!(stats.embeddings_generated, 100);
        assert_eq!(stats.failures, 2);
    }
}
