//! API handlers for RAG Chat

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response, sse::Event},
    response::sse::KeepAlive,
    Json,
};
use futures::stream::StreamExt;
use uuid::Uuid;

use crate::api::state::AppState;
use crate::domain::dtos::*;

use crate::infra::{db, llm};
use crate::services::indexing;

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
) -> Result<Json<Vec<crate::domain::models::SearchResult>>, AppError> {
    tracing::info!("Received search request: query='{}', limit={}, bm25_weight={}, vector_weight={}",
        req.query, req.limit, req.bm25_weight, req.vector_weight);

    // Generate embedding for query
    tracing::debug!("Generating embedding for query: '{}'", req.query);
    let embedding = state.embedder.embed(&req.query)
        .await
        .map_err(|e| {
            tracing::error!("Failed to generate embedding: {}", e);
            AppError::Internal(e.to_string())
        })?;
    tracing::debug!("Generated embedding with dimension: {}", embedding.len());

    let filters = db::SearchFilters {
        category_id: req.category_id,
        date_from: req.date_from.and_then(|d| d.parse().ok()),
        date_to: req.date_to.and_then(|d| d.parse().ok()),
        locations: req.locations,
        keywords: req.keywords,
        source_types: None,
        authors: req.authors,
        concepts: req.concepts,
        organizations: req.organizations,
        persons: req.persons,
        products: req.products,
        word_count_min: req.word_count_min,
        word_count_max: req.word_count_max,
    };

    tracing::debug!("Executing hybrid search with filters: {:?}", filters);
    let results = db::hybrid_search(
        &state.pool,
        &req.query,
        &embedding,
        &filters,
        req.limit,
        req.bm25_weight,
        req.vector_weight,
    ).await.map_err(|e| {
        tracing::error!("Hybrid search failed: {}", e);
        AppError::Internal(e.to_string())
    })?;

    tracing::info!("Search completed successfully, returning {} results", results.len());
    Ok(Json(results))
}

/// Chat with RAG context
pub async fn chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    tracing::info!("Received chat request: message='{}', context_chunks={}, document_ids={:?}",
        req.message, req.context_chunks, req.document_ids);

    // Generate embedding for the query
    tracing::debug!("Generating embedding for message");
    let embedding = state.embedder.embed(&req.message)
        .await
        .map_err(|e| {
            tracing::error!("Failed to generate embedding for chat: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Retrieve relevant chunks
    tracing::debug!("Retrieving {} relevant chunks", req.context_chunks);
    let chunks = db::get_relevant_chunks(
        &state.pool,
        &embedding,
        req.context_chunks,
        req.document_ids.as_deref()
    )
        .await
        .map_err(|e| {
            tracing::error!("Failed to retrieve chunks: {}", e);
            AppError::Internal(e.to_string())
        })?;
    tracing::debug!("Retrieved {} chunks", chunks.len());

    // Build context
    let context: String = chunks.iter()
        .map(|c| format!("---\n{}\n", c.content))
        .collect();

    // Build prompt - use system prompt from env or default
    let default_system_prompt = "You are a helpful assistant answering questions based on the provided context from documents. Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so. Be concise and cite specific parts of the context when relevant.";
    let env_system_prompt = std::env::var("RAG_SYSTEM_PROMPT").ok();
    let system_prompt = env_system_prompt.as_deref().unwrap_or(default_system_prompt);

    let user_prompt = format!(
        "CONTEXT:\n{}\n\nQUESTION:\n{}",
        context,
        req.message
    );

    // Call LLM
    tracing::debug!("Calling LLM with context");
    let config = state.llm_config.read().await;
    let response = llm::call_llm(&config, system_prompt, &user_prompt)
        .await
        .map_err(|e| {
            tracing::error!("LLM call failed: {}", e);
            AppError::Internal(e.to_string())
        })?;
    tracing::debug!("Received LLM response: {} chars", response.len());

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
    tracing::info!("Chat completed successfully, conversation_id={}, {} sources", conversation_id, sources.len());

    Ok(Json(ChatResponse {
        message: response,
        conversation_id,
        sources,
    }))
}

/// Stream chat response using Server-Sent Events
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<axum::response::Sse<impl futures::stream::Stream<Item = Result<Event, axum::Error>>>, AppError> {
    tracing::info!("Received streaming chat request: message='{}', context_chunks={}, document_ids={:?}",
        req.message, req.context_chunks, req.document_ids);

    // Generate embedding for the query
    let embedding = state.embedder.embed(&req.message)
        .await
        .map_err(|e| {
            tracing::error!("Failed to generate embedding for chat: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Retrieve relevant chunks
    let chunks = db::get_relevant_chunks(
        &state.pool,
        &embedding,
        req.context_chunks,
        req.document_ids.as_deref()
    )
        .await
        .map_err(|e| {
            tracing::error!("Failed to retrieve chunks: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Build context
    let context: String = chunks.iter()
        .map(|c| format!("---\n{}\n", c.content))
        .collect();

    let default_system_prompt = "You are a helpful assistant answering questions based on the provided context from documents. Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so. Be concise and cite specific parts of the context when relevant.";
    let env_system_prompt = std::env::var("RAG_SYSTEM_PROMPT").ok();
    let system_prompt = env_system_prompt.as_deref().unwrap_or(default_system_prompt).to_string();

    let user_prompt = format!(
        "CONTEXT:\n{}\n\nQUESTION:\n{}",
        context,
        req.message
    );

    // Get LLM config
    let config = state.llm_config.read().await.clone();
    let conversation_id = req.conversation_id.unwrap_or_else(Uuid::new_v4);

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

    let stream = async_stream::stream! {
        // Stream the LLM response
        match llm::stream_llm(&config, &system_prompt, &user_prompt).await {
            Ok(mut stream) => {
                while let Some(result) = stream.next().await {
                    match result {
                        Ok(chunk) if !chunk.is_empty() => {
                            let json = serde_json::json!({
                                "type": "chunk",
                                "content": chunk
                            });
                            if let Ok(event) = Event::default().json_data(json) {
                                yield Ok(event);
                            }
                        }
                        Err(e) => {
                            let error = serde_json::json!({
                                "type": "error",
                                "message": e.to_string()
                            });
                            if let Ok(event) = Event::default().json_data(error) {
                                yield Ok(event);
                            }
                            break;
                        }
                        _ => {}
                    }
                }

                // Send completion with sources
                let completion = serde_json::json!({
                    "type": "complete",
                    "conversation_id": conversation_id,
                    "sources": sources
                });
                if let Ok(event) = Event::default().json_data(completion) {
                    yield Ok(event);
                }

                tracing::info!("Stream completed successfully for conversation_id={}", conversation_id);
            }
            Err(e) => {
                tracing::error!("LLM streaming failed: {}", e);
                let error = serde_json::json!({
                    "type": "error",
                    "message": e.to_string()
                });
                if let Ok(event) = Event::default().json_data(error) {
                    yield Ok(event);
                }
            }
        }
    };

    Ok(axum::response::Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// List documents
pub async fn list_documents(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<crate::domain::models::Document>>, AppError> {
    let docs = db::list_documents(&state.pool, query.limit, query.offset)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    
    Ok(Json(docs))
}

/// Get single document
pub async fn get_document(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::domain::models::Document>, AppError> {
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
) -> Result<Json<Vec<crate::domain::models::DocumentAsset>>, AppError> {
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
) -> Result<Json<Vec<crate::domain::models::Category>>, AppError> {
    tracing::debug!("Received request to list categories");
    let categories = db::list_categories(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to list categories: {}", e);
            AppError::Internal(e.to_string())
        })?;

    tracing::info!("Returning {} categories", categories.len());
    Ok(Json(categories))
}

/// Index a new document
pub async fn index_document(
    State(state): State<AppState>,
    Json(req): Json<IndexRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if let Some(path) = req.path {
        indexing::index_path(&state.pool, &state.embedder, &path)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }
    
    if let Some(url) = req.url {
        indexing::index_url(&state.pool, &state.embedder, &url)
            .await
            .map_err(|e| AppError::Internal(e.to_string()))?;
    }

    Ok(Json(serde_json::json!({ "status": "indexed" })))
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
        embedding_config: EmbeddingInfo {
            model: state.embedder.get_model_name().to_string(),
            chunk_size: indexing::CHUNK_SIZE,
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

/// Get aggregation stats for faceted search
pub async fn get_aggregation_stats(
    State(state): State<AppState>,
) -> Result<Json<crate::domain::dtos::AggregationStats>, AppError> {
    let stats = db::get_aggregation_stats(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    Ok(Json(stats))
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
