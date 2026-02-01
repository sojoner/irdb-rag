//! API handlers for RAG Chat

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::KeepAlive,
    response::{sse::Event, IntoResponse, Response},
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
    tracing::info!(
        "Received search request: query='{}', limit={}, bm25_weight={}, vector_weight={}",
        req.query,
        req.limit,
        req.bm25_weight,
        req.vector_weight
    );

    // Reject empty or special-only queries early to avoid embedding service timeout
    let trimmed_query = req.query.trim();
    if trimmed_query.is_empty() || trimmed_query == "*" {
        tracing::info!("Query rejected: empty or special character only");
        return Ok(Json(Vec::new()));
    }

    // Generate embedding for query
    tracing::debug!("Generating embedding for query: '{}'", req.query);
    let embedding = state.embedder.embed(&req.query).await.map_err(|e| {
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
        state.reranker.as_ref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Hybrid search failed: {}", e);
        AppError::Internal(e.to_string())
    })?;

    tracing::info!(
        "Search completed successfully, returning {} results",
        results.len()
    );
    Ok(Json(results))
}

/// Fast BM25-only search endpoint for UI
/// Optimized for keyword-based search without embedding generation
pub async fn search_bm25(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Vec<crate::domain::models::SearchResult>>, AppError> {
    tracing::info!("Received BM25 search request: query='{}', limit={}", req.query, req.limit);

    // Reject empty or special-only queries early
    let trimmed_query = req.query.trim();
    if trimmed_query.is_empty() || trimmed_query == "*" {
        tracing::info!("Query rejected: empty or special character only");
        return Ok(Json(Vec::new()));
    }

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

    let results = db::bm25_search(
        &state.pool,
        &req.query,
        &filters,
        req.limit,
        0,
        &req.sort,
    )
        .await
        .map_err(|e| {
            tracing::error!("BM25 search failed: {}", e);
            AppError::Internal(e.to_string())
        })?;

    tracing::info!("BM25 search completed successfully, returning {} results", results.len());
    Ok(Json(results))
}

/// Fast vector-only search endpoint for chat/RAG
/// Optimized for semantic similarity without BM25 overhead
pub async fn search_vector(
    State(state): State<AppState>,
    Json(req): Json<SearchRequest>,
) -> Result<Json<Vec<crate::domain::models::SearchResult>>, AppError> {
    tracing::info!("Received vector search request: query='{}', limit={}", req.query, req.limit);

    // Reject empty or special-only queries early
    let trimmed_query = req.query.trim();
    if trimmed_query.is_empty() || trimmed_query == "*" {
        tracing::info!("Query rejected: empty or special character only");
        return Ok(Json(Vec::new()));
    }

    // Generate embedding for query
    tracing::debug!("Generating embedding for query: '{}'", req.query);
    let embedding = state.embedder.embed(&req.query).await.map_err(|e| {
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

    let results = db::vector_search(&state.pool, &embedding, &filters, req.limit)
        .await
        .map_err(|e| {
            tracing::error!("Vector search failed: {}", e);
            AppError::Internal(e.to_string())
        })?;

    tracing::info!("Vector search completed successfully, returning {} results", results.len());
    Ok(Json(results))
}

/// Chat with RAG context
pub async fn chat(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Json<ChatResponse>, AppError> {
    tracing::info!(
        "Received chat request: message='{}', context_chunks={}, document_ids={:?}",
        req.message,
        req.context_chunks,
        req.document_ids
    );

    // Generate embedding for the query
    tracing::debug!("Generating embedding for message");
    let embedding = state.embedder.embed(&req.message).await.map_err(|e| {
        tracing::error!("Failed to generate embedding for chat: {}", e);
        AppError::Internal(e.to_string())
    })?;

    // Retrieve relevant chunks
    tracing::debug!("Retrieving {} relevant chunks", req.context_chunks);
    let mut chunks = db::get_relevant_chunks(
        &state.pool,
        &embedding,
        req.context_chunks,
        req.document_ids.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to retrieve chunks: {}", e);
        AppError::Internal(e.to_string())
    })?;
    tracing::debug!("Retrieved {} chunks", chunks.len());

    // Rerank chunks if available
    if let Some(reranker) = state.reranker.as_ref() {
        let chunk_contents: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();

        match reranker
            .rerank_and_sort(&req.message, &chunk_contents)
            .await
        {
            Ok(ranked) => {
                let mut reranked = Vec::new();
                for doc in ranked {
                    if let Some(chunk) = chunks.get(doc.index) {
                        reranked.push(chunk.clone());
                    }
                }
                tracing::debug!("Reranked {} chunks for chat context", reranked.len());
                chunks = reranked;
            }
            Err(e) => {
                tracing::warn!("Chunk reranking failed, using original order: {}", e);
            }
        }
    }

    // Build context
    let context: String = chunks
        .iter()
        .map(|c| format!("---\n{}\n", c.content))
        .collect();

    // Build prompt - use system prompt from settings or default
    let default_system_prompt = "You are a helpful assistant answering questions based on the provided context from documents. Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so. Be concise and cite specific parts of the context when relevant.";
    let system_prompt = state
        .settings
        .rag
        .system_prompt
        .as_deref()
        .unwrap_or(default_system_prompt);

    let user_prompt = format!("CONTEXT:\n{}\n\nQUESTION:\n{}", context, req.message);

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
    let sources: Vec<SourceReference> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| SourceReference {
            document_id: c.document_id,
            title: c
                .section_title
                .clone()
                .unwrap_or_else(|| format!("Chunk {}", i + 1)),
            chunk: c.content.chars().take(200).collect::<String>() + "...",
            relevance: 1.0 - (i as f64 * 0.1),
        })
        .collect();

    let conversation_id = req.conversation_id.unwrap_or_else(Uuid::new_v4);
    tracing::info!(
        "Chat completed successfully, conversation_id={}, {} sources",
        conversation_id,
        sources.len()
    );

    Ok(Json(ChatResponse {
        message: response,
        conversation_id,
        sources,
    }))
}

/// Chat with conversation history (new API)
pub async fn chat_conversation(
    State(state): State<AppState>,
    Json(req): Json<crate::domain::dtos::ChatConversationRequest>,
) -> Result<Json<crate::domain::dtos::ChatConversationResponse>, AppError> {
    tracing::info!(
        "Received conversation chat request: {} messages, context_chunks={}, document_ids={:?}",
        req.messages.len(),
        req.context_chunks,
        req.document_ids
    );

    // Get the last user message for context retrieval
    let last_user_message = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    if last_user_message.is_empty() {
        return Err(AppError::Internal("No user message found".to_string()));
    }

    // Generate embedding for the last user message
    tracing::debug!("Generating embedding for last user message");
    let embedding = state
        .embedder
        .embed(&last_user_message)
        .await
        .map_err(|e| {
            tracing::error!("Failed to generate embedding for chat: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Retrieve relevant chunks if document_ids provided
    let mut context = String::new();
    if req.document_ids.is_some() && req.context_chunks > 0 {
        let mut chunks = db::get_relevant_chunks(
            &state.pool,
            &embedding,
            req.context_chunks,
            req.document_ids.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to retrieve chunks: {}", e);
            AppError::Internal(e.to_string())
        })?;
        tracing::debug!("Retrieved {} chunks", chunks.len());

        // Rerank chunks if available
        if let Some(reranker) = state.reranker.as_ref() {
            let chunk_contents: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();

            match reranker
                .rerank_and_sort(&last_user_message, &chunk_contents)
                .await
            {
                Ok(ranked) => {
                    let mut reranked = Vec::new();
                    for doc in ranked {
                        if let Some(chunk) = chunks.get(doc.index) {
                            reranked.push(chunk.clone());
                        }
                    }
                    tracing::debug!("Reranked {} chunks for chat context", reranked.len());
                    chunks = reranked;
                }
                Err(e) => {
                    tracing::warn!("Chunk reranking failed, using original order: {}", e);
                }
            }
        }

        // Build context
        context = chunks
            .iter()
            .map(|c| format!("---\n{}\n", c.content))
            .collect();
    }

    // Build system prompt - use different prompts for RAG vs standalone chat
    let system_prompt = if !context.is_empty() {
        // RAG mode: use document-focused system prompt
        let default_rag_prompt = "You are a helpful assistant answering questions based on the provided context from documents. Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so. Be concise and cite specific parts of the context when relevant.";
        state
            .settings
            .rag
            .system_prompt
            .as_deref()
            .unwrap_or(default_rag_prompt)
    } else {
        // Standalone chat mode: use conversational system prompt
        let default_chat_prompt = "You are a helpful, friendly AI assistant. Be conversational, thoughtful, and engaging in your responses.";
        state
            .settings
            .rag
            .chat_system_prompt
            .as_deref()
            .unwrap_or(default_chat_prompt)
    };

    // Build conversation with context if available
    let mut messages_with_context = req.messages.clone();

    // If we have context, inject it before the last user message
    if !context.is_empty() {
        if let Some(last_idx) = messages_with_context.iter().rposition(|m| m.role == "user") {
            let original_message = messages_with_context[last_idx].content.clone();
            messages_with_context[last_idx].content = format!(
                "CONTEXT FROM DOCUMENTS:\n{}\n\nUSER QUESTION:\n{}",
                context, original_message
            );
        }
    }

    // Format messages for LLM
    let conversation_text = messages_with_context
        .iter()
        .map(|m| {
            if m.role == "user" {
                format!("User: {}", m.content)
            } else {
                format!("Assistant: {}", m.content)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Call LLM
    tracing::debug!("Calling LLM with conversation history");
    let config = state.llm_config.read().await;
    let response = llm::call_llm(&config, system_prompt, &conversation_text)
        .await
        .map_err(|e| {
            tracing::error!("LLM call failed: {}", e);
            AppError::Internal(e.to_string())
        })?;
    tracing::debug!("Received LLM response: {} chars", response.len());

    let conversation_id = req.conversation_id.unwrap_or_else(Uuid::new_v4);
    tracing::info!("Chat conversation completed successfully, conversation_id={}", conversation_id);

    Ok(Json(crate::domain::dtos::ChatConversationResponse {
        message: crate::domain::dtos::ChatMessage {
            role: "assistant".to_string(),
            content: response,
        },
        conversation_id,
        sources: vec![], // TODO: Add sources if context was used
    }))
}

/// Stream chat with conversation history
pub async fn chat_conversation_stream(
    State(state): State<AppState>,
    Json(req): Json<crate::domain::dtos::ChatConversationRequest>,
) -> Result<
    axum::response::Sse<impl futures::stream::Stream<Item = Result<Event, axum::Error>>>,
    AppError,
> {
    tracing::info!(
        "Received streaming conversation chat request: {} messages, context_chunks={}, document_ids={:?}",
        req.messages.len(),
        req.context_chunks,
        req.document_ids
    );

    // Get the last user message for context retrieval
    let last_user_message = req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content.clone())
        .unwrap_or_default();

    if last_user_message.is_empty() {
        return Err(AppError::Internal("No user message found".to_string()));
    }

    // Generate embedding for the last user message
    let embedding = state
        .embedder
        .embed(&last_user_message)
        .await
        .map_err(|e| {
            tracing::error!("Failed to generate embedding for chat: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Retrieve relevant chunks if document_ids provided
    let mut context = String::new();
    let mut sources: Vec<SourceReference> = vec![];

    if req.document_ids.is_some() && req.context_chunks > 0 {
        let mut chunks = db::get_relevant_chunks(
            &state.pool,
            &embedding,
            req.context_chunks,
            req.document_ids.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to retrieve chunks: {}", e);
            AppError::Internal(e.to_string())
        })?;

        // Rerank chunks if available
        if let Some(reranker) = state.reranker.as_ref() {
            let chunk_contents: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();

            match reranker
                .rerank_and_sort(&last_user_message, &chunk_contents)
                .await
            {
                Ok(ranked) => {
                    let mut reranked = Vec::new();
                    for doc in ranked {
                        if let Some(chunk) = chunks.get(doc.index) {
                            reranked.push(chunk.clone());
                        }
                    }
                    tracing::debug!(
                        "Reranked {} chunks for streaming chat context",
                        reranked.len()
                    );
                    chunks = reranked;
                }
                Err(e) => {
                    tracing::warn!("Chunk reranking failed, using original order: {}", e);
                }
            }
        }

        // Build context
        context = chunks
            .iter()
            .map(|c| format!("---\n{}\n", c.content))
            .collect();

        // Build sources
        sources = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| SourceReference {
                document_id: c.document_id,
                title: c
                    .section_title
                    .clone()
                    .unwrap_or_else(|| format!("Chunk {}", i + 1)),
                chunk: c.content.chars().take(200).collect::<String>() + "...",
                relevance: 1.0 - (i as f64 * 0.1),
            })
            .collect();
    }

    // Build system prompt - use different prompts for RAG vs standalone chat
    let system_prompt = if !context.is_empty() {
        // RAG mode: use document-focused system prompt
        let default_rag_prompt = "You are a helpful assistant answering questions based on the provided context from documents. Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so. Be concise and cite specific parts of the context when relevant.";
        state
            .settings
            .rag
            .system_prompt
            .as_deref()
            .unwrap_or(default_rag_prompt)
            .to_string()
    } else {
        // Standalone chat mode: use conversational system prompt
        let default_chat_prompt = "You are a helpful, friendly AI assistant. Be conversational, thoughtful, and engaging in your responses.";
        state
            .settings
            .rag
            .chat_system_prompt
            .as_deref()
            .unwrap_or(default_chat_prompt)
            .to_string()
    };

    // Build conversation with context if available
    let mut messages_with_context = req.messages.clone();

    // If we have context, inject it before the last user message
    if !context.is_empty() {
        if let Some(last_idx) = messages_with_context.iter().rposition(|m| m.role == "user") {
            let original_message = messages_with_context[last_idx].content.clone();
            messages_with_context[last_idx].content = format!(
                "CONTEXT FROM DOCUMENTS:\n{}\n\nUSER QUESTION:\n{}",
                context, original_message
            );
        }
    }

    // Apply token budget management to conversation history
    let budget = crate::services::context_manager::ContextBudget::new(4096, 200, 1024);
    let trimmed_messages = budget.trim_conversation_history(messages_with_context.clone());

    // Format messages for LLM
    let conversation_text = trimmed_messages
        .iter()
        .map(|m| {
            if m.role == "user" {
                format!("User: {}", m.content)
            } else {
                format!("Assistant: {}", m.content)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    // Get LLM config
    let config = state.llm_config.read().await.clone();
    let conversation_id = req.conversation_id.unwrap_or_else(Uuid::new_v4);

    // Clone for use in async block
    let pool = state.pool.clone();
    let last_user_message_clone = last_user_message.clone();

    let stream = async_stream::stream! {
        // Stream the LLM response
        match llm::stream_llm(&config, &system_prompt, &conversation_text).await {
            Ok(mut llm_stream) => {
                let mut full_response = String::new();

                while let Some(result) = llm_stream.next().await {
                    match result {
                        Ok(chunk) if !chunk.is_empty() => {
                            full_response.push_str(&chunk);
                            let json = serde_json::json!({
                                "type": "chunk",
                                "content": chunk
                            });
                            if let Ok(event) = Event::default().json_data(json) {
                                yield Ok(event);
                            }
                        }
                        Err(e) => {
                            tracing::error!("LLM stream error: {}", e);
                            let error = serde_json::json!({
                                "type": "error",
                                "message": e.to_string(),
                                "retryable": true
                            });
                            if let Ok(event) = Event::default().json_data(error) {
                                yield Ok(event);
                            }
                            break;
                        }
                        _ => {}
                    }
                }

                // Persist messages to database
                if !full_response.is_empty() {
                    // Save user message
                    if let Err(e) = db::save_message(&pool, conversation_id, "user", &last_user_message_clone).await {
                        tracing::error!("Failed to save user message: {}", e);
                    }

                    // Save assistant response
                    if let Err(e) = db::save_message(&pool, conversation_id, "assistant", &full_response).await {
                        tracing::error!("Failed to save assistant message: {}", e);
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
                    "message": e.to_string(),
                    "retryable": false
                });
                if let Ok(event) = Event::default().json_data(error) {
                    yield Ok(event);
                }
            }
        }
    };

    Ok(axum::response::Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// Stream chat response using Server-Sent Events (legacy API)
pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<
    axum::response::Sse<impl futures::stream::Stream<Item = Result<Event, axum::Error>>>,
    AppError,
> {
    tracing::info!(
        "Received streaming chat request: message='{}', context_chunks={}, document_ids={:?}",
        req.message,
        req.context_chunks,
        req.document_ids
    );

    // Generate embedding for the query
    let embedding = state.embedder.embed(&req.message).await.map_err(|e| {
        tracing::error!("Failed to generate embedding for chat: {}", e);
        AppError::Internal(e.to_string())
    })?;

    // Retrieve relevant chunks
    let mut chunks = db::get_relevant_chunks(
        &state.pool,
        &embedding,
        req.context_chunks,
        req.document_ids.as_deref(),
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to retrieve chunks: {}", e);
        AppError::Internal(e.to_string())
    })?;

    // Rerank chunks if available
    if let Some(reranker) = state.reranker.as_ref() {
        let chunk_contents: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();

        match reranker
            .rerank_and_sort(&req.message, &chunk_contents)
            .await
        {
            Ok(ranked) => {
                let mut reranked = Vec::new();
                for doc in ranked {
                    if let Some(chunk) = chunks.get(doc.index) {
                        reranked.push(chunk.clone());
                    }
                }
                tracing::debug!(
                    "Reranked {} chunks for streaming chat context",
                    reranked.len()
                );
                chunks = reranked;
            }
            Err(e) => {
                tracing::warn!("Chunk reranking failed, using original order: {}", e);
            }
        }
    }

    // Build context
    let context: String = chunks
        .iter()
        .map(|c| format!("---\n{}\n", c.content))
        .collect();

    let default_system_prompt = "You are a helpful assistant answering questions based on the provided context from documents. Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so. Be concise and cite specific parts of the context when relevant.";
    let system_prompt = state
        .settings
        .rag
        .system_prompt
        .as_deref()
        .unwrap_or(default_system_prompt)
        .to_string();

    let user_prompt = format!("CONTEXT:\n{}\n\nQUESTION:\n{}", context, req.message);

    // Get LLM config
    let config = state.llm_config.read().await.clone();
    let conversation_id = req.conversation_id.unwrap_or_else(Uuid::new_v4);

    // Build sources
    let sources: Vec<SourceReference> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| SourceReference {
            document_id: c.document_id,
            title: c
                .section_title
                .clone()
                .unwrap_or_else(|| format!("Chunk {}", i + 1)),
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
    let categories = db::list_categories(&state.pool).await.map_err(|e| {
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
pub async fn get_status(State(state): State<AppState>) -> Result<Json<SystemStatus>, AppError> {
    let db_stats = db::get_db_stats(&state.pool)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let config = state.llm_config.read().await.clone();

    Ok(Json(SystemStatus {
        db_stats,
        llm_config: config.clone(),
        embedding_config: EmbeddingInfo {
            model: state.embedder.get_model_name().to_string(),
            chunk_size: indexing::DEFAULT_CHUNK_SIZE,
            chunk_overlap: 0,
        },
        env_vars: EnvVars {
            database_url: "postgres://***@***".to_string(), // Masked
            llm_provider: config.provider,
            llm_model: config.model,
            llm_api_url: config.api_url,
            embedding_api_url: state.embedder.get_api_url().to_string(),
            docling_url: state.settings.docling.url.clone(),
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

    Ok(Json(
        serde_json::json!({ "status": "updated", "config": *config }),
    ))
}

/// Get recent logs
pub async fn get_logs(State(state): State<AppState>) -> Json<Vec<String>> {
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

/// Get database and index statistics
pub async fn get_db_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let doc_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to count documents: {}", e)))?;

    let chunk_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM document_chunks")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to count chunks: {}", e)))?;

    let stats = serde_json::json!({
        "documents": doc_count.0,
        "chunks": chunk_count.0,
    });

    Ok(Json(stats))
}

/// Faceted search - returns search results with facet aggregations
pub async fn faceted_search(
    State(state): State<AppState>,
    Json(req): Json<crate::domain::dtos::FacetedSearchRequest>,
) -> Result<Json<crate::domain::dtos::FacetedSearchResponse>, AppError> {
    tracing::info!(
        "Received faceted search request: query='{}', facet_limit={}",
        req.query,
        req.facet_limit
    );

    let trimmed_query = req.query.trim();
    if trimmed_query.is_empty() || trimmed_query == "*" {
        tracing::info!("Query rejected: empty or special character only");
        return Ok(Json(crate::domain::dtos::FacetedSearchResponse {
            results: Vec::new(),
            facets: Vec::new(),
            total_results: 0,
        }));
    }

    // Generate embedding for query
    tracing::debug!("Generating embedding for query: '{}'", req.query);
    let embedding = state.embedder.embed(&req.query).await.map_err(|e| {
        tracing::error!("Failed to generate embedding: {}", e);
        AppError::Internal(e.to_string())
    })?;

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
        word_count_min: None,
        word_count_max: None,
    };

    tracing::debug!("Executing faceted search with filters: {:?}", filters);
    let (results, facets) = db::search_with_facets(
        &state.pool,
        &req.query,
        &embedding,
        &filters,
        req.limit,
        req.bm25_weight,
        req.vector_weight,
        req.facet_limit,
        state.reranker.as_ref().map(|r| r.as_ref()),
    )
    .await
    .map_err(|e| {
        tracing::error!("Faceted search failed: {}", e);
        AppError::Internal(e.to_string())
    })?;

    let total_results = results.len() as i64;
    tracing::info!(
        "Faceted search completed: {} results, {} facets",
        results.len(),
        facets.len()
    );

    Ok(Json(crate::domain::dtos::FacetedSearchResponse {
        results,
        facets,
        total_results,
    }))
}

/// Get facet values for a specific facet type
pub async fn get_facet_values(
    State(state): State<AppState>,
    Json(req): Json<crate::domain::dtos::FacetValuesRequest>,
) -> Result<Json<crate::domain::dtos::FacetValuesResponse>, AppError> {
    tracing::info!(
        "Received facet values request for facet_type: '{}'",
        req.facet_type
    );

    let filters = db::SearchFilters {
        category_id: req.category_id,
        date_from: req.date_from.and_then(|d| d.parse().ok()),
        date_to: req.date_to.and_then(|d| d.parse().ok()),
        locations: req.locations,
        keywords: req.keywords,
        source_types: None,
        authors: req.authors,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
    };

    let values = db::get_facet_values(
        &state.pool,
        &req.facet_type,
        req.query.as_deref(),
        &filters,
        req.limit,
    )
    .await
    .map_err(|e| {
        tracing::error!("Failed to get facet values: {}", e);
        AppError::Internal(e.to_string())
    })?;

    tracing::info!(
        "Retrieved {} values for facet '{}'",
        values.len(),
        req.facet_type
    );

    Ok(Json(crate::domain::dtos::FacetValuesResponse {
        facet_type: req.facet_type,
        values,
    }))
}

// ============================================
// Import Handlers
// ============================================

/// Create a new import job
pub async fn create_import(
    State(state): State<AppState>,
    Json(req): Json<CreateImportRequest>,
) -> Result<Json<ImportJobResponse>, AppError> {
    tracing::info!("Creating import job: source_type={}", req.source_type);

    let job_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    // Create the import job
    sqlx::query(
        r#"
        INSERT INTO import_jobs (id, status, source_type, source_path, created_at)
        VALUES ($1, $2, $3, $4, $5)
        "#,
    )
    .bind(job_id)
    .bind("pending")
    .bind(&req.source_type)
    .bind(&req.source_path)
    .bind(now)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create import job: {}", e);
        AppError::Internal(e.to_string())
    })?;

    tracing::info!("Created import job: {}", job_id);

    // Discover and create import items based on source type
    let mut item_paths: Vec<String> = vec![];

    match req.source_type.as_str() {
        "file" => {
            // Single file import
            if let Some(path) = &req.source_path {
                if std::path::Path::new(path).exists() {
                    item_paths.push(path.clone());
                } else {
                    tracing::error!("File not found: {}", path);
                    return Err(AppError::Internal(format!("File not found: {}", path)));
                }
            }
        }
        "folder" => {
            // Folder import - discover all indexable files
            if let Some(folder) = &req.source_path {
                match crate::services::import::discover_files(folder) {
                    Ok(files) => {
                        item_paths = files
                            .into_iter()
                            .filter_map(|p| p.to_str().map(|s| s.to_string()))
                            .collect();
                    }
                    Err(e) => {
                        tracing::error!("Failed to discover files in {}: {}", folder, e);
                        return Err(AppError::Internal(format!(
                            "Failed to discover files: {}",
                            e
                        )));
                    }
                }
            }
        }
        "url" => {
            // Single URL import
            if let Some(url) = &req.source_path {
                item_paths.push(url.clone());
            }
        }
        "urls" => {
            // Multiple URLs
            if let Some(urls) = &req.urls {
                item_paths.extend_from_slice(urls);
            }
        }
        _ => {
            return Err(AppError::Internal(format!(
                "Invalid source_type: {}",
                req.source_type
            )));
        }
    }

    // Create import items
    for path in &item_paths {
        let item_id = uuid::Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO import_items (id, job_id, source_path, status, created_at)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(item_id)
        .bind(job_id)
        .bind(path)
        .bind("pending")
        .bind(now)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create import item for {}: {}", path, e);
            AppError::Internal(e.to_string())
        })?;
    }

    // Update job total_items
    let total_items = item_paths.len() as i32;
    sqlx::query(
        r#"
        UPDATE import_jobs SET total_items = $1 WHERE id = $2
        "#,
    )
    .bind(total_items)
    .bind(job_id)
    .execute(&state.pool)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update job total_items: {}", e);
        AppError::Internal(e.to_string())
    })?;

    tracing::info!("Created {} import items for job {}", total_items, job_id);

    // Send job to the worker queue for immediate processing
    state.import_job_queue.send(job_id).await.map_err(|e| {
        tracing::error!("Failed to enqueue import job {}: {}", job_id, e);
        AppError::Internal(format!("Failed to enqueue import job: {}", e))
    })?;
    tracing::info!("Enqueued import job {} for processing", job_id);

    Ok(Json(ImportJobResponse {
        id: job_id,
        status: "pending".to_string(),
        source_type: req.source_type,
        source_path: req.source_path,
        total_items,
        processed_items: 0,
        failed_items: 0,
        skipped_items: 0,
        created_at: now.to_rfc3339(),
        started_at: None,
        completed_at: None,
        error_message: None,
    }))
}

/// Get import job status and progress
pub async fn get_import_status(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
) -> Result<Json<ImportProgressResponse>, AppError> {
    let job = db::get_import_job(&state.pool, job_id)
        .await
        .map_err(|_| AppError::NotFound)?;

    let progress = crate::domain::models::ImportProgress::from_job(&job);

    Ok(Json(ImportProgressResponse {
        id: job.id,
        status: job.status,
        progress,
    }))
}

/// List import jobs
pub async fn list_imports(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (jobs, total) = db::list_import_jobs(&state.pool, q.limit, q.offset)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let job_responses: Vec<ImportJobResponse> = jobs
        .into_iter()
        .map(|job| ImportJobResponse {
            id: job.id,
            status: job.status,
            source_type: job.source_type,
            source_path: job.source_path,
            total_items: job.total_items,
            processed_items: job.processed_items,
            failed_items: job.failed_items,
            skipped_items: job.skipped_items,
            created_at: job.created_at.to_rfc3339(),
            started_at: job.started_at.map(|t| t.to_rfc3339()),
            completed_at: job.completed_at.map(|t| t.to_rfc3339()),
            error_message: job.error_message,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "jobs": job_responses,
        "total": total,
        "limit": q.limit,
        "offset": q.offset,
    })))
}

/// Get items for an import job
pub async fn get_import_items(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Query(q): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (items, total) = db::get_import_items(&state.pool, job_id, q.limit, q.offset)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    let item_responses: Vec<ImportItemResponse> = items
        .into_iter()
        .map(|item| ImportItemResponse {
            id: item.id,
            job_id: item.job_id,
            source_path: item.source_path,
            status: item.status,
            retry_count: item.retry_count,
            error_message: item.error_message,
            error_type: item.error_type,
            document_id: item.document_id,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "items": item_responses,
        "total": total,
        "limit": q.limit,
        "offset": q.offset,
    })))
}

/// Resume a failed import job (retry failed items)
pub async fn resume_import(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(_req): Json<ResumeImportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Get failed items
    let items = db::get_failed_items(&state.pool, job_id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    if items.is_empty() {
        return Ok(Json(serde_json::json!({
            "status": "no_items_to_retry",
            "message": "No failed items to retry"
        })));
    }

    // Reset failed items to pending for retry
    for item in items {
        sqlx::query("UPDATE import_items SET status = 'pending', retry_count = $1 WHERE id = $2")
            .bind(item.retry_count)
            .bind(item.id)
            .execute(&state.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to reset item for retry: {}", e);
                AppError::Internal(e.to_string())
            })?;
    }

    tracing::info!("Resumed import job: {}", job_id);

    Ok(Json(serde_json::json!({
        "status": "resumed",
        "message": "Import job resumed, failed items reset for retry"
    })))
}

/// Delete an import job and optionally its imported documents
pub async fn delete_import(
    State(state): State<AppState>,
    Path(job_id): Path<Uuid>,
    Json(req): Json<DeleteImportRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    use crate::services::import::{ImportConfig, ImportJobRunner};

    let config = ImportConfig::from_env();
    let runner = ImportJobRunner::new(config);

    let rows_affected = runner
        .delete_job(&state.pool, job_id, req.delete_documents)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete import job: {}", e);
            AppError::Internal(e.to_string())
        })?;

    tracing::info!(
        "Deleted import job: {} (rows affected: {})",
        job_id,
        rows_affected
    );

    Ok(Json(serde_json::json!({
        "status": "deleted",
        "message": format!("Import job deleted successfully (documents_deleted: {})", req.delete_documents),
        "rows_affected": rows_affected
    })))
}

// ============================================
// Document Deletion
// ============================================

/// Delete a single document
pub async fn delete_document(
    State(state): State<AppState>,
    Path(doc_id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = db::delete_document(&state.pool, doc_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete document {}: {}", doc_id, e);
            AppError::Internal(e.to_string())
        })?;

    if rows == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(serde_json::json!({
        "status": "deleted",
        "id": doc_id,
        "rows_affected": rows
    })))
}

/// Delete multiple documents
pub async fn delete_documents_batch(
    State(state): State<AppState>,
    Json(req): Json<crate::domain::dtos::DeleteDocumentsRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = db::delete_documents_batch(&state.pool, &req.ids)
        .await
        .map_err(|e| {
            tracing::error!("Failed to batch delete documents: {}", e);
            AppError::Internal(e.to_string())
        })?;

    Ok(Json(serde_json::json!({
        "status": "deleted",
        "deleted_count": rows,
        "requested_count": req.ids.len()
    })))
}

// ============================================
// Knowledge Base Handlers
// ============================================

/// Add knowledge base paths (local paths and/or URLs)
pub async fn add_knowledge_base_paths(
    State(state): State<AppState>,
    Json(req): Json<AddKnowledgeBasePathsRequest>,
) -> Result<Json<AddKnowledgeBasePathsResponse>, AppError> {
    use crate::services::import::ImportJobRunner;

    tracing::info!("Received request to add knowledge base paths");

    let mut sources = Vec::new();
    let mut skipped = 0;

    // Add local paths
    if let Some(paths) = &req.local_paths {
        for path in paths {
            // Check if already indexed
            let doc_count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM documents WHERE source_path = $1")
                    .bind(path)
                    .fetch_one(&state.pool)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to check document existence: {}", e);
                        AppError::Internal(e.to_string())
                    })?;

            if doc_count.0 > 0 {
                tracing::debug!("Path already indexed: {}", path);
                skipped += 1;
            } else {
                sources.push(path.clone());
            }
        }
    }

    // Add URLs
    if let Some(urls) = &req.urls {
        for url in urls {
            // Check if already indexed
            let doc_count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM documents WHERE source_path = $1")
                    .bind(url)
                    .fetch_one(&state.pool)
                    .await
                    .map_err(|e| {
                        tracing::error!("Failed to check URL existence: {}", e);
                        AppError::Internal(e.to_string())
                    })?;

            if doc_count.0 > 0 {
                tracing::debug!("URL already indexed: {}", url);
                skipped += 1;
            } else {
                sources.push(url.clone());
            }
        }
    }

    if sources.is_empty() {
        return Ok(Json(AddKnowledgeBasePathsResponse {
            job_id: Uuid::new_v4(),
            paths_queued: 0,
            paths_skipped: skipped,
            message: "No new sources to index".to_string(),
        }));
    }

    // Create import job
    let settings = crate::config::Settings::new().map_err(|e| AppError::Internal(e.to_string()))?;
    let runner = ImportJobRunner::new(settings.import.clone());

    let job_id = runner
        .create_job(&state.pool, "manual_add_paths", None)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create import job: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Create import items
    let source_refs: Vec<&str> = sources.iter().map(|s| s.as_str()).collect();
    crate::services::import::ImportItemManager
        .create_items(&state.pool, job_id, source_refs)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create import items: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Queue job
    state.import_job_queue.send(job_id).await.map_err(|e| {
        tracing::error!("Failed to queue import job: {}", e);
        AppError::Internal(e.to_string())
    })?;

    tracing::info!("Created import job {} with {} paths", job_id, sources.len());

    Ok(Json(AddKnowledgeBasePathsResponse {
        job_id,
        paths_queued: sources.len(),
        paths_skipped: skipped,
        message: format!(
            "Queued {} sources for import ({} already indexed)",
            sources.len(),
            skipped
        ),
    }))
}

/// Import Chrome bookmarks
pub async fn import_chrome_bookmarks(
    State(state): State<AppState>,
    Json(req): Json<ImportBookmarksRequest>,
) -> Result<Json<AddKnowledgeBasePathsResponse>, AppError> {
    use crate::services::bookmark_parser;
    use crate::services::import::ImportJobRunner;

    tracing::info!(
        "Received request to import Chrome bookmarks from: {}",
        req.path
    );

    // Parse bookmarks
    let urls = bookmark_parser::parse_chrome_bookmarks(&req.path).map_err(|e| {
        tracing::error!("Failed to parse bookmarks: {}", e);
        AppError::Internal(e.to_string())
    })?;

    if urls.is_empty() {
        return Ok(Json(AddKnowledgeBasePathsResponse {
            job_id: Uuid::new_v4(),
            paths_queued: 0,
            paths_skipped: 0,
            message: "No URLs found in bookmarks file".to_string(),
        }));
    }

    // Filter out already-indexed URLs
    let mut filtered_urls = Vec::new();
    let mut skipped = 0;

    for url in urls {
        let doc_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM documents WHERE source_path = $1")
                .bind(&url)
                .fetch_one(&state.pool)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to check URL existence: {}", e);
                    AppError::Internal(e.to_string())
                })?;

        if doc_count.0 > 0 {
            skipped += 1;
        } else {
            filtered_urls.push(url);
        }
    }

    if filtered_urls.is_empty() {
        return Ok(Json(AddKnowledgeBasePathsResponse {
            job_id: Uuid::new_v4(),
            paths_queued: 0,
            paths_skipped: skipped,
            message: "All bookmarks already indexed".to_string(),
        }));
    }

    // Create import job
    let settings = crate::config::Settings::new().map_err(|e| AppError::Internal(e.to_string()))?;
    let runner = ImportJobRunner::new(settings.import.clone());

    let job_id = runner
        .create_job(&state.pool, "chrome_bookmarks", Some(&req.path))
        .await
        .map_err(|e| {
            tracing::error!("Failed to create import job: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Create import items
    let url_refs: Vec<&str> = filtered_urls.iter().map(|u| u.as_str()).collect();
    crate::services::import::ImportItemManager
        .create_items(&state.pool, job_id, url_refs)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create import items: {}", e);
            AppError::Internal(e.to_string())
        })?;

    // Queue job
    state.import_job_queue.send(job_id).await.map_err(|e| {
        tracing::error!("Failed to queue import job: {}", e);
        AppError::Internal(e.to_string())
    })?;

    tracing::info!(
        "Created import job {} with {} bookmarks",
        job_id,
        filtered_urls.len()
    );

    Ok(Json(AddKnowledgeBasePathsResponse {
        job_id,
        paths_queued: filtered_urls.len(),
        paths_skipped: skipped,
        message: format!(
            "Queued {} bookmarks for import ({} already indexed)",
            filtered_urls.len(),
            skipped
        ),
    }))
}

/// Manually trigger knowledge base scan
pub async fn trigger_scan(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let settings = crate::config::Settings::new().map_err(|e| AppError::Internal(e.to_string()))?;

    let scanner = crate::services::startup_scan::StartupScanner::new(
        state.pool.clone(),
        settings.knowledge_base.clone(),
        state.import_job_queue.clone(),
    );

    // Spawn scan in background
    tokio::spawn(async move {
        if let Err(e) = scanner.run().await {
            tracing::error!("Manual scan failed: {}", e);
        }
    });

    Ok(Json(serde_json::json!({
        "status": "scanning",
        "message": "Knowledge base scan started in background"
    })))
}

// ============================================
// Error Handling
// ============================================

// ============================================
// Conversation Management Handlers
// ============================================

/// List all conversations
pub async fn list_conversations(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ConversationsListResponse>, AppError> {
    #[derive(sqlx::FromRow)]
    struct ConversationRow {
        id: Uuid,
        title: Option<String>,
        created_at: String,
        updated_at: String,
        message_count: i64,
    }

    let rows: Vec<ConversationRow> = sqlx::query_as(
        r#"
        SELECT
            c.id,
            c.title,
            c.created_at::TEXT as created_at,
            c.updated_at::TEXT as updated_at,
            COUNT(m.id) as message_count
        FROM conversations c
        LEFT JOIN messages m ON c.id = m.conversation_id
        GROUP BY c.id, c.title, c.created_at, c.updated_at
        ORDER BY c.updated_at DESC
        LIMIT $1 OFFSET $2
        "#
    )
    .bind(query.limit as i64)
    .bind(query.offset as i64)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    let conversations: Vec<ConversationInfo> = rows.into_iter().map(|row| ConversationInfo {
        id: row.id,
        title: row.title,
        created_at: row.created_at,
        updated_at: row.updated_at,
        message_count: row.message_count,
    }).collect();

    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM conversations")
        .fetch_one(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    Ok(Json(ConversationsListResponse {
        conversations,
        total: total.0,
    }))
}

/// Create a new conversation
pub async fn create_conversation(
    State(state): State<AppState>,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<CreateConversationResponse>, AppError> {
    let conversation_id = Uuid::new_v4();

    sqlx::query("INSERT INTO conversations (id, title) VALUES ($1, $2)")
        .bind(conversation_id)
        .bind(&req.title)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    Ok(Json(CreateConversationResponse {
        id: conversation_id,
        title: req.title,
    }))
}

/// Delete a conversation (cascade deletes messages)
pub async fn delete_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<DeleteResponse>, AppError> {
    let result = sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }

    Ok(Json(DeleteResponse {
        status: "success".to_string(),
        deleted_count: result.rows_affected(),
    }))
}

/// Get a specific conversation with its messages
pub async fn get_conversation(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Get conversation info
    let conversation: Option<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT id, title FROM conversations WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    if conversation.is_none() {
        return Err(AppError::NotFound);
    }

    let (conversation_id, title) = conversation.unwrap();

    // Get messages
    let messages: Vec<ChatMessage> = sqlx::query_as(
        "SELECT role, content FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC"
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

    Ok(Json(serde_json::json!({
        "id": conversation_id,
        "title": title,
        "messages": messages
    })))
}

#[derive(Debug)]
pub enum AppError {
    NotFound,
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not found").into_response(),
            AppError::Internal(msg) => {
                tracing::error!("Internal error: {}", msg);
                (StatusCode::INTERNAL_SERVER_ERROR, msg).into_response()
            }
        }
    }
}
