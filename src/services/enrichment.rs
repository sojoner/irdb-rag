//! Document Enrichment Module
//!
//! Handles document content extraction and metadata enrichment using Docling and LLM

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;
use sha2::{Sha256, Digest};

use crate::domain::models::LLMConfig;
use crate::infra::llm::call_llm_with_timeout;
use crate::services::enrichment_utils::{
    parse_keywords_from_string, clean_json_response, extract_author_from_entities,
    merge_entities, batch_text,
};

/// Structured metadata response from LLM
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct MetadataResponse {
    pub summary: Option<String>,
    pub keywords: Vec<String>,
    pub entities: EntitiesResponse,
}

/// Structured entities response from LLM
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct EntitiesResponse {
    pub persons: Vec<String>,
    pub organizations: Vec<String>,
    pub products: Vec<String>,
    pub locations: Vec<String>,
    pub concepts: Vec<String>,
    pub questions: Vec<String>,
}

/// Wikipedia category classification response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryClassification {
    pub category: String,
    pub confidence: f32,
    pub reasoning: String,
}

/// Document origin information from Docling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentOrigin {
    pub mimetype: Option<String>,
    pub filename: Option<String>,
    pub binary_hash: Option<String>,
    pub uri: Option<String>,
}

/// Document structure extracted from Docling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentStructure {
    pub sections: Vec<String>,
    pub has_tables: bool,
    pub has_figures: bool,
    pub has_formulas: bool,
    pub table_count: i32,
    pub figure_count: i32,
    pub element_types: Vec<String>,
}

/// Extraction quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionQuality {
    pub confidence_score: f32,
    pub completeness: f32,
    pub layout_preserved: bool,
}

/// Document metadata extracted from content
#[derive(Debug, Clone)]
pub struct DocumentMetadata {
    pub title: Option<String>,
    pub summary: Option<String>,
    pub keywords: Vec<String>,
    pub entities: Value,
    pub author: Option<String>,
    pub category_id: Option<Uuid>,
    pub category_name: Option<String>,
    // Docling-specific metadata
    pub docling_metadata: Option<Value>,
    pub page_count: Option<i32>,
    pub tables: Option<Vec<Value>>,
    pub images: Option<Vec<Value>>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
    // Enhanced metadata from Docling
    pub document_origin: Option<DocumentOrigin>,
    pub document_structure: Option<DocumentStructure>,
    pub extraction_quality: Option<ExtractionQuality>,
}

impl Default for DocumentMetadata {
    fn default() -> Self {
        Self {
            title: None,
            summary: None,
            keywords: Vec::new(),
            entities: json!({}),
            author: None,
            category_id: None,
            category_name: None,
            docling_metadata: None,
            page_count: None,
            tables: None,
            images: None,
            creation_date: None,
            modification_date: None,
            document_origin: None,
            document_structure: None,
            extraction_quality: None,
        }
    }
}

/// Enricher handles document content extraction and metadata generation
pub struct Enricher {
    llm_config: LLMConfig,
    ner_config: LLMConfig,
    docling_url: String,
    llm_timeout_seconds: u64,
}

impl Default for Enricher {
    fn default() -> Self {
        Self::new()
    }
}

impl Enricher {
    /// Create a new Enricher instance
    pub fn new() -> Self {
        Self::with_config(None, None, None)
    }

    /// Create Enricher with custom settings
    pub fn with_config(
        docling_url: Option<String>,
        llm_timeout_seconds: Option<u64>,
        settings: Option<&crate::config::Settings>,
    ) -> Self {
        let docling_url = docling_url
            .or_else(|| settings.map(|s| s.docling.url.clone()))
            .unwrap_or_else(|| "http://localhost:5001".to_string());
        let llm_timeout_seconds = llm_timeout_seconds
            .or_else(|| settings.map(|s| s.docling.timeout_seconds))
            .unwrap_or(300);

        let (llm_config, ner_config) = if let Some(s) = settings {
            (
                LLMConfig::from_provider_config(&s.llm.metadata),
                LLMConfig::from_provider_config(&s.llm.ner),
            )
        } else {
            // Fallback defaults if settings not provided
            (
                LLMConfig {
                    provider: "openai".to_string(),
                    api_url: "https://api.openai.com/v1".to_string(),
                    api_key: String::new(),
                    model: "ibm/granite-4-h-tiny".to_string(),
                },
                LLMConfig {
                    provider: "openai".to_string(),
                    api_url: "https://api.openai.com/v1".to_string(),
                    api_key: String::new(),
                    model: "google/gemini-3-flash-preview".to_string(),
                },
            )
        };

        Self {
            llm_config,
            ner_config,
            docling_url,
            llm_timeout_seconds,
        }
    }

    /// Enrich a file by extracting content and generating metadata
    pub async fn enrich_file(&self, path: &Path) -> Result<(String, DocumentMetadata)> {
        // Read file for hashing (idempotent deduplication)
        let file_bytes = tokio::fs::read(path).await
            .context("Failed to read file")?;
        let file_hash = compute_sha256_hash(&file_bytes);

        // Extract text content from file and get full Docling response
        let (content, docling_response) = self.extract_file_content(path).await?;

        let title_hint = path.file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Unknown");

        // Extract metadata using LLM
        let mut metadata = self.extract_metadata(&content, title_hint).await?;

        // Enrich with Docling-specific metadata (basic extraction)
        self.enrich_with_docling_metadata(&mut metadata, &docling_response, title_hint);

        // Extract enhanced Docling metadata (document structure, origin, quality)
        self.extract_enhanced_docling_metadata(&mut metadata, &docling_response);

        // Attach computed binary hash to document origin for idempotent indexing
        if let Some(ref mut origin) = metadata.document_origin {
            origin.binary_hash = Some(file_hash);
        } else {
            metadata.document_origin = Some(DocumentOrigin {
                mimetype: None,
                filename: path.file_name().and_then(|n| n.to_str()).map(String::from),
                binary_hash: Some(file_hash),
                uri: None,
            });
        }

        // Extract file system metadata (creation, modification times)
        self.enrich_with_file_metadata(&mut metadata, path).await?;

        // Classify document into Wikipedia category
        let category = self.classify_document_category(&content, &metadata).await?;
        if let Some(cat) = category {
            metadata.category_name = Some(cat.category.clone());
            // Category ID will be resolved in the indexing service via database lookup
        }

        Ok((content, metadata))
    }

    /// Enrich a URL by extracting content and generating metadata
    pub async fn enrich_url(&self, url: &str) -> Result<(String, DocumentMetadata)> {
        // Fetch and extract content from URL
        let content = self.extract_url_content(url).await?;

        // Extract metadata using LLM
        let title = url.split('/').next_back().unwrap_or("Web Document");
        let mut metadata = self.extract_metadata(&content, title).await?;

        // Classify document into Wikipedia category
        let category = self.classify_document_category(&content, &metadata).await?;
        if let Some(cat) = category {
            metadata.category_name = Some(cat.category.clone());
            // Category ID will be resolved in the indexing service via database lookup
        }

        Ok((content, metadata))
    }

    /// Extract text content from a file using Docling (synchronous API for better performance)
    /// Returns (content, full_docling_response)
    pub async fn extract_file_content(&self, path: &Path) -> Result<(String, Value)> {
        let docling_url = &self.docling_url;
        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document");

        // 1. Pre-flight check for PDFs
        if path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()) == Some("pdf".to_string()) {
            match check_pdf_integrity(path) {
                Ok(true) => {
                    tracing::debug!("✅ PDF pre-flight check passed for {}", file_name);
                }
                Ok(false) => {
                    tracing::warn!("⚠️ PDF pre-flight check failed (missing MediaBox) for {}. Using fallback parser.", file_name);
                    return self.extract_file_content_fallback(path).await;
                }
                Err(e) => {
                    tracing::warn!("⚠️ PDF pre-flight check error for {}: {}. Using fallback parser.", file_name, e);
                    return self.extract_file_content_fallback(path).await;
                }
            }
        }

        // Read file bytes
        let file_bytes = tokio::fs::read(path).await
            .context("Failed to read file")?;

        // Docling doesn't support .txt, treat as .md
        // Also sanitize filename to avoid issues with special characters
        let file_name_to_send = sanitize_filename_for_docling(file_name);

        // Build client with extended timeout for sync API
        // VLM enrichment with picture descriptions can take 120+ seconds per large PDF
        // Use full configured timeout to allow Docling to complete complex documents
        let request_timeout_secs = self.llm_timeout_seconds;
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(request_timeout_secs))
            .build()
            .context("Failed to build HTTP client")?;

        // Configure Ollama VLM API for picture description
        // Docling needs this per-request to know where Ollama is
        // Using rag-ollama handle for consistent container networking
        let vlm_url = "http://rag-ollama:11434/v1/chat/completions";
        let picture_description_api = serde_json::json!({
            "url": vlm_url,
            "headers": {},
            "params": {"model": "granite3.2-vision:2b"},
            "timeout": 60.0,  // Reduced from 120s
            "concurrency": 4, // Reduced from 8 to prevent Docling container overload
            "prompt": "Describe this image in a few sentences."
        }).to_string();

        tracing::info!("🚀 Sending request to Docling at {} for file: {} (timeout: {}s)", docling_url, file_name, request_timeout_secs);
        tracing::debug!("VLM API configuration: {}", picture_description_api);

        // Create multipart form with VLM options
        // Note: wait_for_completion=true forces synchronous behavior (blocks until result ready)
        // VLM enrichment can take 2-5+ minutes on large PDFs with many images
        // Use the full configured timeout to allow complete processing
        let docling_doc_timeout = self.llm_timeout_seconds;

        let form = reqwest::multipart::Form::new()
            .part(
                "files",
                reqwest::multipart::Part::bytes(file_bytes)
                    .file_name(file_name_to_send)
            )
            .text("do_picture_description", "true")
            .text("do_picture_classification", "true")
            .text("picture_description_api", picture_description_api)
            .text("document_timeout", docling_doc_timeout.to_string())
            .text("wait_for_completion", "true");  // Force sync mode - blocks until done

        // Use synchronous endpoint - much faster than async polling
        // This will block until Docling completes processing (up to request_timeout_secs)
        let response_result = client
            .post(format!("{}/v1/convert/file", docling_url))
            .multipart(form)
            .send()
            .await;

        let response = match response_result {
            Ok(resp) => resp,
            Err(e) => {
                tracing::error!("❌ Failed to connect to Docling service: {}. Using fallback parser.", e);
                return self.extract_file_content_fallback(path).await;
            }
        };

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("❌ Docling service error ({}): {}. Using fallback parser.", status, error_text);
            return self.extract_file_content_fallback(path).await;
        }

        tracing::info!("✅ Docling response received successfully for {}", file_name);

        let json: Value = match response.json().await {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("❌ Failed to parse Docling response: {}. Using fallback parser.", e);
                return self.extract_file_content_fallback(path).await;
            }
        };

        // Extract markdown content from Docling response
        let content = json["document"]["md_content"]
            .as_str()
            .map(|s| s.to_string());

        match content {
            Some(c) if !c.trim().is_empty() => Ok((c, json)),
            _ => {
                tracing::warn!("⚠️ Docling returned empty content for {}. Using fallback parser.", file_name);
                self.extract_file_content_fallback(path).await
            }
        }
    }

    /// Fallback content extraction using lopdf for PDFs or basic text reading
    async fn extract_file_content_fallback(&self, path: &Path) -> Result<(String, Value)> {
        let extension = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());
        
        let content = if extension == Some("pdf".to_string()) {
            tracing::info!("🔄 Using lopdf fallback for {:?}", path);
            extract_text_from_pdf(path)?
        } else {
            tracing::info!("🔄 Using basic text fallback for {:?}", path);
            tokio::fs::read_to_string(path).await
                .context("Failed to read file as text")?
        };

        if content.trim().is_empty() {
            anyhow::bail!("Fallback extraction returned empty content for file: {:?}", path);
        }

        // Create a minimal Docling-like response structure so downstream code doesn't break
        let mock_response = json!({
            "document": {
                "md_content": content,
                "metadata": {
                    "filename": path.file_name().and_then(|n| n.to_str()),
                    "mimetype": if extension == Some("pdf".to_string()) { "application/pdf" } else { "text/plain" }
                }
            }
        });

        Ok((content, mock_response))
    }

    /// Extract text content from a URL
    pub async fn extract_url_content(&self, url: &str) -> Result<String> {
        let client = reqwest::Client::new();
        let response = client.get(url).send().await
            .context("Failed to fetch URL")?;

        if !response.status().is_success() {
            anyhow::bail!("Failed to fetch URL: {}", response.status());
        }

        let html = response.text().await?;

        // Simple HTML to text conversion (in production, use html2text or similar)
        let text = html2text::from_read(html.as_bytes(), 80);

        Ok(text)
    }

    /// Enrich metadata with Docling-specific fields
    fn enrich_with_docling_metadata(
        &self,
        metadata: &mut DocumentMetadata,
        docling_response: &Value,
        title_hint: &str,
    ) {
        if let Some(doc_metadata) = docling_response.get("document")
            .and_then(|d| d.get("metadata"))
        {
            metadata.docling_metadata = Some(doc_metadata.clone());

            // Extract and apply metadata fields using pure functions
            extract_string_field(doc_metadata, &["title"], &mut metadata.title, title_hint);
            extract_string_field(doc_metadata, &["author"], &mut metadata.author, "");
            extract_string_field(doc_metadata, &["created", "creation_date", "CreationDate"],
                               &mut metadata.creation_date, "");
            extract_string_field(doc_metadata, &["modified", "modification_date", "ModDate"],
                               &mut metadata.modification_date, "");
        }

        // Extract page count
        metadata.page_count = docling_response.get("document")
            .and_then(|d| d.get("pages"))
            .and_then(|p| p.as_array())
            .map(|pages| pages.len() as i32);

        // Extract tables and images
        metadata.tables = extract_array_if_nonempty(docling_response, &["tables"]);
        metadata.images = extract_array_if_nonempty(docling_response, &["images"])
            .or_else(|| extract_array_if_nonempty(docling_response, &["document", "images"]));
    }

    /// Extract metadata from content using LLM with SLIM NER format
    pub async fn extract_metadata(&self, content: &str, title_hint: &str) -> Result<DocumentMetadata> {
        // Split content into sentences for better processing
        let sentences = split_into_sentences(content);

        // Use first ~3000 chars (approx 750 tokens) for richer context
        let context = sentences.iter()
            .scan(0, |len, sentence| {
                *len += sentence.len();
                if *len <= 3000 {
                    Some(sentence.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        // 1. Generate summary (using first 1500 chars for speed)
        let summary_text = if context.chars().count() > 1500 {
            context.chars().take(1500).collect::<String>()
        } else {
            context.clone()
        };

        // Parallelize summary generation and entity extraction
        // extract_entities is usually the slowest, so running it alongside summary generation helps
        let (summary_result, entities_result) = tokio::join!(
            self.generate_summary(&summary_text, title_hint),
            self.extract_entities(&context)
        );

        let summary = summary_result?;
        let entities = entities_result?;

        // 2. Extract keywords from summary + beginning
        let keywords = self.extract_keywords(&summary, &context).await?;

        // 4. Extract author from entities if present using pure function
        let author = extract_author_from_entities(&entities);

        Ok(DocumentMetadata {
            title: Some(title_hint.to_string()),
            summary: Some(summary),
            keywords,
            entities,
            author,
            category_id: None,
            category_name: None,
            docling_metadata: None,
            page_count: None,
            tables: None,
            images: None,
            creation_date: None,
            modification_date: None,
            document_origin: None,
            document_structure: None,
            extraction_quality: None,
        })
    }

    /// Generate a concise summary of the content
    async fn generate_summary(&self, content: &str, title: &str) -> Result<String> {
        let system = "You are a document summarization assistant. Create concise, informative summaries.";
        let user = format!(
            "Summarize this document in 2-3 sentences. Focus on the main topic and key points.\n\nTitle: {}\n\nContent:\n{}",
            title, content
        );

        let response = call_llm_with_timeout(&self.llm_config, system, &user, Some(150), Some(0.3), self.llm_timeout_seconds)
            .await
            .map_err(|e| {
                eprintln!("❌ LLM Error in generate_summary: {:?}", e);
                e
            })
            .context("Failed to generate summary")?;

        Ok(response.trim().to_string())
    }

    /// Extract keywords from content
    async fn extract_keywords(&self, summary: &str, content: &str) -> Result<Vec<String>> {
        let system = "You are a keyword extraction assistant. Extract the most important keywords and topics.";
        let content_preview = content.chars().take(1000).collect::<String>();
        let user = format!(
            "Extract 5-8 important keywords or key phrases from this content. Return ONLY a comma-separated list.\n\nSummary: {}\n\nContent preview:\n{}",
            summary,
            content_preview
        );

        let response = call_llm_with_timeout(&self.llm_config, system, &user, Some(100), Some(0.2), self.llm_timeout_seconds)
            .await
            .context("Failed to extract keywords")?;

        // Parse comma-separated keywords using pure function
        let keywords = parse_keywords_from_string(&response);

        Ok(keywords)
    }

    /// Extract enhanced metadata from Docling response (structure, origin, quality)
    fn extract_enhanced_docling_metadata(
        &self,
        metadata: &mut DocumentMetadata,
        docling_response: &Value,
    ) {
        // Extract document origin using pure function
        metadata.document_origin = extract_document_origin(docling_response);

        // Extract structure using functional composition
        let (element_types, sections) = extract_document_structure(docling_response);
        let table_count = count_array_items(docling_response, &["tables"]) as i32;
        let figure_count = count_array_items(docling_response, &["pictures"])
            .max(count_array_items(docling_response, &["document", "images"])) as i32;
        let has_formulas = has_formulas_in_document(docling_response);

        metadata.document_structure = Some(DocumentStructure {
            sections,
            has_tables: table_count > 0,
            has_figures: figure_count > 0,
            has_formulas,
            table_count,
            figure_count,
            element_types,
        });

        // Calculate extraction quality using pure function
        metadata.extraction_quality = Some(calculate_extraction_quality(
            metadata.page_count.unwrap_or(0) > 0,
            metadata.document_structure.is_some(),
            metadata.document_origin.is_some(),
        ));
    }

    /// Extract file system metadata (creation and modification times)
    async fn enrich_with_file_metadata(
        &self,
        metadata: &mut DocumentMetadata,
        path: &Path,
    ) -> Result<()> {
        match tokio::fs::metadata(path).await {
            Ok(file_metadata) => {
                // Get modification time (most documents we care about have this)
                if let Ok(modified) = file_metadata.modified() {
                    if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
                        let datetime = chrono::DateTime::<chrono::Utc>::from(
                            std::time::SystemTime::UNIX_EPOCH + duration
                        );
                        metadata.modification_date = Some(datetime.to_rfc3339());
                    }
                }

                // Creation time is OS-dependent, but try to extract if available
                #[cfg(windows)]
                {
                    if let Ok(created) = file_metadata.created() {
                        if let Ok(duration) = created.duration_since(std::time::UNIX_EPOCH) {
                            let datetime = chrono::DateTime::<chrono::Utc>::from(
                                std::time::SystemTime::UNIX_EPOCH + duration
                            );
                            metadata.creation_date = Some(datetime.to_rfc3339());
                        }
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Could not read file metadata for {:?}: {}", path, e);
            }
        }

        Ok(())
    }

    /// Classify document into a Wikipedia category using LLM
    async fn classify_document_category(
        &self,
        _content: &str,
        metadata: &DocumentMetadata,
    ) -> Result<Option<CategoryClassification>> {
        let summary = metadata.summary.as_deref().unwrap_or("");
        let concepts = metadata
            .entities
            .get("concepts")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();

        let system = "You are an expert document classifier. Classify documents into Wikipedia categories based on their content.";
        let user = format!(
            r#"Classify this document into a single Wikipedia category. Be specific and use standard Wikipedia categories.

Title: {}
Summary: {}
Key Concepts: {}

Respond ONLY with a JSON object in this exact format (no markdown, no explanations):
{{
  "category": "Wikipedia Category Name",
  "confidence": 0.95,
  "reasoning": "Brief explanation of why this category fits"
}}"#,
            metadata.title.as_deref().unwrap_or("Unknown"),
            summary,
            concepts
        );

        match call_llm_with_timeout(&self.llm_config, system, &user, Some(150), Some(0.3), self.llm_timeout_seconds).await {
            Ok(response) => {
                match self.parse_category_classification(&response) {
                    Ok(category) => Ok(Some(category)),
                    Err(e) => {
                        tracing::warn!("Failed to parse category classification: {}", e);
                        Ok(None)
                    }
                }
            }
            Err(e) => {
                tracing::warn!("Failed to classify document category: {}", e);
                Ok(None)
            }
        }
    }

    /// Parse category classification response from LLM
    fn parse_category_classification(&self, response: &str) -> Result<CategoryClassification> {
        let cleaned = clean_json_response(response);

        let parsed: CategoryClassification = serde_json::from_str(&cleaned)
            .context("Failed to parse category classification JSON")?;

        Ok(parsed)
    }

    /// Extract named entities using JSON-based prompting with optimized batching
    async fn extract_entities(&self, content: &str) -> Result<Value> {
        let mut all_entities = json!({
            "persons": vec![] as Vec<String>,
            "organizations": vec![] as Vec<String>,
            "products": vec![] as Vec<String>,
            "locations": vec![] as Vec<String>,
            "concepts": vec![] as Vec<String>,
            "questions": vec![] as Vec<String>
        });

        // Split content into sentences and batch using pure function
        let sentences = split_into_sentences(content);
        // Get batch count from settings, fallback to 16
        let batch_count = match crate::config::Settings::new() {
            Ok(settings) => settings.import.entity_extraction_batches,
            Err(_) => 16,
        };
        let batches = batch_text(sentences, 2000, batch_count);

        // Process all batches in parallel
        let system = "You are an expert named entity extraction system. You must extract ALL entities from the text and return them in valid JSON format.";
        let entity_prompt_template = |text: &str| -> String {
            format!(
                r#"Extract ALL named entities from the text below. Be thorough and capture every person, organization, location, product, concept, and question mentioned.

Return ONLY a JSON object (no markdown, no code blocks, no explanations). Use this exact format:

{{
  "persons": ["Full Name 1", "Full Name 2"],
  "organizations": ["Company 1", "Organization 2"],
  "locations": ["City 1", "Country 2", "Place 3"],
  "products": ["Product 1", "Product 2"],
  "concepts": ["Concept 1", "Topic 2"],
  "questions": ["Question 1?", "Question 2?"]
}}

Rules:
- Extract FULL names for persons (e.g., "Steve Jobs", not just "Jobs")
- Include ALL organizations mentioned
- Include cities, countries, states, and any geographic locations
- Include product names, brand names
- Extract key concepts and topics discussed
- Extract any questions posed in the text

Text: {}"#,
                text
            )
        };

        // Create futures for all batches
        let timeout = self.llm_timeout_seconds;
        let futures: Vec<_> = batches
            .iter()
            .map(|batch_text| {
                let system = system.to_string();
                let user = entity_prompt_template(batch_text);
                let ner_config = self.ner_config.clone();

                async move {
                    call_llm_with_timeout(
                        &ner_config,
                        &system,
                        &user,
                        Some(500),  // Max tokens for entity extraction
                        Some(0.2),  // Temperature
                        timeout,
                    )
                    .await
                }
            })
            .collect();

        // Execute all batch extractions in parallel with functional composition
        futures::future::join_all(futures)
            .await
            .into_iter()
            .filter_map(|result| result.ok())
            .for_each(|llm_response| {
                if let Ok(batch_entities) = self.parse_json_entities(&llm_response) {
                    merge_entities(&mut all_entities, &batch_entities);
                }
            });

        Ok(all_entities)
    }

    /// Parse JSON response from entity extraction
    fn parse_json_entities(&self, response: &str) -> Result<Value> {
        // Clean response using pure function
        let cleaned = clean_json_response(response);

        // Try to parse as JSON
        let parsed: Value = serde_json::from_str(&cleaned)
            .context("Failed to parse entity extraction JSON")?;

        // Ensure all expected fields exist with empty arrays using pure function
        use crate::services::enrichment_utils::ensure_entity_fields;
        let entities = ensure_entity_fields(&parsed);

        Ok(entities)
    }
}

/// Enrich a chunk with document metadata context
pub fn enrich_chunk(
    title: &str,
    summary: &str,
    keywords: &[String],
    questions: &[String],
    chunk: &str,
) -> String {
    let mut enriched = String::new();

    enriched.push_str(&format!("Title: {}\n", title));
    enriched.push_str(&format!("Summary: {}\n", summary));
    enriched.push_str(&format!("Keywords: {}\n", keywords.join(", ")));
    enriched.push_str("Questions:\n");

    if questions.is_empty() {
        enriched.push('\n');
    } else {
        for question in questions {
            enriched.push_str(&format!("- {}\n", question));
        }
    }

    enriched.push_str("---\n");
    enriched.push_str(chunk);

    enriched
}

/// Split text into sentences using Unicode segmentation rules
fn split_into_sentences(text: &str) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;

    text.split_sentence_bounds()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

// Pure helper functions for metadata extraction (functional programming patterns)

/// Extract a string field from JSON using a prioritized list of keys
fn extract_string_field(
    source: &Value,
    keys: &[&str],
    target: &mut Option<String>,
    hint: &str,
) {
    if target.is_none() || target.as_deref() == Some(hint) {
        for key in keys {
            if let Some(value) = source.get(key).and_then(|v| v.as_str()) {
                if !value.is_empty() {
                    *target = Some(value.to_string());
                    return;
                }
            }
        }
    }
}

/// Extract a non-empty array from JSON at the given key path
fn extract_array_if_nonempty(json: &Value, path: &[&str]) -> Option<Vec<Value>> {
    let mut current = json;
    for key in path {
        current = current.get(key)?;
    }
    current.as_array().and_then(|arr| {
        if arr.is_empty() {
            None
        } else {
            Some(arr.clone())
        }
    })
}

/// Extract document origin information from Docling response
fn extract_document_origin(docling_response: &Value) -> Option<DocumentOrigin> {
    docling_response
        .get("document")
        .and_then(|d| d.get("metadata"))
        .map(|origin| DocumentOrigin {
            mimetype: origin.get("mimetype").and_then(|v| v.as_str()).map(String::from),
            filename: origin.get("filename").and_then(|v| v.as_str()).map(String::from),
            binary_hash: origin.get("binary_hash").and_then(|v| v.as_str()).map(String::from),
            uri: origin.get("uri").and_then(|v| v.as_str()).map(String::from),
        })
}

/// Extract document structure (element types and sections) using functional composition
fn extract_document_structure(docling_response: &Value) -> (Vec<String>, Vec<String>) {
    let texts = docling_response
        .get("document")
        .and_then(|doc| doc.get("texts"))
        .and_then(|v| v.as_array());

    match texts {
        Some(texts) => {
            let element_types: Vec<String> = texts
                .iter()
                .filter_map(|item| item.get("_object_type").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect::<std::collections::HashSet<_>>()
                .into_iter()
                .collect();

            let sections = texts
                .iter()
                .filter(|item| {
                    item.get("_object_type")
                        .and_then(|v| v.as_str())
                        .is_some_and(|t| t == "SectionHeader")
                })
                .filter_map(|item| item.get("content").and_then(|v| v.as_str()))
                .map(|s| s.to_string())
                .collect();

            (element_types, sections)
        }
        None => (Vec::new(), Vec::new()),
    }
}

/// Count array items at the given key path
fn count_array_items(json: &Value, path: &[&str]) -> usize {
    let mut current = json;
    for key in path {
        match current.get(key) {
            Some(val) => current = val,
            None => return 0,
        }
    }
    current.as_array().map(|arr| arr.len()).unwrap_or(0)
}

/// Check if document contains formulas
fn has_formulas_in_document(docling_response: &Value) -> bool {
    docling_response
        .get("document")
        .and_then(|doc| doc.get("md_content").and_then(|v| v.as_str()))
        .map(|content| content.contains("$$") || content.contains("\\["))
        .unwrap_or(false)
}

/// Calculate extraction quality metrics using pure function
fn calculate_extraction_quality(has_content: bool, has_structure: bool, has_metadata: bool) -> ExtractionQuality {
    ExtractionQuality {
        confidence_score: if has_content && has_structure { 0.9 } else { 0.6 },
        completeness: if has_content && has_structure && has_metadata { 0.95 } else { 0.7 },
        layout_preserved: has_structure,
    }
}

/// Sanitize filename for Docling to handle special characters
/// Replaces problematic Unicode characters and ensures .txt -> .md conversion
fn sanitize_filename_for_docling(filename: &str) -> String {
    let mut sanitized = filename.to_string();

    // Convert .txt to .md (Docling doesn't support .txt)
    if sanitized.ends_with(".txt") {
        sanitized = format!("{}.md", sanitized.trim_end_matches(".txt"));
    }

    // Replace problematic Unicode characters that cause Docling failures
    // Em-dash variants
    sanitized = sanitized.replace('\u{2014}', "-");  // U+2014 em-dash
    sanitized = sanitized.replace('\u{2013}', "-");  // U+2013 en-dash
    sanitized = sanitized.replace('\u{2015}', "-");  // U+2015 horizontal bar

    // Quotes
    sanitized = sanitized.replace('\u{201C}', "\""); // U+201C left double quote
    sanitized = sanitized.replace('\u{201D}', "\""); // U+201D right double quote
    sanitized = sanitized.replace('\u{2018}', "'");  // U+2018 left single quote
    sanitized = sanitized.replace('\u{2019}', "'");  // U+2019 right single quote

    // Other problematic characters
    sanitized = sanitized.replace('|', "_");  // Pipe character
    sanitized = sanitized.replace(':', "_");  // Colon (problematic on some systems)
    sanitized = sanitized.replace('?', "");   // Question mark
    sanitized = sanitized.replace('*', "");   // Asterisk
    sanitized = sanitized.replace('<', "");   // Less than
    sanitized = sanitized.replace('>', "");   // Greater than

    // Additional problematic characters from various encodings
    sanitized = sanitized.replace('\u{00A0}', " ");  // Non-breaking space
    sanitized = sanitized.replace('\u{202F}', " ");  // Narrow no-break space

    // Aggressive ASCII conversion for maximum compatibility
    // Also replace spaces with underscores for better compatibility
    sanitized = sanitized.chars()
        .map(|c| if c.is_ascii() { 
            if c == ' ' { '_' } else { c }
        } else { 
            '_' 
        })
        .collect();

    // Replace leading/trailing spaces and dots
    sanitized = sanitized.trim().to_string();
    while sanitized.starts_with('.') || sanitized.starts_with('-') || sanitized.starts_with('_') {
        sanitized = sanitized.chars().skip(1).collect();
    }
    while sanitized.ends_with('.') || sanitized.ends_with('_') {
        sanitized = sanitized.chars().take(sanitized.chars().count() - 1).collect();
    }

    // Ensure filename is not empty after sanitization
    if sanitized.is_empty() {
        sanitized = "document.pdf".to_string();
    }

    sanitized
}

/// Check PDF integrity using lopdf
/// Returns true if PDF has valid MediaBox/CropBox for all pages
fn check_pdf_integrity(path: &Path) -> Result<bool> {
    #[cfg(feature = "ssr")]
    {
        let doc = lopdf::Document::load(path)
            .context("Failed to load PDF with lopdf")?;
        
        for page_id in doc.get_pages().values() {
            let page = doc.get_object(*page_id)
                .and_then(|obj| obj.as_dict())
                .context("Failed to get page dictionary")?;
            
            let has_dimensions = page.has(b"MediaBox") || page.has(b"CropBox");
            if !has_dimensions {
                return Ok(false);
            }
        }
        Ok(true)
    }
    #[cfg(not(feature = "ssr"))]
    {
        Ok(true)
    }
}

/// Extract text from PDF using lopdf
fn extract_text_from_pdf(path: &Path) -> Result<String> {
    #[cfg(feature = "ssr")]
    {
        let doc = lopdf::Document::load(path)
            .context("Failed to load PDF with lopdf")?;
        
        let mut content = String::new();
        let pages = doc.get_pages();
        
        for page_num in 1..=pages.len() {
            if let Ok(text) = doc.extract_text(&[page_num as u32]) {
                content.push_str(&text);
                content.push('\n');
            }
        }
        
        Ok(content)
    }
    #[cfg(not(feature = "ssr"))]
    {
        anyhow::bail!("PDF extraction not supported in this environment")
    }
}

/// Compute SHA256 hash of file content for idempotent indexing
pub fn compute_sha256_hash(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
