//! Document Enrichment Module
//!
//! Handles document content extraction and metadata enrichment using Docling and LLM

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

use crate::llm::LLMConfig;

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
}

impl Default for Enricher {
    fn default() -> Self {
        Self::new()
    }
}

impl Enricher {
    /// Create a new Enricher instance
    pub fn new() -> Self {
        Self {
            llm_config: LLMConfig::for_metadata(),
            ner_config: LLMConfig::for_ner(),
        }
    }

    /// Enrich a file by extracting content and generating metadata
    pub async fn enrich_file(&self, path: &Path) -> Result<(String, DocumentMetadata)> {
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

        // Extract file system metadata (creation, modification times)
        self.enrich_with_file_metadata(&mut metadata, path).await?;

        // Classify document into Wikipedia category
        let category = self.classify_document_category(&content, &metadata).await?;
        if let Some(cat) = category {
            metadata.category_name = Some(cat.category.clone());
            metadata.category_id = Some(generate_category_uuid(&cat.category));
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
            metadata.category_id = Some(generate_category_uuid(&cat.category));
        }

        Ok((content, metadata))
    }

    /// Extract text content from a file using Docling
    /// Returns (content, full_docling_response)
    async fn extract_file_content(&self, path: &Path) -> Result<(String, Value)> {
        let docling_url = std::env::var("DOCLING_URL")
            .unwrap_or_else(|_| "http://localhost:5001".to_string());

        // Read file bytes
        let file_bytes = tokio::fs::read(path).await
            .context("Failed to read file")?;

        let file_name = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("document");

        // Create multipart form with options - use ALL Docling features
        let options = serde_json::json!({
            "do_ocr": true,
            "do_table_structure": true,
            "generate_picture_images": true,
            "generate_page_images": true,  // Enable page images too
            "images_scale": 2.0,
            "ocr_engine": "easyocr"
        });

        let form = reqwest::multipart::Form::new()
            .part(
                "files",
                reqwest::multipart::Part::bytes(file_bytes)
                    .file_name(file_name.to_string())
            )
            .text("options", options.to_string());

        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/v1/convert/file", docling_url))
            .multipart(form)
            .send()
            .await
            .context("Failed to connect to Docling service")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Docling service error ({}): {}", status, error_text);
        }

        let json: Value = response.json().await
            .context("Failed to parse Docling response")?;

        // Extract markdown content from Docling response
        let content = json["document"]["md_content"]
            .as_str()
            .context("No markdown content in Docling response")?
            .to_string();

        if content.trim().is_empty() {
            anyhow::bail!("Docling returned empty content for file: {:?}", path);
        }

        // Return both content and full response for metadata extraction
        Ok((content, json))
    }

    /// Extract text content from a URL
    async fn extract_url_content(&self, url: &str) -> Result<String> {
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
        // Extract document-level metadata
        if let Some(doc_metadata) = docling_response.get("document")
            .and_then(|d| d.get("metadata"))
        {
            metadata.docling_metadata = Some(doc_metadata.clone());

            // Extract title from Docling metadata if available
            if metadata.title.is_none() || metadata.title.as_deref() == Some(title_hint) {
                if let Some(title) = doc_metadata.get("title").and_then(|t| t.as_str()) {
                    if !title.is_empty() {
                        metadata.title = Some(title.to_string());
                    }
                }
            }

            // Extract author from Docling metadata
            if metadata.author.is_none() {
                if let Some(author) = doc_metadata.get("author").and_then(|a| a.as_str()) {
                    if !author.is_empty() {
                        metadata.author = Some(author.to_string());
                    }
                }
            }

            // Extract creation date
            if let Some(created) = doc_metadata.get("created")
                .or_else(|| doc_metadata.get("creation_date"))
                .or_else(|| doc_metadata.get("CreationDate"))
                .and_then(|d| d.as_str())
            {
                metadata.creation_date = Some(created.to_string());
            }

            // Extract modification date
            if let Some(modified) = doc_metadata.get("modified")
                .or_else(|| doc_metadata.get("modification_date"))
                .or_else(|| doc_metadata.get("ModDate"))
                .and_then(|d| d.as_str())
            {
                metadata.modification_date = Some(modified.to_string());
            }
        }

        // Extract page count
        if let Some(pages) = docling_response.get("document")
            .and_then(|d| d.get("pages"))
            .and_then(|p| p.as_array())
        {
            metadata.page_count = Some(pages.len() as i32);
        }

        // Extract tables
        if let Some(tables) = docling_response.get("tables")
            .and_then(|t| t.as_array())
        {
            if !tables.is_empty() {
                metadata.tables = Some(tables.clone());
            }
        }

        // Extract images
        if let Some(images) = docling_response.get("images")
            .or_else(|| docling_response.get("document").and_then(|d| d.get("images")))
            .and_then(|i| i.as_array())
        {
            if !images.is_empty() {
                metadata.images = Some(images.clone());
            }
        }
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
        let summary_text = if context.len() > 1500 {
            &context[..1500]
        } else {
            &context
        };
        let summary = self.generate_summary(summary_text, title_hint).await?;

        // 2. Extract keywords from summary + beginning
        let keywords = self.extract_keywords(&summary, &context).await?;

        // 3. Extract entities using SLIM NER (works best on shorter chunks)
        let entities = self.extract_entities(&context).await?;

        // 4. Extract author from entities if present
        let author = entities["persons"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|v| v.as_str())
            .map(String::from);

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
        use crate::llm::call_llm_with_options;

        let system = "You are a document summarization assistant. Create concise, informative summaries.";
        let user = format!(
            "Summarize this document in 2-3 sentences. Focus on the main topic and key points.\n\nTitle: {}\n\nContent:\n{}",
            title, content
        );

        let response = call_llm_with_options(&self.llm_config, system, &user, Some(150), Some(0.3))
            .await
            .context("Failed to generate summary")?;

        Ok(response.trim().to_string())
    }

    /// Extract keywords from content
    async fn extract_keywords(&self, summary: &str, content: &str) -> Result<Vec<String>> {
        use crate::llm::call_llm_with_options;

        let system = "You are a keyword extraction assistant. Extract the most important keywords and topics.";
        let user = format!(
            "Extract 5-8 important keywords or key phrases from this content. Return ONLY a comma-separated list.\n\nSummary: {}\n\nContent preview:\n{}",
            summary,
            &content[..content.len().min(1000)]
        );

        let response = call_llm_with_options(&self.llm_config, system, &user, Some(100), Some(0.2))
            .await
            .context("Failed to extract keywords")?;

        // Parse comma-separated keywords
        let keywords: Vec<String> = response
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s.len() < 50) // Reasonable keyword length
            .take(8) // Limit to 8 keywords
            .collect();

        Ok(keywords)
    }

    /// Extract enhanced metadata from Docling response (structure, origin, quality)
    fn extract_enhanced_docling_metadata(
        &self,
        metadata: &mut DocumentMetadata,
        docling_response: &Value,
    ) {
        // Extract document origin information
        if let Some(origin) = docling_response.get("document").and_then(|d| d.get("metadata")) {
            metadata.document_origin = Some(DocumentOrigin {
                mimetype: origin.get("mimetype").and_then(|v| v.as_str()).map(String::from),
                filename: origin.get("filename").and_then(|v| v.as_str()).map(String::from),
                binary_hash: origin.get("binary_hash").and_then(|v| v.as_str()).map(String::from),
                uri: origin.get("uri").and_then(|v| v.as_str()).map(String::from),
            });
        }

        // Extract document structure information
        let mut element_types = Vec::new();
        let mut table_count = 0;
        let mut figure_count = 0;
        let mut has_formulas = false;
        let mut sections = Vec::new();

        // Count tables
        if let Some(tables) = docling_response.get("tables").and_then(|v| v.as_array()) {
            table_count = tables.len() as i32;
        }

        // Count figures/pictures
        if let Some(pictures) = docling_response
            .get("pictures")
            .or_else(|| docling_response.get("document").and_then(|d| d.get("images")))
            .and_then(|v| v.as_array())
        {
            figure_count = pictures.len() as i32;
        }

        // Check for formulas in the document content
        if let Some(doc) = docling_response.get("document") {
            if let Some(content) = doc.get("md_content").and_then(|v| v.as_str()) {
                has_formulas = content.contains("$$") || content.contains("\\[");
            }
        }

        // Collect element types from the document structure
        if let Some(doc) = docling_response.get("document") {
            if let Some(texts) = doc.get("texts").and_then(|v| v.as_array()) {
                for item in texts {
                    if let Some(obj_type) = item.get("_object_type").and_then(|v| v.as_str()) {
                        if !element_types.contains(&obj_type.to_string()) {
                            element_types.push(obj_type.to_string());
                        }

                        // Collect section titles
                        if obj_type == "SectionHeader" {
                            if let Some(text) = item
                                .get("content")
                                .and_then(|v| v.as_str())
                            {
                                sections.push(text.to_string());
                            }
                        }
                    }
                }
            }
        }

        metadata.document_structure = Some(DocumentStructure {
            sections,
            has_tables: table_count > 0,
            has_figures: figure_count > 0,
            has_formulas,
            table_count,
            figure_count,
            element_types,
        });

        // Calculate extraction quality metrics
        let has_content = metadata.page_count.unwrap_or(0) > 0;
        let has_structure = metadata.document_structure.is_some();
        let has_metadata = metadata.document_origin.is_some();

        metadata.extraction_quality = Some(ExtractionQuality {
            confidence_score: if has_content && has_structure { 0.9 } else { 0.6 },
            completeness: if has_content && has_structure && has_metadata { 0.95 } else { 0.7 },
            layout_preserved: has_structure,
        });
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
        use crate::llm::call_llm_with_options;

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

        match call_llm_with_options(&self.llm_config, system, &user, Some(150), Some(0.3)).await {
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
        let cleaned = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        let parsed: CategoryClassification = serde_json::from_str(cleaned)
            .context("Failed to parse category classification JSON")?;

        Ok(parsed)
    }

    /// Extract named entities using JSON-based prompting with optimized batching
    async fn extract_entities(&self, content: &str) -> Result<Value> {
        use crate::llm::call_llm_with_options;

        let mut all_entities = json!({
            "persons": vec![] as Vec<String>,
            "organizations": vec![] as Vec<String>,
            "products": vec![] as Vec<String>,
            "locations": vec![] as Vec<String>,
            "concepts": vec![] as Vec<String>,
            "questions": vec![] as Vec<String>
        });

        // Split content into sentences
        let sentences = split_into_sentences(content);

        // Process sentences in optimized batches for efficiency
        // Group sentences into ~2000 char batches (was 500) to reduce LLM calls
        let mut batches = Vec::new();
        let mut batch = String::new();
        const TARGET_BATCH_SIZE: usize = 2000; // Increased from 500 for better batching
        const MAX_BATCHES: usize = 3; // Reduced from 15 to 3 batches max

        for sentence in sentences {
            // Skip very short sentences (likely fragments)
            if sentence.len() < 10 {
                continue;
            }

            // Add to batch
            if !batch.is_empty() {
                batch.push(' ');
            }
            batch.push_str(&sentence);

            // When batch reaches target size, save it and start new one
            if batch.len() >= TARGET_BATCH_SIZE {
                if batches.len() < MAX_BATCHES {
                    batches.push(batch.clone());
                    batch.clear();
                } else {
                    // If we've hit max batches, append remaining to last batch
                    if let Some(last) = batches.last_mut() {
                        last.push(' ');
                        last.push_str(&batch);
                    }
                    batch.clear();
                }
            }
        }

        // Add any remaining content to batches
        if !batch.is_empty() {
            if batches.len() < MAX_BATCHES {
                batches.push(batch);
            } else if let Some(last) = batches.last_mut() {
                last.push(' ');
                last.push_str(&batch);
            }
        }

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
        let futures: Vec<_> = batches
            .iter()
            .map(|batch_text| {
                let system = system.to_string();
                let user = entity_prompt_template(batch_text);
                let ner_config = self.ner_config.clone();

                async move {
                    call_llm_with_options(
                        &ner_config,
                        &system,
                        &user,
                        Some(500),  // Max tokens for entity extraction
                        Some(0.2),  // Temperature
                    )
                    .await
                }
            })
            .collect();

        // Execute all batch extractions in parallel
        let results = futures::future::join_all(futures).await;

        // Merge all batch results
        for llm_response in results.into_iter().flatten() {
            if let Ok(batch_entities) = self.parse_json_entities(&llm_response) {
                merge_entities(&mut all_entities, &batch_entities);
            }
        }

        Ok(all_entities)
    }

    /// Parse JSON response from entity extraction
    fn parse_json_entities(&self, response: &str) -> Result<Value> {
        // Clean response - remove markdown code blocks if present
        let cleaned = response
            .trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        // Try to parse as JSON
        let parsed: Value = serde_json::from_str(cleaned)
            .context("Failed to parse entity extraction JSON")?;

        // Ensure all expected fields exist with empty arrays as defaults
        let mut entities = json!({
            "persons": [],
            "organizations": [],
            "products": [],
            "locations": [],
            "concepts": [],
            "questions": []
        });

        if let Some(obj) = parsed.as_object() {
            if let Some(entities_obj) = entities.as_object_mut() {
                for (key, value) in obj {
                    if entities_obj.contains_key(key) {
                        if let Some(arr) = value.as_array() {
                            entities_obj.insert(key.clone(), json!(arr));
                        }
                    }
                }
            }
        }

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

/// Merge entities from source into target, deduplicating values
fn merge_entities(target: &mut Value, source: &Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (key, source_array) in source_obj {
            if let Some(source_items) = source_array.as_array() {
                let target_array = target_obj
                    .entry(key.clone())
                    .or_insert_with(|| json!([]));

                if let Some(target_items) = target_array.as_array_mut() {
                    for item in source_items {
                        if let Some(s) = item.as_str() {
                            // Only add if not already present (deduplicate)
                            if !target_items.iter().any(|v| v.as_str() == Some(s)) {
                                target_items.push(json!(s));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Generate a deterministic UUID for a category based on its name
/// This ensures the same category name always produces the same UUID
fn generate_category_uuid(category_name: &str) -> Uuid {
    // Use a simple deterministic approach: hash the category name
    // This ensures the same category name always produces the same UUID
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    category_name.hash(&mut hasher);
    let hash = hasher.finish();

    // Create a deterministic UUID from the hash
    // We'll use the hash as the source for UUID generation
    let bytes: [u8; 16] = [
        ((hash >> 56) & 0xFF) as u8,
        ((hash >> 48) & 0xFF) as u8,
        ((hash >> 40) & 0xFF) as u8,
        ((hash >> 32) & 0xFF) as u8,
        ((hash >> 24) & 0xFF) as u8,
        ((hash >> 16) & 0xFF) as u8,
        ((hash >> 8) & 0xFF) as u8,
        (hash & 0xFF) as u8,
        ((hash >> 56) & 0xFF) as u8,
        ((hash >> 48) & 0xFF) as u8,
        ((hash >> 40) & 0xFF) as u8,
        ((hash >> 32) & 0xFF) as u8,
        ((hash >> 24) & 0xFF) as u8,
        ((hash >> 16) & 0xFF) as u8,
        ((hash >> 8) & 0xFF) as u8,
        (hash & 0xFF) as u8,
    ];

    Uuid::from_bytes(bytes)
}
