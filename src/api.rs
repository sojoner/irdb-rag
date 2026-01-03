//! API handlers for RAG Chat
//! 
//! Provides REST endpoints for search, chat, document management,
//! and LLM integration with OpenAI/Anthropic/OpenRouter.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{db, indexer::Embedder, llm::{self, LLMConfig}};

// ============================================
// App State
// ============================================

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub embedder: Arc<Embedder>,
    pub llm_config: Arc<RwLock<LLMConfig>>,
    pub log_buffer: Arc<Mutex<Vec<String>>>,
}

impl AppState {
    pub fn new(pool: PgPool, embedder: Embedder, log_buffer: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            pool,
            embedder: Arc::new(embedder),
            llm_config: Arc::new(RwLock::new(LLMConfig::from_env())),
            log_buffer,
        }
    }
}

// ============================================
// Request/Response Types
// ============================================

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub message: String,
    pub conversation_id: Option<Uuid>,
    #[serde(default = "default_context_chunks")]
    pub context_chunks: i32,
    pub document_ids: Option<Vec<Uuid>>,
}

fn default_context_chunks() -> i32 { 5 }

#[derive(Debug, Serialize)]
pub struct ChatResponse {
    pub message: String,
    pub conversation_id: Uuid,
    pub sources: Vec<SourceReference>,
}

#[derive(Debug, Serialize)]
pub struct SourceReference {
    pub document_id: Uuid,
    pub title: String,
    pub chunk: String,
    pub relevance: f64,
}

#[derive(Debug, Deserialize)]
pub struct IndexRequest {
    pub path: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_page_limit")]
    pub limit: i32,
    #[serde(default)]
    pub offset: i32,
}

fn default_page_limit() -> i32 { 20 }

// ============================================
// API Handlers
// ============================================

/// Health check endpoint
pub async fn health_check() -> &'static str {
    "OK"
}

/// Hybrid search endpoint
pub async fn search(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Vec<db::SearchResult>>, AppError> {
    // Generate embedding for query
    let embedding = state.embedder.embed(&req.query)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let filters = db::SearchFilters {
        category_id: req.category_id,
        date_from: req.date_from.and_then(|d| d.parse().ok()),
        date_to: req.date_to.and_then(|d| d.parse().ok()),
        locations: req.locations,
        keywords: req.keywords,
        source_types: None,
    };

    let results = db::hybrid_search(
        &state.pool,
        &req.query,
        &embedding,
        &filters,
        req.limit,
        req.bm25_weight,
        req.vector_weight,
    ).await.map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(results))
}

/// Chat with RAG context
pub async fn chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    // Generate embedding for the query
    let embedding = state.embedder.embed(&req.message)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Retrieve relevant chunks
    let chunks = db::get_relevant_chunks(
        &state.pool, 
        &embedding, 
        req.context_chunks,
        req.document_ids.as_deref()
    )
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Build context
    let context: String = chunks.iter()
        .map(|c| format!("---\n{}\n", c.content))
        .collect();

    // Build prompt
    let system_prompt = r#"You are a helpful assistant answering questions based on the provided context from documents.
Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so.
Be concise and cite specific parts of the context when relevant."#;

    let user_prompt = format!(
        "CONTEXT:\n{}\n\nQUESTION:\n{}",
        context,
        req.message
    );

    // Call LLM
    let config = state.llm_config.read().await;
    let response = llm::call_llm(&config, system_prompt, &user_prompt)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Build sources
    let sources: Vec<SourceReference> = chunks.iter()
        .enumerate()
        .map(|(i, c)| SourceReference {
            document_id: c.document_id,
            title: c.section_title.clone().unwrap_or_else(|| format!("Chunk {}", i + 1)),
            chunk: c.content.chars().take(200).collect::<String>() + "...",
            relevance: 1.0 - (i as f64 * 0.1),
        })
        .collect();

    let conversation_id = req.conversation_id.unwrap_or_else(Uuid::new_v4);

    Ok(Json(ChatResponse {
        message: response,
        conversation_id,
        sources,
    }))
}

/// List documents
pub async fn list_documents(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<db::Document>>, AppError> {
    let docs = db::list_documents(&state.pool, query.limit, query.offset)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(docs))
}

/// Get single document
pub async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<db::Document>, AppError> {
    let doc = db::get_document(&state.pool, id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound)?;
    
    Ok(Json(doc))
}

/// Get document assets (images, formulas, etc.)
pub async fn get_document_assets(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<db::DocumentAsset>>, AppError> {
    let assets = db::get_document_assets(&state.pool, id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(assets))
}

/// Export document as markdown
pub async fn export_markdown(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<String, AppError> {
    let doc = db::get_document(&state.pool, id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .ok_or(AppError::NotFound)?;

    let markdown = format!(
        "# {}\n\n**Source:** {}\n\n---\n\n{}",
        doc.title,
        doc.source_path.unwrap_or_else(|| "Unknown".to_string()),
        doc.content
    );

    Ok(markdown)
}

/// List categories
pub async fn list_categories(
    State(state): State<AppState>,
) -> Result<Json<Vec<db::Category>>, AppError> {
    let categories = db::list_categories(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(categories))
}

/// Index a new document
pub async fn index_document(
    State(state): State<AppState>,
    Json(req): Json<IndexRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(path) = req.path {
        crate::indexer::index_path(&state.pool, &state.embedder, &path)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }
    
    if let Some(url) = req.url {
        crate::indexer::index_url(&state.pool, &state.embedder, &url)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    Ok(Json(serde_json::json!({ "status": "indexed" })))
}

// ============================================
// System Status & Config
// ============================================

#[derive(Debug, Serialize)]
pub struct SystemStatus {
    pub db_stats: db::DbStats,
    pub llm_config: LLMConfig,
    pub embedding_config: EmbeddingConfig,
    pub env_vars: EnvVars,
}

#[derive(Debug, Serialize)]
pub struct EmbeddingConfig {
    pub model: String,
    pub chunk_size: usize,
    pub chunk_overlap: usize,
}

#[derive(Debug, Serialize)]
pub struct EnvVars {
    pub database_url: String,
    pub llm_provider: String,
    pub llm_model: String,
    pub llm_api_url: String,
    pub embedding_api_url: String,
    pub docling_url: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateModelRequest {
    pub provider: String,
    pub model: String,
    pub api_url: Option<String>,
    pub api_key: Option<String>,
}

/// Get system status and configuration
pub async fn get_status(
    State(state): State<AppState>,
) -> Result<Json<SystemStatus>, AppError> {
    let db_stats = db::get_db_stats(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let config = state.llm_config.read().await.clone();

    Ok(Json(SystemStatus {
        db_stats,
        llm_config: config.clone(),
        embedding_config: EmbeddingConfig {
            model: state.embedder.get_model_name().to_string(),
            chunk_size: crate::indexer::CHUNK_SIZE,
            chunk_overlap: 0,
        },
        env_vars: EnvVars {
            database_url: "postgres://***@***".to_string(), // Masked
            llm_provider: config.provider,
            llm_model: config.model,
            llm_api_url: config.api_url,
            embedding_api_url: state.embedder.get_api_url().to_string(),
            docling_url: std::env::var("DOCLING_URL").unwrap_or_else(|_| "http://localhost:5001".to_string()),
        },
    }))
}

/// Update LLM model configuration
pub async fn update_model(
    State(state): State<AppState>,
    Json(req): Json<UpdateModelRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let mut config = state.llm_config.write().await;
    config.provider = req.provider;
    config.model = req.model;
    if let Some(url) = req.api_url {
        config.api_url = url;
    }
    if let Some(key) = req.api_key {
        config.api_key = key;
    }

    Ok(Json(serde_json::json!({ "status": "updated", "config": *config })))
}

/// Get recent logs
pub async fn get_logs(
    State(state): State<AppState>,
) -> Json<Vec<String>> {
    let logs = state.log_buffer.lock().unwrap().clone();
    Json(logs)
}

/// Serve the UI (fallback for non-API routes)
pub async fn serve_ui() -> Html<&'static str> {
    Html(include_str!("../static/index.html"))
}

// ============================================
// Error Handling
// ============================================

#[derive(Debug)]
pub enum AppError {
    NotFound,
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => {
                (StatusCode::NOT_FOUND, "Not found").into_response()
            }
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
            }
        }
    }
}
