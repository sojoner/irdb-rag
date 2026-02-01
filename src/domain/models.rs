use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
#[cfg(feature = "ssr")]
use sqlx::FromRow;
use uuid::Uuid;

#[cfg(feature = "ssr")]
use crate::config::{EmbeddingConfig, LLMProviderConfig};

// ============================================
// Core Domain Models
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct Document {
    pub id: Uuid,
    pub title: String,
    pub source_path: Option<String>,
    pub source_type: String,
    pub content: String,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub category_id: Option<Uuid>,
    pub keywords: Option<Vec<String>>,
    pub locations: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub status: String,
    #[cfg_attr(feature = "ssr", sqlx(json))]
    pub entities: Option<serde_json::Value>,
    #[cfg_attr(feature = "ssr", sqlx(json))]
    pub metadata: Option<serde_json::Value>,
    pub content_hash: Option<String>,
    pub embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct DocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub chunk_index: i32,
    pub content: String,
    pub page_number: Option<i32>,
    pub section_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct DocumentAsset {
    pub id: Uuid,
    pub document_id: Uuid,
    pub asset_type: String,
    pub page_number: Option<i32>,
    pub alt_text: Option<String>,
    pub caption: Option<String>,
    pub content_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct SearchResult {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub source_path: Option<String>,
    pub category_name: Option<String>,
    pub bm25_score: f64, // Normalized score (0-1)
    pub vector_score: f64,
    pub combined_score: f64, // Normalized combined score (0-1)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reranker_score: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,
    #[serde(default)]
    #[cfg_attr(feature = "ssr", sqlx(default))]
    pub raw_bm25_score: f64, // Raw RSV from BM25 (for debugging/display)
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ssr", sqlx(default))]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>, // For sorting by date
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchMetadata {
    pub duration_ms: u128,
    pub total_results: usize,
    pub unique_documents: usize,
    pub total_chunks_searched: usize,
    pub bm25_weight: f64,
    pub vector_weight: f64,
    /// The actual BM25 query that was executed (after field qualification)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_query: Option<String>,
    /// Fields that were searched
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_fields: Option<Vec<String>>,
    /// Whether the query was detected as field-qualified
    #[serde(default)]
    pub was_field_qualified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct DbStats {
    pub document_count: i64,
    pub chunk_count: i64,
    pub database_size: String,
}

/// Sort order for search results
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum SortOrder {
    #[default]
    Relevance, // BM25 score descending (default for searches with query)
    DateDesc,  // Newest first (default for browse/filter-only)
    DateAsc,   // Oldest first
    TitleAsc,  // Alphabetical A-Z
    TitleDesc, // Alphabetical Z-A
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LLMConfig {
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

impl LLMConfig {
    /// Create LLMConfig from a provider configuration
    #[cfg(feature = "ssr")]
    pub fn from_provider_config(config: &LLMProviderConfig) -> Self {
        Self {
            provider: config.provider.clone(),
            api_url: config.api_url.clone(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InfraEmbeddingConfig {
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub dimensions: u32,
}

impl InfraEmbeddingConfig {
    /// Create InfraEmbeddingConfig from a config file
    #[cfg(feature = "ssr")]
    pub fn from_config(config: &EmbeddingConfig) -> Self {
        Self {
            provider: config.provider.clone(),
            api_url: config.api_url.clone(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
            dimensions: config.dimensions,
        }
    }
}

// ============================================
// Import Job Models
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct ImportJob {
    pub id: Uuid,
    pub status: String,      // pending, running, completed, failed, cancelled
    pub source_type: String, // folder, url, file_upload
    pub source_path: Option<String>,
    pub total_items: i32,
    pub processed_items: i32,
    pub failed_items: i32,
    pub skipped_items: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct ImportItem {
    pub id: Uuid,
    pub job_id: Uuid,
    pub source_path: String,
    pub status: String, // pending, processing, completed, failed, skipped
    pub retry_count: i32,
    pub error_message: Option<String>,
    pub error_type: Option<String>, // transient, permanent
    pub document_id: Option<Uuid>,
    pub file_size_bytes: i64, // for prioritizing processing (smallest first)
    pub created_at: DateTime<Utc>,
    pub processed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ErrorType {
    Transient, // Retry with backoff
    Permanent, // Skip immediately
}

impl ErrorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorType::Transient => "transient",
            ErrorType::Permanent => "permanent",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "transient" => Some(ErrorType::Transient),
            "permanent" => Some(ErrorType::Permanent),
            _ => None,
        }
    }
}

impl std::str::FromStr for ErrorType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "transient" => Ok(ErrorType::Transient),
            "permanent" => Ok(ErrorType::Permanent),
            _ => Err(format!("Invalid error type: {}", s)),
        }
    }
}

impl ErrorType {
    /// Classify error based on error message
    /// Distinguishes between transient errors (should retry) and permanent errors (should skip)
    pub fn classify(error_message: &str) -> Self {
        let msg_lower = error_message.to_lowercase();

        // Transient errors - should retry with backoff
        if msg_lower.contains("timeout")
            || msg_lower.contains("503")
            || msg_lower.contains("connection refused")
            || msg_lower.contains("rate limit")
            || msg_lower.contains("temporarily unavailable")
            || msg_lower.contains("500 internal")
            || msg_lower.contains("502 bad gateway")
            || msg_lower.contains("504 gateway timeout")
            || msg_lower.contains("too many requests")
        {
            return ErrorType::Transient;
        }

        // Permanent errors - should skip immediately
        // - Unsupported file formats
        // - Corrupted files
        // - Encoding issues
        // - Scanned PDFs requiring OCR (when OCR disabled)
        if msg_lower.contains("unsupported")
            || msg_lower.contains("format not recognized")
            || msg_lower.contains("invalid")
            || msg_lower.contains("corrupted")
            || msg_lower.contains("damaged")
            || msg_lower.contains("file not found")
            || msg_lower.contains("permission denied")
            || msg_lower.contains("not a pdf")
            || msg_lower.contains("utf-8")
            || msg_lower.contains("encoding")
            || msg_lower.contains("character decode")
            || msg_lower.contains("scanned image")
            || msg_lower.contains("ocr required")
        {
            return ErrorType::Permanent;
        }

        // Default to permanent to avoid infinite retries on unknown errors
        ErrorType::Permanent
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportProgress {
    pub total: i32,
    pub processed: i32,
    pub completed: i32,
    pub failed: i32,
    pub skipped: i32,
    pub percent: f64,
}

impl ImportProgress {
    pub fn from_job(job: &ImportJob) -> Self {
        let total = job.total_items.max(1); // Avoid division by zero
        let percent = (job.processed_items as f64 / total as f64) * 100.0;
        Self {
            total: job.total_items,
            processed: job.processed_items,
            completed: job.processed_items - job.failed_items - job.skipped_items,
            failed: job.failed_items,
            skipped: job.skipped_items,
            percent,
        }
    }
}
