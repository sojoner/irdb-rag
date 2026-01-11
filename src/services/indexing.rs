//! Document Indexer Service
//!
//! Handles document parsing (via Docling), chunking, and embedding generation.

use anyhow::{Result, Context};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::PgPool;
use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;
use serde_json::Value;

use crate::infra::{db, embedder::Embedder};
use crate::services::enrichment::{self, Enricher, compute_sha256_hash};

// Default chunk size - configurable via settings.import.chunk_size_tokens
// text-splitter creates chunks ~22% of target size (114 actual with 512 target)
// Default 2048 tokens target yields ~450 actual tokens - optimized for 768-dim embeddings
// GPU: 4096 target → ~900 actual tokens (closer to full embedding dimension)
pub const DEFAULT_CHUNK_SIZE: usize = 2048;

// ============================================
// Document Processing
// ============================================

/// Split text into chunks using text-splitter
fn chunk_text(text: &str, target_tokens: usize) -> Vec<String> {
    use text_splitter::{ChunkConfig, TextSplitter};

    let splitter = TextSplitter::new(ChunkConfig::new(target_tokens).with_trim(true));

    splitter.chunks(text)
        .map(|s: &str| s.to_string())
        .collect()
}

// ============================================
// Indexing Operations
// ============================================


/// Index a file or directory
pub async fn index_path(pool: &PgPool, embedder: &Embedder, path: &str) -> Result<Vec<uuid::Uuid>> {
    index_path_with_config(pool, embedder, path, None).await
}

/// Index a file or directory with custom settings
pub async fn index_path_with_config(pool: &PgPool, embedder: &Embedder, path: &str, settings: Option<&crate::config::Settings>) -> Result<Vec<uuid::Uuid>> {
    let path = Path::new(path);
    let mut indexed_ids = Vec::new();

    if path.is_dir() {
        // Collect all files first to show progress
        let mut files: Vec<_> = walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .collect();

        // Sort by file size (smallest first) for bin packing - quick wins first
        files.sort_by_key(|entry| {
            entry.metadata()
                .ok()
                .map(|m| m.len())
                .unwrap_or(u64::MAX)
        });

        let total = files.len();
        let total_size: u64 = files.iter()
            .filter_map(|e| e.metadata().ok().map(|m| m.len()))
            .sum();

        let total_size_mb = total_size as f64 / (1024.0 * 1024.0);
        tracing::info!("📚 Found {} documents to index ({:.2} MB total)\n", total, total_size_mb);

        // Process documents in parallel batches
        let batch_size = settings.map(|s| s.import.indexing_batch_size).unwrap_or(4);
        let embedder = Arc::new(embedder.clone());

        for (batch_idx, batch) in files.chunks(batch_size).enumerate() {
            let batch_num = batch_idx + 1;
            let total_batches = total.div_ceil(batch_size);

            tracing::info!("⚙️  Processing batch {}/{} ({} documents)\n", batch_num, total_batches, batch.len());

            // Create futures for all documents in this batch
            let futures: Vec<_> = batch
                .iter()
                .enumerate()
                .map(|(idx_in_batch, entry)| {
                    let doc_num = batch_idx * batch_size + idx_in_batch + 1;
                    let file_path = entry.path().to_path_buf();
                    let file_name = file_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let file_size = entry.metadata()
                        .ok()
                        .map(|m| m.len())
                        .unwrap_or(0);
                    let file_size_mb = file_size as f64 / (1024.0 * 1024.0);

                    tracing::info!("  ├─ Document {}/{}: {} ({:.2} MB)", doc_num, total, file_name, file_size_mb);

                    let pool = pool.clone();
                    let embedder = embedder.clone();

                    async move {
                        index_file(&pool, &embedder, &file_path, settings).await
                    }
                })
                .collect();

            // Execute all documents in batch in parallel
            let results = futures::future::join_all(futures).await;

            // Check results and report
            for (idx_in_batch, result) in results.iter().enumerate() {
                let doc_num = batch_idx * batch_size + idx_in_batch + 1;
                match result {
                    Ok(Some(id)) => {
                        tracing::info!("  └─ ✓ Document {}/{} completed", doc_num, total);
                        indexed_ids.push(*id);
                    }
                    Ok(None) => {
                        tracing::info!("  └─ ⏭️ Document {}/{} skipped", doc_num, total);
                    }
                    Err(e) => {
                        tracing::error!("  └─ ✗ Document {}/{} failed: {}", doc_num, total, e);
                    }
                }
            }
        }

        tracing::info!("🎉 Indexing complete: {} documents processed ({:.2} MB total)\n", total, total_size_mb);
    } else if let Some(id) = index_file(pool, embedder, path, settings).await? {
        indexed_ids.push(id);
    }

    Ok(indexed_ids)
}

/// Index a single file
async fn index_file(pool: &PgPool, embedder: &Embedder, path: &Path, settings: Option<&crate::config::Settings>) -> Result<Option<uuid::Uuid>> {
    use std::time::Instant;

    let path_str = path.to_string_lossy().to_string();

    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Skip hidden files or non-document types
    if extension.is_empty() || matches!(extension.as_str(), "ds_store" | "gitignore") {
        return Ok(None);
    }

    // IDEMPOTENCY CHECK 1: Quick check if path exists
    if let Some(existing_id) = db::find_document_by_path(pool, &path_str).await? {
        tracing::info!("  ⏭️  Skipping {} - already indexed (path exists)", path_str);
        return Ok(Some(existing_id));
    }

    let start_total = Instant::now();

    // Read file for hashing (idempotent deduplication) - needed regardless of enrichment
    let file_bytes = tokio::fs::read(path).await
        .context("Failed to read file")?;
    let file_hash = compute_sha256_hash(&file_bytes);

    // Load settings to check enrichment status
    let settings = settings.cloned().or_else(|| crate::config::Settings::new().ok());
    let enrichment_enabled = settings.as_ref()
        .map(|s| s.enrichment.enabled)
        .unwrap_or(true);

    // Stage 1: Extract & Enrich Content (Docling + Metadata)
    let start_stage1 = Instant::now();
    tracing::info!("  ├─ Stage 1/5: Extracting & enriching content...");

    // Reduce timeout when enrichment is disabled to avoid hanging on malformed PDFs
    let timeout = if enrichment_enabled {
        None
    } else {
        Some(60) // 60 seconds timeout for extraction only
    };
    let enricher = Enricher::with_config(None, timeout, settings.as_ref());
    
    let (content, metadata) = if enrichment_enabled {
        enricher.enrich_file(path).await?
    } else {
        // Skip enrichment: just extract content without LLM processing
        let (content, _) = enricher.extract_file_content(path).await?;
        let basic_metadata = enrichment::DocumentMetadata {
            title: path.file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string()),
            summary: None,
            keywords: Vec::new(),
            entities: serde_json::json!({}),
            author: None,
            category_id: None,
            category_name: None,
            docling_metadata: None,
            page_count: None,
            tables: None,
            images: None,
            creation_date: None,
            modification_date: None,
            document_origin: Some(enrichment::DocumentOrigin {
                mimetype: None,
                filename: path.file_name().and_then(|n| n.to_str()).map(String::from),
                binary_hash: Some(file_hash.clone()),
                uri: None,
            }),
            document_structure: None,
            extraction_quality: None,
        };
        (content, basic_metadata)
    };

    // IDEMPOTENCY CHECK 2: Check content hash if available
    if let Some(ref origin) = metadata.document_origin {
        if let Some(ref hash) = origin.binary_hash {
            if let Some(existing_id) = db::find_duplicate_document(pool, &path_str, hash).await? {
                tracing::info!(
                    "  ⏭️  Skipping {} - duplicate content (matches doc {})",
                    path_str, existing_id
                );
                return Ok(Some(existing_id));
            }
        }
    }

    let title = metadata.title.clone().unwrap_or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    });

    let stage1_duration = start_stage1.elapsed();
    tracing::info!("  │   ✓ Duration: {:.2}s", stage1_duration.as_secs_f64());
    tracing::debug!("  │   📄 Title: {}", title);
    tracing::debug!("  │   📝 Summary: {}", metadata.summary.as_deref().unwrap_or("(none)"));
    tracing::debug!("  │   🔑 Keywords: {:?}", metadata.keywords);
    tracing::debug!("  │   👥 Entities: {}", serde_json::to_string_pretty(&metadata.entities).unwrap_or_default());

    // Stage 2: Chunking
    let start_stage2 = Instant::now();
    tracing::info!("  ├─ Stage 2/5: Chunking content...");

    let chunk_size = settings.as_ref().map(|s| s.import.chunk_size_tokens).unwrap_or(DEFAULT_CHUNK_SIZE);
    let raw_chunks = chunk_text(&content, chunk_size);
    let num_chunks = raw_chunks.len();

    let stage2_duration = start_stage2.elapsed();
    tracing::info!("  │   ✓ Duration: {:.2}s | Created {} chunks", stage2_duration.as_secs_f64(), num_chunks);

    // Stage 3: Enrich Chunks
    let start_stage3 = Instant::now();
    tracing::info!("  ├─ Stage 3/5: Enriching chunks...");

    let enriched_chunks: Vec<String> = raw_chunks.iter().map(|chunk| {
        let questions: Vec<String> = metadata.entities["questions"]
            .as_array()
            .map(|arr: &Vec<Value>| {
                arr.iter()
                .filter_map(|v: &Value| v.as_str().map(String::from))
                .collect()
            })
            .unwrap_or_default();

        enrichment::enrich_chunk(
            &title,
            metadata.summary.as_deref().unwrap_or(""),
            &metadata.keywords,
            &questions,
            chunk
        )
    }).collect();

    let stage3_duration = start_stage3.elapsed();
    tracing::info!("  │   ✓ Duration: {:.2}s", stage3_duration.as_secs_f64());

    // Stage 4: Generate Embeddings
    let start_stage4 = Instant::now();
    tracing::info!("  ├─ Stage 4/5: Generating embeddings...");

    // Optimization: Batch embedding requests to avoid overloading Ollama
    // Also fix double-embedding of the first chunk
    let embedding_batch_size = settings.as_ref()
        .map(|s| s.docling.batch_embedding_limit)
        .unwrap_or(64);

    let mut all_embeddings = Vec::new();
    
    // First embedding is for the document itself (using the first enriched chunk as representative)
    // We'll include it in the first batch to save an API call
    let mut texts_to_embed: Vec<&str> = Vec::with_capacity(enriched_chunks.len() + 1);
    if !enriched_chunks.is_empty() {
        texts_to_embed.push(&enriched_chunks[0]); // Document embedding
        for chunk in &enriched_chunks {
            texts_to_embed.push(chunk);
        }
    }

    if !texts_to_embed.is_empty() {
        for chunk in texts_to_embed.chunks(embedding_batch_size) {
            let batch_embeddings = embedder.embed_batch(chunk.to_vec()).await?;
            all_embeddings.extend(batch_embeddings);
        }
    }

    let stage4_duration = start_stage4.elapsed();
    tracing::info!("  │   ✓ Duration: {:.2}s | Embedded {} items", stage4_duration.as_secs_f64(), all_embeddings.len());

    // Stage 5: Store in Database
    let start_stage5 = Instant::now();
    tracing::info!("  └─ Stage 5/5: Storing in database...");

    if all_embeddings.is_empty() {
        anyhow::bail!("No embeddings generated for document: {}", path_str);
    }

    let doc_embedding = &all_embeddings[0];

    // Serialize docling metadata to JSON for storage
    let metadata_json = serde_json::json!({
        "docling_metadata": metadata.docling_metadata,
        "page_count": metadata.page_count,
        "tables": metadata.tables,
        "images": metadata.images,
        "creation_date": metadata.creation_date,
        "modification_date": metadata.modification_date,
        "document_origin": metadata.document_origin,
        "document_structure": metadata.document_structure,
        "extraction_quality": metadata.extraction_quality,
    });

    // Extract content hash for idempotency
    let content_hash = metadata.document_origin
        .as_ref()
        .and_then(|o| o.binary_hash.as_deref());

    // Resolve category: if enrichment provided a category name, get or create the category
    let category_id = if let Some(ref category_name) = metadata.category_name {
        match db::get_or_create_category(pool, category_name).await {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!("Failed to get/create category '{}': {}", category_name, e);
                None
            }
        }
    } else {
        None
    };

    // Extract locations from entities
    let locations = metadata.entities
        .get("locations")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect());

    // Insert document
    let doc_id = db::insert_document(
        pool,
        db::InsertDocumentParams {
            title: &title,
            content: &content,
            source_path: Some(&path_str),
            source_type: &extension,
            embedding: doc_embedding,
            summary: metadata.summary.as_deref(),
            keywords: Some(metadata.keywords.clone()),
            locations,
            entities: Some(metadata.entities.clone()),
            author: metadata.author.as_deref(),
            category_id,
            metadata: Some(metadata_json),
            content_hash,
        }
    ).await?;

    // Log category assignment if available
    if let Some(ref category) = metadata.category_name {
        tracing::info!("  │   📁 Category: {} (ID: {:?})", category, category_id);
    }

    // Insert chunks with their embeddings in a single batch
    let mut chunk_params = Vec::with_capacity(raw_chunks.len());
    for (idx, raw_chunk) in raw_chunks.iter().enumerate() {
        let chunk_embedding = all_embeddings[idx + 1].clone(); // +1 because first embedding is for doc
        chunk_params.push(db::InsertChunkParams {
            chunk_index: idx as i32,
            content: raw_chunk.clone(),
            embedding: chunk_embedding,
            page_number: None, // Could be extracted from docling metadata if needed
        });
    }

    db::insert_chunks_batch(pool, doc_id, chunk_params).await?;

    // Store document assets (images, figures, etc.) if extracted by Docling
    if let Some(ref images) = metadata.images {
        tracing::info!("  │   💾 Storing {} extracted images...", images.len());
        for (idx, image) in images.iter().enumerate() {
            // Extract image metadata if available
            let alt_text = image.get("alt_text")
                .and_then(|v| v.as_str())
                .or_else(|| image.get("description").and_then(|v| v.as_str()));

            let page_num = image.get("page_num")
                .or_else(|| image.get("page_number"))
                .and_then(|v| v.as_i64())
                .map(|n| n as i32);

            match db::insert_asset(
                pool,
                doc_id,
                "image",
                page_num,
                alt_text,
                None,
                Some(&image),
            ).await {
                Ok(_) => {
                    tracing::debug!("    ✓ Image {} stored", idx + 1);
                }
                Err(e) => {
                    tracing::warn!("    ⚠️  Failed to store image {}: {}", idx + 1, e);
                }
            }
        }
        tracing::info!("  │   ✓ Images stored");
    }

    let stage5_duration = start_stage5.elapsed();
    let total_duration = start_total.elapsed();

    tracing::info!("      ✓ Duration: {:.2}s | Stored document #{}", stage5_duration.as_secs_f64(), doc_id);
    tracing::info!("  ⏱️  Total time: {:.2}s\n", total_duration.as_secs_f64());

    Ok(Some(doc_id))
}

/// Index content from a URL
pub async fn index_url(pool: &PgPool, embedder: &Embedder, url: &str) -> Result<Option<uuid::Uuid>> {
    index_url_with_config(pool, embedder, url, None).await
}

/// Index content from a URL with custom settings
pub async fn index_url_with_config(pool: &PgPool, embedder: &Embedder, url: &str, settings: Option<&crate::config::Settings>) -> Result<Option<uuid::Uuid>> {
    tracing::info!("Processing URL: {}", url);

    // Check enrichment status
    let enrichment_enabled = settings
        .map(|s| s.enrichment.enabled)
        .unwrap_or(true);

    // 1. Enrich Content (Docling + Metadata)
    // Reduce timeout when enrichment is disabled
    let timeout = if enrichment_enabled {
        None
    } else {
        Some(60)
    };
    let enricher = Enricher::with_config(None, timeout, settings);
    
    let (content, metadata) = if enrichment_enabled {
        enricher.enrich_url(url).await?
    } else {
        // Skip enrichment: just extract content without LLM processing
        let content = enricher.extract_url_content(url).await?;
        let basic_metadata = enrichment::DocumentMetadata {
            title: url.split('/').next_back().map(|s| s.to_string()),
            summary: None,
            keywords: Vec::new(),
            entities: serde_json::json!({}),
            author: None,
            category_id: None,
            category_name: None,
            docling_metadata: None,
            page_count: None,
            tables: None,
            images: None,
            creation_date: None,
            modification_date: None,
            document_origin: Some(enrichment::DocumentOrigin {
                mimetype: Some("text/html".to_string()),
                filename: None,
                binary_hash: None,
                uri: Some(url.to_string()),
            }),
            document_structure: None,
            extraction_quality: None,
        };
        (content, basic_metadata)
    };
    
    let title = metadata.title.clone().unwrap_or_else(|| {
        url.split('/').next_back()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Web Document".to_string())
    });

    // Chunking
    let chunk_size = settings.map(|s| s.import.chunk_size_tokens).unwrap_or(DEFAULT_CHUNK_SIZE);
    let raw_chunks = chunk_text(&content, chunk_size);

    // Enrich Chunks
    let enriched_chunks: Vec<String> = raw_chunks.iter().map(|chunk| {
        let questions: Vec<String> = metadata.entities["questions"]
            .as_array()
            .map(|arr: &Vec<Value>| {
                arr.iter()
                .filter_map(|v: &Value| v.as_str().map(String::from))
                .collect()
            })
            .unwrap_or_default();

        enrichment::enrich_chunk(
            &title,
            metadata.summary.as_deref().unwrap_or(""),
            &metadata.keywords,
            &questions,
            chunk
        )
    }).collect();

    // Embed & Store
    // Batch embed all chunks at once (includes document text)
    let mut texts_to_embed: Vec<&str> = Vec::with_capacity(enriched_chunks.len() + 1);
    if !enriched_chunks.is_empty() {
        texts_to_embed.push(&enriched_chunks[0]); // Document embedding
        for chunk in &enriched_chunks {
            texts_to_embed.push(chunk);
        }
    }

    let mut all_embeddings = Vec::new();
    let embedding_batch_size = settings.as_ref()
        .map(|s| s.docling.batch_embedding_limit)
        .unwrap_or(64);

    if !texts_to_embed.is_empty() {
        for chunk in texts_to_embed.chunks(embedding_batch_size) {
            let batch_embeddings = embedder.embed_batch(chunk.to_vec()).await?;
            all_embeddings.extend(batch_embeddings);
        }
    }

    if all_embeddings.is_empty() {
        anyhow::bail!("No embeddings generated for URL: {}", url);
    }

    // First embedding is for the document
    let doc_embedding = &all_embeddings[0];

    // For URLs, metadata will be minimal (no docling-specific fields for now)
    let metadata_json = serde_json::json!({
        "source_url": url,
    });

    // Extract content hash for idempotency (URLs don't have binary_hash usually)
    let content_hash = metadata.document_origin
        .as_ref()
        .and_then(|o| o.binary_hash.as_deref());

    // Resolve category: if enrichment provided a category name, get or create the category
    let category_id = if let Some(ref category_name) = metadata.category_name {
        match db::get_or_create_category(pool, category_name).await {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!("Failed to get/create category '{}': {}", category_name, e);
                None
            }
        }
    } else {
        None
    };

    // Extract locations from entities
    let locations = metadata.entities
        .get("locations")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect());

    let doc_id = db::insert_document(
        pool,
        db::InsertDocumentParams {
            title: &title,
            content: &content,
            source_path: Some(url),
            source_type: "url",
            embedding: doc_embedding,
            summary: metadata.summary.as_deref(),
            keywords: Some(metadata.keywords),
            locations,
            entities: Some(metadata.entities),
            author: metadata.author.as_deref(),
            category_id,
            metadata: Some(metadata_json),
            content_hash,
        }
    ).await?;

    // Insert chunks in batch
    let mut chunk_params = Vec::with_capacity(raw_chunks.len());
    for (idx, raw_chunk) in raw_chunks.iter().enumerate() {
        let chunk_embedding = all_embeddings[idx + 1].clone(); // +1 because first embedding is for doc
        chunk_params.push(db::InsertChunkParams {
            chunk_index: idx as i32,
            content: raw_chunk.clone(),
            embedding: chunk_embedding,
            page_number: None,
        });
    }

    db::insert_chunks_batch(pool, doc_id, chunk_params).await?;

    // Store document assets (images, figures, etc.) if extracted
    if let Some(ref images) = metadata.images {
        tracing::info!("  💾 Storing {} extracted images...", images.len());
        for (idx, image) in images.iter().enumerate() {
            let alt_text = image.get("alt_text")
                .and_then(|v| v.as_str())
                .or_else(|| image.get("description").and_then(|v| v.as_str()));

            let page_num = image.get("page_num")
                .or_else(|| image.get("page_number"))
                .and_then(|v| v.as_i64())
                .map(|n| n as i32);

            match db::insert_asset(
                pool,
                doc_id,
                "image",
                page_num,
                alt_text,
                None,
                Some(&image),
            ).await {
                Ok(_) => {
                    tracing::debug!("    ✓ Image {} stored", idx + 1);
                }
                Err(e) => {
                    tracing::warn!("    ⚠️  Failed to store image {}: {}", idx + 1, e);
                }
            }
        }
    }

    tracing::info!("Indexed URL: {} ({})", url, doc_id);
    Ok(Some(doc_id))
}

/// Watch folders for changes and auto-index
pub async fn watch_folders(
    pool: &PgPool, 
    embedder: &Embedder, 
    folders: Vec<String>
) -> Result<()> {
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                tx.send(event).ok();
            }
        },
        Config::default().with_poll_interval(Duration::from_secs(2)),
    )?;

    for folder in &folders {
        tracing::info!("Watching folder: {}", folder);
        watcher.watch(Path::new(folder), RecursiveMode::Recursive)?;
    }

    loop {
        match rx.recv() {
            Ok(event) => {
                use notify::EventKind;
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    for path in event.paths {
                        if path.is_file() {
                            tracing::info!("Detected change: {:?}", path);
                            if let Err(e) = index_file(pool, embedder, &path, None).await {
                                tracing::error!("Failed to index {:?}: {}", path, e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                tracing::error!("Watch error: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::services::enrichment::enrich_chunk;

    #[test]
    fn test_enrich_chunk() {
        let title = "Test Document";
        let summary = "This is a summary.";
        let keywords = vec!["key1".to_string(), "key2".to_string()];
        let questions = vec!["What is this?".to_string(), "Why?".to_string()];
        let chunk = "This is the chunk content.";

        let enriched = enrich_chunk(title, summary, &keywords, &questions, chunk);

        let expected = "Title: Test Document\nSummary: This is a summary.\nKeywords: key1, key2\nQuestions:\n- What is this?\n- Why?\n---\nThis is the chunk content.";
        
        assert_eq!(enriched, expected);
    }

    #[test]
    fn test_enrich_chunk_no_questions() {
        let title = "Test Document";
        let summary = "This is a summary.";
        let keywords = vec!["key1".to_string()];
        let questions: Vec<String> = vec![];
        let chunk = "Chunk content.";

        let enriched = enrich_chunk(title, summary, &keywords, &questions, chunk);

        let expected = "Title: Test Document\nSummary: This is a summary.\nKeywords: key1\nQuestions:\n\n---\nChunk content.";
        
        assert_eq!(enriched, expected);
    }
}
