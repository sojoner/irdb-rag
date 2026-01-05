use serde::{Deserialize, Serialize};
#[cfg(feature = "ssr")]
use sqlx::FromRow;
use uuid::Uuid;
use chrono::{DateTime, Utc};

// ============================================
// Core Domain Models
// ============================================

#[allow(dead_code)]
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
    pub word_count: Option<i32>,
    pub status: String,
    #[cfg_attr(feature = "ssr", sqlx(json))]
    pub entities: Option<serde_json::Value>,
    #[cfg_attr(feature = "ssr", sqlx(json))]
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(dead_code)]
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
    pub bm25_score: f64,
    pub vector_score: f64,
    pub combined_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ssr", derive(FromRow))]
pub struct DbStats {
    pub document_count: i64,
    pub chunk_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LLMConfig {
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

impl LLMConfig {
    pub fn from_env() -> Self {
        Self {
            provider: std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string()),
            api_url: std::env::var("LLM_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            api_key: std::env::var("LLM_API_KEY").unwrap_or_default(),
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4".to_string()),
        }
    }

    /// Create config for metadata extraction (uses faster, non-reasoning model)
    pub fn for_metadata() -> Self {
        Self {
            provider: std::env::var("METADATA_LLM_PROVIDER")
                .unwrap_or_else(|_| std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string())),
            api_url: std::env::var("METADATA_LLM_API_URL")
                .unwrap_or_else(|_| std::env::var("LLM_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string())),
            api_key: std::env::var("METADATA_LLM_API_KEY")
                .unwrap_or_else(|_| std::env::var("LLM_API_KEY").unwrap_or_default()),
            model: std::env::var("METADATA_LLM_MODEL")
                .unwrap_or_else(|_| "ibm/granite-4-h-tiny".to_string()),
        }
    }

    /// Create config for NER (Named Entity Recognition)
    pub fn for_ner() -> Self {
        Self {
            provider: std::env::var("NER_LLM_PROVIDER")
                .unwrap_or_else(|_| std::env::var("METADATA_LLM_PROVIDER")
                    .unwrap_or_else(|_| std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string()))),
            api_url: std::env::var("NER_LLM_API_URL")
                .unwrap_or_else(|_| std::env::var("METADATA_LLM_API_URL")
                    .unwrap_or_else(|_| std::env::var("LLM_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()))),
            api_key: std::env::var("NER_LLM_API_KEY")
                .unwrap_or_else(|_| std::env::var("METADATA_LLM_API_KEY")
                    .unwrap_or_else(|_| std::env::var("LLM_API_KEY").unwrap_or_default())),
            model: std::env::var("NER_LLM_MODEL")
                .unwrap_or_else(|_| "google/gemini-3-flash-preview".to_string()),
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
    pub fn from_env() -> Self {
        let dimensions = std::env::var("EMBEDDING_DIMENSIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .expect("EMBEDDING_DIMENSIONS environment variable must be set");

        Self {
            provider: std::env::var("EMBEDDING_PROVIDER").unwrap_or_else(|_| "openai".to_string()),
            api_url: std::env::var("EMBEDDING_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            api_key: std::env::var("EMBEDDING_API_KEY").unwrap_or_default(),
            model: std::env::var("EMBEDDING_MODEL").unwrap_or_else(|_| "text-embedding-3-small".to_string()),
            dimensions,
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
    pub status: String, // pending, running, completed, failed, cancelled
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
    pub fn classify(error_message: &str) -> Self {
        let msg_lower = error_message.to_lowercase();
        if msg_lower.contains("timeout")
            || msg_lower.contains("503")
            || msg_lower.contains("connection refused")
            || msg_lower.contains("rate limit")
        {
            ErrorType::Transient
        } else {
            ErrorType::Permanent
        }
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
