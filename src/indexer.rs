//! Document Indexer for RAG Chat
//! 
//! Handles document parsing (via Docling), chunking, and embedding generation.

use anyhow::{Context, Result};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use sqlx::PgPool;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::Duration;
use text_splitter::TextSplitter;
use serde_json::json;

use crate::{db, llm::{self, LLMConfig}};

pub const CHUNK_SIZE: usize = 512;

// ============================================
// Embedder
// ============================================

/// Local embedding model wrapper using LM Studio (OpenAI compatible)
pub struct Embedder {
    client: reqwest::Client,
    api_url: String,
    model_name: String,
}

impl Embedder {
    pub fn new() -> Result<Self> {
        let api_url = std::env::var("EMBEDDING_API_URL")
            .unwrap_or_else(|_| "http://localhost:1234/v1".to_string());
        let model_name = std::env::var("EMBEDDING_MODEL")
            .unwrap_or_else(|_| "Qwen/Qwen3-Embedding-0.6B-GGUF".to_string());

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
            client,
            api_url,
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

            if !model_ids.contains(&self.model_name) {
                tracing::error!("Model '{}' not found in available models: {:?}", self.model_name, model_ids);
                anyhow::bail!("Model '{}' not found. Available models: {:?}", self.model_name, model_ids);
            } else {
                tracing::info!("Verified model '{}' is available", self.model_name);
            }
        }

        Ok(())
    }

    /// Generate embedding for a single text with retry logic
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.api_url);

        // First attempt
        let response = self.client.post(&url)
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
                let retry_response = self.client.post(&url)
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

    /// Generate embeddings for multiple texts
    #[allow(dead_code)]
    pub async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.api_url);
        
        let response = self.client.post(&url)
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
// Docling Client
// ============================================

struct DoclingClient {
    base_url: String,
    client: reqwest::Client,
}

impl DoclingClient {
    fn new() -> Self {
        let timeout = std::env::var("DOCLING_TIMEOUT_SECONDS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(180); // Default 3 minutes for document conversion

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout))
            .connect_timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to create Docling HTTP client");

        Self {
            base_url: std::env::var("DOCLING_URL")
                .unwrap_or_else(|_| "http://localhost:5001".to_string()),
            client,
        }
    }

    async fn convert_file(&self, path: &Path) -> Result<(String, serde_json::Value)> {
        let file_name = path.file_name().unwrap().to_string_lossy().to_string();
        let file_content = tokio::fs::read(path).await?;

        let part = reqwest::multipart::Part::bytes(file_content)
            .file_name(file_name);

        // Enhanced options for better document processing
        // Enable OCR, table structure detection, and image extraction
        let options = json!({
            "do_ocr": true,
            "do_table_structure": true,
            "generate_picture_images": true,
            "generate_page_images": false,  // Set to true if you want full page images
            "images_scale": 2.0,  // Higher resolution for better OCR
            "ocr_engine": "easyocr"  // Can be "easyocr" or "tesseract"
        });

        let form = reqwest::multipart::Form::new()
            .part("files", part)
            .text("options", options.to_string());

        let response = self.client
            .post(format!("{}/v1/convert/file", self.base_url))
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            anyhow::bail!("Docling API error: {}", error);
        }

        let json: serde_json::Value = response.json().await?;

        // Extract metadata from DocLing response
        let doc_metadata = json["document"]["metadata"].clone();

        // Log metadata if available
        if let Some(meta) = doc_metadata.as_object() {
            tracing::debug!("Document metadata: {:?}", meta);
        }

        // Docling returns markdown in the response
        let markdown = json["document"]["md_content"]
            .as_str()
            .context("No markdown in Docling response")?
            .to_string();

        Ok((markdown, doc_metadata))
    }

    async fn convert_url(&self, url: &str) -> Result<String> {
        let body = serde_json::json!({
            "sources": [{"kind": "http", "url": url}],
            "options": {
                "do_ocr": true,
                "do_table_structure": true
            }
        });

        let response = self.client
            .post(format!("{}/v1/convert/source", self.base_url))
            .json(&body)
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            anyhow::bail!("Docling API error: {}", error);
        }

        let json: serde_json::Value = response.json().await?;
        // Response structure might be a list of results
        let markdown = json["results"][0]["markdown"]
            .as_str()
            .context("No markdown in Docling response")?
            .to_string();

        Ok(markdown)
    }
}

// ============================================
// Document Processing
// ============================================

/// Extract metadata using LLM - enhanced with typed entities
pub async fn extract_metadata(
    content: &str,
    docling_meta: Option<&serde_json::Value>
) -> Result<(String, Vec<String>, serde_json::Value, Option<String>)> {
    // Use metadata-specific model (entity extraction)
    let config = LLMConfig::for_metadata();

    // Take first 4k chars for faster processing
    let sample = content.chars().take(4000).collect::<String>();

    // SLIM NER Prompt Format
    // <human>: text \n <classify> params </classify> \n <bot>:
    // We include summary and questions as "categories" to extract
    let params = "persons, organizations, locations, products, concepts, topics, questions, dates, summary";
    let prompt = format!("<human>: {}\n<classify> {} </classify>\n<bot>:", sample, params);

    tracing::debug!("Sending metadata extraction request to model: {}", config.model);
    tracing::debug!("Prompt:\n{}", prompt);

    // Call LLM with empty system prompt and formatted user prompt
    let response = llm::call_llm_with_options(&config, "", &prompt, None, Some(0.1)).await?;

    tracing::debug!("Raw LLM Response:\n{}", response);

    let (summary, keywords, entities) = parse_metadata_response(&response);

    // Extract author from DocLing metadata if available
    let author = docling_meta
        .and_then(|m| m["author"].as_str())
        .or_else(|| docling_meta.and_then(|m| m["authors"].as_str()))
        .map(String::from);

    Ok((summary, keywords, entities, author))
}

/// Parse the raw LLM response into structured metadata
pub fn parse_metadata_response(response: &str) -> (String, Vec<String>, serde_json::Value) {
    // Parse SLIM NER output
    // Example: {locations: ['Loc1'], organizations: [], person: ['Pers1'], products: []}
    
    let extract_list = |key: &str, content: &str| -> Vec<String> {
        // Try to find "key: [" with flexible spacing
        if let Some(key_start) = content.find(key) {
            let after_key = &content[key_start + key.len()..];
            if let Some(list_start) = after_key.find('[') {
                let between = &after_key[..list_start];
                if between.trim() == ":" {
                    if let Some(list_end) = after_key[list_start..].find(']') {
                        let list_str = &after_key[list_start + 1..list_start + list_end];
                        return list_str.split(',')
                            .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                }
            }
        }
        Vec::new()
    };

    let mut persons = extract_list("person", response);
    if persons.is_empty() { persons = extract_list("persons", response); }
    if persons.is_empty() { persons = extract_list("people", response); }
    
    let mut organizations = extract_list("organization", response);
    if organizations.is_empty() { organizations = extract_list("organizations", response); }

    let mut locations = extract_list("location", response);
    if locations.is_empty() { locations = extract_list("locations", response); }

    let mut products = extract_list("product", response);
    if products.is_empty() { products = extract_list("products", response); }
    if products.is_empty() { products = extract_list("software", response); }

    let mut concepts = extract_list("concept", response);
    if concepts.is_empty() { concepts = extract_list("concepts", response); }

    let mut keywords = extract_list("topic", response);
    if keywords.is_empty() { keywords = extract_list("topics", response); }

    let mut questions = extract_list("question", response);
    if questions.is_empty() { questions = extract_list("questions", response); }

    let mut dates = extract_list("date", response);
    if dates.is_empty() { dates = extract_list("dates", response); }

    let summaries = extract_list("summary", response);
    let mut summary = summaries.first().cloned().unwrap_or_default();

    if summary.is_empty() {
        summary = "Summary not available from metadata extraction process.".to_string();
    }

    // Fallback for keywords if empty
    if keywords.is_empty() {
         keywords = concepts.clone();
    }

    // Build structured entities JSON
    let entities = serde_json::json!({
        "persons": persons,
        "organizations": organizations,
        "locations": locations,
        "products": products,
        "concepts": concepts,
        "questions": questions,
        "dates": dates,
        "topics": keywords // Include topics in entities as well for completeness
    });

    (summary, keywords, entities)
}

/// Enrich a chunk with metadata context
#[allow(dead_code)]
pub fn enrich_chunk(
    title: &str,
    summary: &str,
    keywords: &[String],
    questions: &[String],
    chunk: &str,
) -> String {
    let keywords_str = keywords.join(", ");
    let questions_str = questions.iter()
        .map(|q| format!("- {}", q))
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "Title: {}\nSummary: {}\nKeywords: {}\nQuestions:\n{}\n---\n{}",
        title, summary, keywords_str, questions_str, chunk
    )
}

/// Split text into chunks using text-splitter
fn chunk_text(text: &str, target_tokens: usize) -> Vec<String> {
    let splitter = TextSplitter::default()
        .with_trim_chunks(true);
    
    splitter.chunks(text, target_tokens)
        .map(|s| s.to_string())
        .collect()
}

// ============================================
// Indexing Operations
// ============================================


/// Index a file or directory
pub async fn index_path(pool: &PgPool, embedder: &Embedder, path: &str) -> Result<()> {
    let path = Path::new(path);
    
    if path.is_dir() {
        for entry in walkdir::WalkDir::new(path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
        {
            if let Err(e) = index_file(pool, embedder, entry.path()).await {
                tracing::error!("Failed to index {:?}: {}", entry.path(), e);
            }
        }
    } else {
        index_file(pool, embedder, path).await?;
    }

    Ok(())
}

/// Index a single file
async fn index_file(pool: &PgPool, embedder: &Embedder, path: &Path) -> Result<()> {
    let extension = path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    // Skip hidden files or non-document types
    if extension.is_empty() || matches!(extension.as_str(), "ds_store" | "gitignore") {
        return Ok(());
    }

    tracing::info!("Processing file: {:?}", path);

    // 1. Convert to Markdown via Docling
    let docling = DoclingClient::new();
    let (content, docling_meta) = match extension.as_str() {
        "pdf" | "docx" | "pptx" | "html" => docling.convert_file(path).await?,
        "md" | "markdown" | "txt" => {
            let content = tokio::fs::read_to_string(path).await?;
            (content, serde_json::json!(null))
        },
        _ => {
            tracing::warn!("Unsupported file type: {}", extension);
            return Ok(());
        }
    };

    let title = path.file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Untitled".to_string());

    // 2. Extract Metadata via LLM (with DocLing metadata)
    tracing::info!("Extracting metadata for: {}", title);
    let docling_meta_ref = if docling_meta.is_null() { None } else { Some(&docling_meta) };
    let (summary, keywords, entities, author) = extract_metadata(&content, docling_meta_ref).await
        .unwrap_or_else(|e| {
            tracing::error!("Metadata extraction failed: {}", e);
            ("".to_string(), vec![], serde_json::json!({}), None)
        });

    // 3. Chunking
    let raw_chunks = chunk_text(&content, CHUNK_SIZE);

    // 4. Enrich Chunks with simplified metadata (title + keywords only)
    let enriched_chunks: Vec<String> = raw_chunks.iter().map(|chunk| {
        // Simple enrichment: just prepend title and keywords context
        let keyword_str = keywords.join(", ");
        format!("Document: {}\nKeywords: {}\n\n{}", title, keyword_str, chunk)
    }).collect();

    tracing::info!("Indexing: {} ({} chunks)", title, enriched_chunks.len());

    // 5. Generate Embeddings & Store

    // Document embedding (use first enriched chunk)
    let doc_embedding_text = &enriched_chunks[0];
    let doc_embedding = embedder.embed(doc_embedding_text).await?;

    // Insert document
    let doc_id = db::insert_document(
        pool,
        db::InsertDocumentParams {
            title: &title,
            content: &content,
            source_path: Some(&path.to_string_lossy()),
            source_type: &extension,
            embedding: &doc_embedding,
            summary: if summary.is_empty() { None } else { Some(&summary) },
            keywords: Some(keywords),
            entities: Some(entities.clone()),
            author: author.as_deref(),
        }
    ).await?;

    // Insert chunks
    for (idx, (raw_chunk, enriched_chunk)) in raw_chunks.iter().zip(enriched_chunks.iter()).enumerate() {
        let chunk_embedding = embedder.embed(enriched_chunk).await?;
        db::insert_chunk(
            pool,
            doc_id,
            idx as i32,
            raw_chunk, // Store raw content for display
            &chunk_embedding,
            None,
        ).await?;
    }

    tracing::info!("Indexed document: {} ({})", title, doc_id);
    Ok(())
}

/// Index content from a URL
pub async fn index_url(pool: &PgPool, embedder: &Embedder, url: &str) -> Result<()> {
    tracing::info!("Processing URL: {}", url);
    
    let docling = DoclingClient::new();
    let content = docling.convert_url(url).await?;
    
    let title = url.split('/').next_back()
        .map(|s| s.to_string())
        .unwrap_or_else(|| "Web Document".to_string());

    // Extract Metadata (no DocLing metadata for URLs)
    let (_summary, keywords, entities, author) = extract_metadata(&content, None).await
        .unwrap_or_else(|_| ("".to_string(), vec![], serde_json::json!({}), None));

    // Chunking
    let raw_chunks = chunk_text(&content, CHUNK_SIZE);

    // Enrich Chunks with simplified metadata (title + keywords only)
    let enriched_chunks: Vec<String> = raw_chunks.iter().map(|chunk| {
        let keyword_str = keywords.join(", ");
        format!("Document: {}\nKeywords: {}\n\n{}", title, keyword_str, chunk)
    }).collect();

    // Embed & Store
    let doc_embedding_text = &enriched_chunks[0];
    let doc_embedding = embedder.embed(doc_embedding_text).await?;

    let doc_id = db::insert_document(
        pool,
        db::InsertDocumentParams {
            title: &title,
            content: &content,
            source_path: Some(url),
            source_type: "url",
            embedding: &doc_embedding,
            summary: None,
            keywords: Some(keywords),
            entities: Some(entities),
            author: author.as_deref(),
        }
    ).await?;

    for (idx, (raw_chunk, enriched_chunk)) in raw_chunks.iter().zip(enriched_chunks.iter()).enumerate() {
        let chunk_embedding = embedder.embed(enriched_chunk).await?;
        db::insert_chunk(
            pool,
            doc_id,
            idx as i32,
            raw_chunk,
            &chunk_embedding,
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
    use super::*;

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
