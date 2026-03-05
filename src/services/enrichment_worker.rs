//! Background Enrichment Worker
//!
//! Asynchronously enriches unenriched documents with:
//! - Summary generation
//! - Keywords extraction
//! - Named entity recognition
//! - Document chunking
//! - Vector embeddings
//!
//! Processes documents that were imported raw (e.g., Wikipedia) without the enrichment pipeline.

use anyhow::{Context, Result};
use sqlx::PgPool;
use std::time::Duration;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::infra::embedder::Embedder;
use crate::services::enrichment::{self, Enricher, DocumentMetadata};

/// Statistics from background enrichment processing
#[derive(Debug, Clone)]
pub struct EnrichmentStats {
    pub documents_processed: u32,
    pub documents_enriched: u32,
    pub chunks_created: u32,
    pub embeddings_generated: u32,
    pub failures: u32,
}

/// Configuration for enrichment worker
#[derive(Debug, Clone)]
pub struct EnrichmentWorkerConfig {
    pub limit: usize,
    pub batch_size: usize,
    pub retry_attempts: u32,
    pub retry_delay_ms: u64,
}

impl Default for EnrichmentWorkerConfig {
    fn default() -> Self {
        Self {
            limit: 1000,
            batch_size: 10,
            retry_attempts: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// Process unenriched documents and generate full metadata + embeddings
///
/// Queries for documents with status='pending' and missing enrichment (no summary/entities),
/// then enriches them with LLM metadata, chunks content, and generates embeddings.
pub async fn process_unenriched_documents(
    pool: &PgPool,
    embedder: &Embedder,
    config: EnrichmentWorkerConfig,
) -> Result<EnrichmentStats> {
    info!(
        "🔄 Starting background enrichment processor (limit: {}, batch_size: {})",
        config.limit, config.batch_size
    );

    let mut stats = EnrichmentStats {
        documents_processed: 0,
        documents_enriched: 0,
        chunks_created: 0,
        embeddings_generated: 0,
        failures: 0,
    };

    let settings = crate::config::Settings::new().ok();
    let enricher = Enricher::with_config(None, None, settings.as_ref());

    let unenriched_docs = sqlx::query_as::<_, (Uuid, String, String)>(
        r#"
        SELECT id, title, content
        FROM documents
        WHERE (summary IS NULL OR summary = '')
          AND (entities IS NULL OR entities = '{}')
          AND LENGTH(content) > 200
          AND status != 'skipped'
        ORDER BY created_at ASC
        LIMIT $1
        "#,
    )
    .bind(config.limit as i64)
    .fetch_all(pool)
    .await
    .context("Failed to fetch unenriched documents")?;

    info!(
        "📋 Found {} documents pending enrichment",
        unenriched_docs.len()
    );

    for (doc_id, title, content) in unenriched_docs {
        info!("  ├─ Processing document: {}", title);
        stats.documents_processed += 1;

        // Skip very short content (Wikipedia redirect pages, stubs, etc)
        if content.len() < 200 {
            info!(
                "    ⏭️  Skipping document {} - content too short ({}B)",
                doc_id,
                content.len()
            );
            // Don't count as failure, just skip
            continue;
        }

        // Limit content size to avoid overwhelming the LLM
        let content_to_process = if content.len() > 50000 {
            info!(
                "    ⚠️  Truncating content from {} to 50KB for LLM processing",
                content.len()
            );
            &content[..50000]
        } else {
            &content
        };

        // Extract enrichment metadata using LLM
        match enricher
            .extract_metadata(content_to_process, &title)
            .await
        {
            Ok(metadata) => {
                info!("    ✓ Metadata extracted");

                // Update document with enrichment data
                if let Err(e) = update_document_enrichment(pool, doc_id, &metadata).await {
                    error!("    ✗ Failed to update document enrichment: {}", e);
                    stats.failures += 1;
                    continue;
                }

                stats.documents_enriched += 1;

                // Stage 2: Chunk the content
                let chunks = chunk_text(&content, 2048);
                info!("    ├─ Created {} chunks", chunks.len());
                stats.chunks_created += chunks.len() as u32;

                // Stage 3: Enrich chunks with metadata
                let enriched_chunks: Vec<String> = chunks
                    .iter()
                    .map(|chunk| {
                        let keywords = metadata
                            .keywords
                            .clone()
                            .into_iter()
                            .take(10)
                            .collect::<Vec<_>>();
                        let questions: Vec<String> = metadata.entities["questions"]
                            .as_array()
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        enrichment::enrich_chunk(
                            &title,
                            metadata.summary.as_deref().unwrap_or(""),
                            &keywords,
                            &questions,
                            chunk,
                        )
                    })
                    .collect();

                // Stage 4: Generate embeddings
                match embed_with_retry(embedder, &enriched_chunks, &config).await {
                    Ok(embeddings) => {
                        info!(
                            "    ├─ Generated {} embeddings",
                            embeddings.len()
                        );

                        // Stage 5: Store chunks with embeddings
                        if let Err(e) = store_chunks_with_embeddings(
                            pool,
                            doc_id,
                            &chunks,
                            &enriched_chunks,
                            &embeddings,
                        )
                        .await
                        {
                            error!("    ✗ Failed to store chunks: {}", e);
                            stats.failures += 1;
                        } else {
                            info!("    ✓ Chunks stored with embeddings");
                            stats.embeddings_generated += embeddings.len() as u32;

                            // Mark document as completed
                            if let Err(e) = sqlx::query("UPDATE documents SET status = 'completed' WHERE id = $1")
                                .bind(doc_id)
                                .execute(pool)
                                .await
                            {
                                error!("    ✗ Failed to mark document as completed: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        error!("    ✗ Failed to generate embeddings: {}", e);
                        // Mark document as failed
                        let _ = sqlx::query("UPDATE documents SET status = 'failed' WHERE id = $1")
                            .bind(doc_id)
                            .execute(pool)
                            .await;
                        stats.failures += 1;
                    }
                }
            }
            Err(e) => {
                error!("    ✗ Failed to extract metadata: {}", e);
                // Mark document as failed
                let _ = sqlx::query("UPDATE documents SET status = 'failed' WHERE id = $1")
                    .bind(doc_id)
                    .execute(pool)
                    .await;
                stats.failures += 1;
            }
        }
    }

    Ok(stats)
}

/// Chunk text into smaller pieces
fn chunk_text(text: &str, target_tokens: usize) -> Vec<String> {
    use text_splitter::{ChunkConfig, TextSplitter};

    let splitter = TextSplitter::new(ChunkConfig::new(target_tokens).with_trim(true));
    splitter.chunks(text).map(|s: &str| s.to_string()).collect()
}

/// Update document with enrichment metadata
async fn update_document_enrichment(
    pool: &PgPool,
    doc_id: Uuid,
    metadata: &DocumentMetadata,
) -> Result<()> {
    let summary = metadata.summary.as_deref().unwrap_or("");
    let keywords = &metadata.keywords;

    sqlx::query(
        r#"
        UPDATE documents 
        SET summary = $1,
            keywords = $2,
            entities = $3,
            author = $4
        WHERE id = $5
        "#,
    )
    .bind(summary)
    .bind(keywords)
    .bind(&metadata.entities)
    .bind(metadata.author.as_deref())
    .bind(doc_id)
    .execute(pool)
    .await
    .context("Failed to update document enrichment")?;

    Ok(())
}

/// Generate embeddings with retry logic
async fn embed_with_retry(
    embedder: &Embedder,
    texts: &[String],
    config: &EnrichmentWorkerConfig,
) -> Result<Vec<Vec<f32>>> {
    let mut attempt = 0;
    loop {
        let str_texts: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
        match embedder.embed_batch(str_texts).await {
            Ok(embeddings) => return Ok(embeddings),
            Err(e) if attempt < config.retry_attempts => {
                attempt += 1;
                warn!(
                    "Embedding failed (attempt {}/{}): {}. Retrying in {}ms...",
                    attempt, config.retry_attempts, e, config.retry_delay_ms
                );
                tokio::time::sleep(Duration::from_millis(config.retry_delay_ms)).await;
            }
            Err(e) => return Err(e),
        }
    }
}

/// Store document chunks with their embeddings
async fn store_chunks_with_embeddings(
    pool: &PgPool,
    doc_id: Uuid,
    chunks: &[String],
    enriched_chunks: &[String],
    embeddings: &[Vec<f32>],
) -> Result<()> {
    if embeddings.len() != chunks.len() {
        return Err(anyhow::anyhow!(
            "Embedding count mismatch: expected {}, got {}",
            chunks.len(),
            embeddings.len()
        ));
    }

    let mut tx = pool.begin().await?;

    for (chunk_idx, ((_, enriched_chunk), embedding)) in chunks
        .iter()
        .zip(enriched_chunks.iter())
        .zip(embeddings.iter())
        .enumerate()
    {
        let chunk_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO document_chunks 
            (id, document_id, chunk_index, content, embedding)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(chunk_id)
        .bind(doc_id)
        .bind(chunk_idx as i32)
        .bind(enriched_chunk)
        .bind(embedding.as_slice())
        .execute(&mut *tx)
        .await
        .context("Failed to insert chunk")?;
    }

    tx.commit().await?;
    Ok(())
}
