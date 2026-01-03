//! Document Indexer for RAG Chat
//! 
//! Handles document parsing (via Docling), chunking, and embedding generation.

use anyhow::{Context, Result};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::PgPool;
use std::path::Path;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::time::Duration;
use serde_json::{json, Value};

use crate::{db, enricher::{self, Enricher}};

pub const CHUNK_SIZE: usize = 512;

// ============================================
// Embedder
// ============================================

/// Local embedding model wrapper using OpenAI-compatible APIs (LM Studio, OpenRouter, etc.)
#[derive(Clone)]
pub struct Embedder {
    client: Arc<reqwest::Client>,
    api_url: String,
    api_key: Option<String>,
    model_name: String,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        let api_url = std::env::var("EMBEDDING_API_URL")
            .unwrap_or_else(|_| "http://localhost:1234/v1".to_string());
        let api_key = std::env::var("EMBEDDING_API_KEY").ok();
        let model_name = std::env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| "qwen/qwen3-embedding-8b".to_string());

        // Configure timeout for embedding requests
        let timeout = std::env::var("EMBEDDING_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(120); // Default 2 minutes for embedding generation

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .connect_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10) // Reuse connections
            .build()?;

        tracing::info!("Initializing Embedder with URL: {}, Model: {}, Timeout: {}s",
            api_url, model_name, timeout);

        Ok(Self {
            client: Arc::new(client),
            api_url,
            api_key,
            model_name,
        })
    }

    pub fn get_model_name(&self) -> &str {
        &self.model_name
    }

    pub fn get_api_url(&self) -> &str {
        &self.api_url
    }

    /// Initialize and verify the embedding model
    pub async fn init(&self) -> Result<()> {
        // Skip model verification for OpenRouter (embedding models aren't in /models endpoint)
        if self.api_url.contains("openrouter.ai") {
            tracing::info!("Skipping model verification for OpenRouter - embedding models not in /models endpoint");
            return Ok(());
        }

        let url = format!("{}/models", self.api_url);

        let response = self.client.get(&url)
            .send()
            .await;

        // If we can't connect to list models, just warn but don't fail hard
        // as some providers might not support this endpoint or have different auth
        let response = match response {
            Ok(res) => res,
            Err(e) => {
                tracing::warn!("Could not verify models at {}: {}", url, e);
                return Ok(());
            }
        };

        if !response.status().is_success() {
            tracing::warn!("Failed to list models: {}", response.status());
            return Ok(());
        }

        let json: serde_json::Value = response.json().await?;
        let models = json["data"].as_array();

        if let Some(models) = models {
            let model_ids: Vec<String> = models.iter()
                .filter_map(|m| m["id"].as_str().map(String::from))
                .collect();

            // Only verify if we got a non-empty model list
            if !model_ids.is_empty() {
                if !model_ids.contains(&self.model_name) {
                    tracing::error!("Model '{}' not found in available models: {:?}", self.model_name, model_ids);
                    anyhow::bail!("Model '{}' not found. Available models: {:?}", self.model_name, model_ids);
                } else {
                    tracing::info!("Verified model '{}' is available", self.model_name);
                }
            } else {
                tracing::warn!("Model list is empty, skipping model verification for '{}'", self.model_name);
            }
        }

        Ok(())
    }

    /// Generate embedding for a single text with retry logic
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.api_url);

        // First attempt
        let mut request = self.client.post(&url);

        // Add authorization header if API key is provided
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .json(&json!({
                "input": text,
                "model": self.model_name
            }))
            .send()
            .await?;

        // Check if model needs loading
        if !response.status().is_success() {
            let error_text = response.text().await?;

            // If model is unloaded or doesn't exist, wait and retry
            if error_text.contains("Model unloaded") || error_text.contains("does not exist") {
                tracing::warn!("Embedding model needs loading, waiting 5 seconds and retrying...");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                // Retry
                let mut retry_request = self.client.post(&url);

                // Add authorization header if API key is provided
                if let Some(api_key) = &self.api_key {
                    retry_request = retry_request.header("Authorization", format!("Bearer {}", api_key));
                }

                let retry_response = retry_request
                    .json(&json!({
                        "input": text,
                        "model": self.model_name
                    }))
                    .send()
                    .await?;

                if !retry_response.status().is_success() {
                    let retry_error = retry_response.text().await?;
                    anyhow::bail!("Embedding API error (after retry): {}", retry_error);
                }

                let json: serde_json::Value = retry_response.json().await?;
                let embedding = json["data"][0]["embedding"]
                    .as_array()
                    .context("Invalid embedding response format")?
                    .iter()
                    .map(|v| v.as_f64().unwrap() as f32)
                    .collect();

                return Ok(embedding);
            } else {
                anyhow::bail!("Embedding API error: {}", error_text);
            }
        }

        let json: serde_json::Value = response.json().await?;
        let embedding = json["data"][0]["embedding"]
            .as_array()
            .context("Invalid embedding response format")?
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();

        Ok(embedding)
    }

    /// Generate embeddings for multiple texts in a single API call
    pub async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.api_url);

        let mut request = self.client.post(&url);

        // Add authorization header if API key is provided
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .json(&json!({
                "input": texts,
                "model": self.model_name
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            anyhow::bail!("Embedding API error: {}", error);
        }

        let json: serde_json::Value = response.json().await?;
        let data = json["data"].as_array().context("Invalid embedding response format")?;
        
        let mut embeddings = Vec::new();
        for item in data {
            let embedding = item["embedding"]
                .as_array()
                .context("Invalid embedding format")?
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }
}

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
pub async fn index_path(pool: &PgPool, embedder: &Embedder, path: &str) -> Result<()> {
    let path = Path::new(path);

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

        // Process documents in parallel batches of 4
        let batch_size = 4;
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
                        index_file(&pool, &embedder, &file_path).await
                    }
                })
                .collect();

            // Execute all documents in batch in parallel
            let results = futures::future::join_all(futures).await;

            // Check results and report
            for (idx_in_batch, result) in results.iter().enumerate() {
                let doc_num = batch_idx * batch_size + idx_in_batch + 1;
                match result {
                    Ok(_) => {
                        tracing::info!("  └─ ✓ Document {}/{} completed", doc_num, total);
                    }
                    Err(e) => {
                        tracing::error!("  └─ ✗ Document {}/{} failed: {}", doc_num, total, e);
                    }
                }
            }
        }

        tracing::info!("🎉 Indexing complete: {} documents processed ({:.2} MB total)\n", total, total_size_mb);
    } else {
        index_file(pool, embedder, path).await?;
    }

    Ok(())
}

/// Index a single file
async fn index_file(pool: &PgPool, embedder: &Embedder, path: &Path) -> Result<()> {
    use std::time::Instant;

    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Skip hidden files or non-document types
    if extension.is_empty() || matches!(extension.as_str(), "ds_store" | "gitignore") {
        return Ok(());
    }

    let start_total = Instant::now();

    // Stage 1: Extract & Enrich Content (Docling + Metadata)
    let start_stage1 = Instant::now();
    tracing::info!("  ├─ Stage 1/5: Extracting & enriching content...");

    let enricher = Enricher::new();
    let (content, metadata) = enricher.enrich_file(path).await?;

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

    let raw_chunks = chunk_text(&content, CHUNK_SIZE);
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

        enricher::enrich_chunk(
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

    let texts_to_embed: Vec<&str> = std::iter::once(&enriched_chunks[0])
        .chain(enriched_chunks.iter())
        .map(|s| s.as_str())
        .collect();

    let embeddings = embedder.embed_batch(texts_to_embed).await?;

    let stage4_duration = start_stage4.elapsed();
    tracing::info!("  │   ✓ Duration: {:.2}s | Embedded {} items", stage4_duration.as_secs_f64(), embeddings.len());

    // Stage 5: Store in Database
    let start_stage5 = Instant::now();
    tracing::info!("  └─ Stage 5/5: Storing in database...");

    let doc_embedding = &embeddings[0];

    // Insert document
    let doc_id = db::insert_document(
        pool,
        db::InsertDocumentParams {
            title: &title,
            content: &content,
            source_path: Some(&path.to_string_lossy()),
            source_type: &extension,
            embedding: doc_embedding,
            summary: metadata.summary.as_deref(),
            keywords: Some(metadata.keywords.clone()),
            entities: Some(metadata.entities.clone()),
            author: metadata.author.as_deref(),
            category_id: metadata.category_id,
        }
    ).await?;

    // Log category assignment if available
    if let Some(ref category) = metadata.category_name {
        tracing::info!("  │   📁 Category: {} (ID: {:?})", category, metadata.category_id);
    }

    // Insert chunks with their embeddings
    for (idx, raw_chunk) in raw_chunks.iter().enumerate() {
        let chunk_embedding = &embeddings[idx + 1]; // +1 because first embedding is for doc
        db::insert_chunk(
            pool,
            doc_id,
            idx as i32,
            raw_chunk,
            chunk_embedding,
            None,
        ).await?;
    }

    let stage5_duration = start_stage5.elapsed();
    let total_duration = start_total.elapsed();

    tracing::info!("      ✓ Duration: {:.2}s | Stored document #{}", stage5_duration.as_secs_f64(), doc_id);
    tracing::info!("  ⏱️  Total time: {:.2}s\n", total_duration.as_secs_f64());

    Ok(())
}

/// Index content from a URL
pub async fn index_url(pool: &PgPool, embedder: &Embedder, url: &str) -> Result<()> {
    tracing::info!("Processing URL: {}", url);
    
    // 1. Enrich Content (Docling + Metadata)
    let enricher = Enricher::new();
    let (content, metadata) = enricher.enrich_url(url).await?;
    
    let title = metadata.title.clone().unwrap_or_else(|| {
        url.split('/').next_back()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Web Document".to_string())
    });

    // Chunking
    let raw_chunks = chunk_text(&content, CHUNK_SIZE);

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

        enricher::enrich_chunk(
            &title,
            metadata.summary.as_deref().unwrap_or(""),
            &metadata.keywords,
            &questions,
            chunk
        )
    }).collect();

    // Embed & Store
    // Batch embed all chunks at once (includes document text)
    let texts_to_embed: Vec<&str> = std::iter::once(&enriched_chunks[0])
        .chain(enriched_chunks.iter())
        .map(|s| s.as_str())
        .collect();

    let embeddings = embedder.embed_batch(texts_to_embed).await?;

    // First embedding is for the document
    let doc_embedding = &embeddings[0];

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
            entities: Some(metadata.entities),
            author: metadata.author.as_deref(),
            category_id: metadata.category_id,
        }
    ).await?;

    for (idx, raw_chunk) in raw_chunks.iter().enumerate() {
        let chunk_embedding = &embeddings[idx + 1]; // +1 because first embedding is for doc
        db::insert_chunk(
            pool,
            doc_id,
            idx as i32,
            raw_chunk,
            chunk_embedding,
            None,
        ).await?;
    }

    tracing::info!("Indexed URL: {} ({})", url, doc_id);
    Ok(())
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
                            if let Err(e) = index_file(pool, embedder, &path).await {
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
    use crate::enricher::enrich_chunk;

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
