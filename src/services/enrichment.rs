//! Document Enrichment Module
//!
//! Handles document content extraction and metadata enrichment using Docling and LLM

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

use crate::domain::models::LLMConfig;
use crate::infra::llm::{call_llm_with_options};
use crate::services::enrichment_utils::{
    parse_keywords_from_string, clean_json_response, extract_author_from_entities,
    merge_entities, generate_category_uuid, batch_text,
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

        let response = call_llm_with_options(&self.llm_config, system, &user, Some(150), Some(0.3))
            .await
            .context("Failed to generate summary")?;

        Ok(response.trim().to_string())
    }

    /// Extract keywords from content
    async fn extract_keywords(&self, summary: &str, content: &str) -> Result<Vec<String>> {
        let system = "You are a keyword extraction assistant. Extract the most important keywords and topics.";
        let user = format!(
            "Extract 5-8 important keywords or key phrases from this content. Return ONLY a comma-separated list.\n\nSummary: {}\n\nContent preview:\n{}",
            summary,
            &content[..content.len().min(1000)]
        );

        let response = call_llm_with_options(&self.llm_config, system, &user, Some(100), Some(0.2))
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
        let batches = batch_text(sentences, 2000, 3);

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

// Note: merge_entities and generate_category_uuid have been moved to
// services::enrichment_utils as pure functions and are imported above

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_extract_string_field_with_single_key() {
        let json = json!({ "title": "Test Title" });
        let mut result = None;
        extract_string_field(&json, &["title"], &mut result, "");
        assert_eq!(result, Some("Test Title".to_string()));
    }

    #[test]
    fn test_extract_string_field_with_prioritized_keys() {
        let json = json!({ "modified": "2024-01-01", "modification_date": "2024-01-02" });
        let mut result = None;
        extract_string_field(&json, &["modified", "modification_date"], &mut result, "");
        assert_eq!(result, Some("2024-01-01".to_string()));
    }

    #[test]
    fn test_extract_string_field_empty_values_skipped() {
        let json = json!({ "title": "", "author": "John Doe" });
        let mut result = None;
        extract_string_field(&json, &["title", "author"], &mut result, "");
        assert_eq!(result, Some("John Doe".to_string()));
    }

    #[test]
    fn test_extract_array_if_nonempty_success() {
        let json = json!({ "items": [1, 2, 3] });
        let result = extract_array_if_nonempty(&json, &["items"]);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_extract_array_if_nonempty_empty_array() {
        let json = json!({ "items": [] });
        let result = extract_array_if_nonempty(&json, &["items"]);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_array_if_nonempty_missing_key() {
        let json = json!({});
        let result = extract_array_if_nonempty(&json, &["items"]);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_document_origin() {
        let docling = json!({
            "document": {
                "metadata": {
                    "mimetype": "application/pdf",
                    "filename": "test.pdf",
                    "binary_hash": "abc123",
                    "uri": "file:///test.pdf"
                }
            }
        });
        let origin = extract_document_origin(&docling);
        assert!(origin.is_some());
        let o = origin.unwrap();
        assert_eq!(o.mimetype, Some("application/pdf".to_string()));
        assert_eq!(o.filename, Some("test.pdf".to_string()));
    }

    #[test]
    fn test_extract_document_structure() {
        let docling = json!({
            "document": {
                "texts": [
                    { "_object_type": "Text", "content": "Hello" },
                    { "_object_type": "SectionHeader", "content": "Introduction" },
                    { "_object_type": "Text", "content": "World" },
                ]
            }
        });
        let (types, sections) = extract_document_structure(&docling);
        assert!(types.contains(&"Text".to_string()));
        assert!(types.contains(&"SectionHeader".to_string()));
        assert_eq!(sections, vec!["Introduction".to_string()]);
    }

    #[test]
    fn test_count_array_items() {
        let json = json!({ "document": { "tables": [1, 2, 3, 4, 5] } });
        let count = count_array_items(&json, &["document", "tables"]);
        assert_eq!(count, 5);
    }

    #[test]
    fn test_count_array_items_missing_path() {
        let json = json!({});
        let count = count_array_items(&json, &["document", "tables"]);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_has_formulas_in_document_true() {
        let docling = json!({
            "document": {
                "md_content": "The equation is $$x^2 + y^2 = z^2$$"
            }
        });
        assert!(has_formulas_in_document(&docling));
    }

    #[test]
    fn test_has_formulas_in_document_with_brackets() {
        let docling = json!({
            "document": {
                "md_content": "Formula: \\[x = \\frac{-b}{2a}\\]"
            }
        });
        assert!(has_formulas_in_document(&docling));
    }

    #[test]
    fn test_has_formulas_in_document_false() {
        let docling = json!({
            "document": {
                "md_content": "No formulas here"
            }
        });
        assert!(!has_formulas_in_document(&docling));
    }

    #[test]
    fn test_calculate_extraction_quality_full() {
        let quality = calculate_extraction_quality(true, true, true);
        assert_eq!(quality.confidence_score, 0.9);
        assert_eq!(quality.completeness, 0.95);
        assert!(quality.layout_preserved);
    }

    #[test]
    fn test_calculate_extraction_quality_partial() {
        let quality = calculate_extraction_quality(true, false, false);
        assert_eq!(quality.confidence_score, 0.6);
        assert_eq!(quality.completeness, 0.7);
        assert!(!quality.layout_preserved);
    }

    #[test]
    fn test_split_into_sentences() {
        let text = "First sentence. Second sentence! Third sentence?";
        let sentences = split_into_sentences(text);
        assert!(!sentences.is_empty());
        assert!(sentences.iter().all(|s| !s.is_empty()));
    }
}
