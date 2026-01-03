use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::llm::LLMConfig;

// ============================================
// Shared Data Models
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
#[allow(dead_code)]
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
    pub created_at: String, // Using String for simplicity in frontend
    pub word_count: Option<i32>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(dead_code)]
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
pub struct DbStats {
    pub document_count: i64,
    pub chunk_count: i64,
}

// ============================================
// Request/Response Types
// ============================================

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: i32,
    #[serde(default = "default_bm25_weight")]
    pub bm25_weight: f64,
    #[serde(default = "default_vector_weight")]
    pub vector_weight: f64,
    pub category_id: Option<Uuid>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub locations: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
}

fn default_limit() -> i32 { 10 }
fn default_bm25_weight() -> f64 { 0.5 }
fn default_vector_weight() -> f64 { 0.5 }

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChatRequest {
    pub message: String,
    pub conversation_id: Option<Uuid>,
    #[serde(default = "default_context_chunks")]
    pub context_chunks: i32,
    pub document_ids: Option<Vec<Uuid>>,
}

fn default_context_chunks() -> i32 { 5 }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChatResponse {
    pub message: String,
    pub conversation_id: Uuid,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SourceReference {
    pub document_id: Uuid,
    pub title: String,
    pub chunk: String,
    pub relevance: f64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct IndexRequest {
    pub path: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ListQuery {
    #[serde(default = "default_page_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_page_limit() -> i32 { 20 }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemStatus {
    pub db_stats: DbStats,
    pub llm_config: LLMConfig,
    pub embedding_config: EmbeddingConfig,
    pub env_vars: EnvVars,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmbeddingConfig {
    pub model: String,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EnvVars {
    pub database_url: String,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_api_url: String,
    pub embedding_api_url: String,
    pub docling_url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct UpdateModelRequest {
    pub provider: String,
    pub model: String,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
}
