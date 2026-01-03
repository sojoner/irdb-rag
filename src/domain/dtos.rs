use serde::{Deserialize, Serialize};
use uuid::Uuid;


// We need to handle LLMConfig. For now, let's assume it will be in domain::models or we import it.
// Since we haven't moved LLMConfig yet, we might have a temporary issue.
// But we are defining the target state.
// Let's assume LLMConfig is moved to domain::models or domain::config.
// I will put LLMConfig in domain::models for now to avoid circular deps.

use crate::domain::models::LLMConfig; 

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
    pub authors: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub organizations: Option<Vec<String>>,
    pub persons: Option<Vec<String>>,
    pub products: Option<Vec<String>>,
    pub word_count_min: Option<i32>,
    pub word_count_max: Option<i32>,
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

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
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
    pub db_stats: crate::domain::models::DbStats,
    pub llm_config: LLMConfig,
    pub embedding_config: EmbeddingInfo, // Renamed from EmbeddingConfig to avoid conflict
    pub env_vars: EnvVars,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EmbeddingInfo {
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EntityStats {
    pub entity_type: String,
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct AggregationStats {
    pub categories: Vec<(String, i64)>,
    pub keywords: Vec<(String, i64)>,
    pub locations: Vec<(String, i64)>,
    pub persons: Vec<(String, i64)>,
    pub organizations: Vec<(String, i64)>,
    pub products: Vec<(String, i64)>,
    pub concepts: Vec<(String, i64)>,
    pub authors: Vec<(String, i64)>,
    pub word_count_ranges: Vec<(String, i64)>,
}
