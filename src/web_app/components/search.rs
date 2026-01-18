use crate::domain::models::SearchResult;
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SearchFilters {
    pub category_id: Option<Uuid>,
    pub keywords: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub locations: Option<Vec<String>>,
    pub persons: Option<Vec<String>>,
    pub organizations: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchRequest {
    pub query: String,
    pub limit: i32,
    pub bm25_weight: f64,
    pub vector_weight: f64,
    pub category_id: Option<Uuid>,
    pub keywords: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub locations: Option<Vec<String>>,
    pub persons: Option<Vec<String>>,
    pub organizations: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
}

#[server(SearchDocuments, "/api")]
pub async fn search_documents(request: SearchRequest) -> Result<Vec<SearchResult>, ServerFnError> {
    let SearchRequest {
        query,
        limit,
        bm25_weight,
        vector_weight,
        category_id,
        keywords,
        concepts,
        locations,
        persons,
        organizations,
        authors,
    } = request;
    use crate::api::state::AppState;
    use crate::infra::db;
    use crate::infra::db_utils;
    use tracing::info;

    info!("========== SEARCH DEBUG START ==========");
    info!("Query: '{}'", query);

    // Debug: Show tokenization
    let tokens = db_utils::tokenize_query(&query);
    info!("Tokens: {:?}", tokens);

    // Debug: Show built queries
    let phrase_query = db_utils::build_phrase_query(&tokens);
    info!("Phrase query: {}", phrase_query);

    let boolean_query = db_utils::build_boolean_query(&query);
    info!("Boolean query: {}", boolean_query);

    let prefix_query = db_utils::build_prefix_query(&query);
    info!("Prefix query: {}", prefix_query);

    let sanitized = db_utils::sanitize_bm25_query(&query);
    info!("Sanitized BM25: {}", sanitized);

    info!("Weights: BM25={}, Vector={}", bm25_weight, vector_weight);
    info!("========== SEARCH DEBUG END ==========");

    // Extract the AppState from context
    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    // Check if we have any search criteria
    let trimmed_query = query.trim();
    let has_query = !trimmed_query.is_empty() && trimmed_query != "*";
    let has_filters = category_id.is_some()
        || (keywords.as_ref().map_or(false, |k| !k.is_empty()))
        || (concepts.as_ref().map_or(false, |c| !c.is_empty()))
        || (locations.as_ref().map_or(false, |l| !l.is_empty()))
        || (persons.as_ref().map_or(false, |p| !p.is_empty()))
        || (organizations.as_ref().map_or(false, |o| !o.is_empty()))
        || (authors.as_ref().map_or(false, |a| !a.is_empty()));

    if !has_query && !has_filters {
        info!("SERVER: No query or filters provided, returning empty results");
        return Ok(Vec::new());
    }

    // Build filter object
    let db_filters = db::SearchFilters {
        category_id,
        date_from: None,
        date_to: None,
        locations: locations.filter(|v| !v.is_empty()),
        keywords: keywords.filter(|v| !v.is_empty()),
        source_types: None,
        authors: authors.filter(|v| !v.is_empty()),
        concepts: concepts.filter(|v| !v.is_empty()),
        organizations: organizations.filter(|v| !v.is_empty()),
        persons: persons.filter(|v| !v.is_empty()),
        products: None,
        word_count_min: None,
        word_count_max: None,
    };

    // If we have a query, use hybrid search; otherwise use filter-only search
    if has_query {
        // Generate embedding for the query
        let embedding = state
            .embedder
            .embed(&query)
            .await
            .map_err(|e| ServerFnError::new(format!("Embedding failed: {}", e)))?;

        let results = db::hybrid_search(
            &state.pool,
            &query,
            &embedding,
            &db_filters,
            limit,
            bm25_weight,
            vector_weight,
            state.reranker.as_ref(),
        )
        .await
        .map_err(|e| ServerFnError::new(format!("Search failed: {}", e)))?;

        info!("SERVER: Hybrid search returned {} results", results.len());
        Ok(results)
    } else {
        // Filter-only search (no text/semantic search, pure filter matching)
        let results = db::filter_only_search(&state.pool, &db_filters, limit)
            .await
            .map_err(|e| ServerFnError::new(format!("Search failed: {}", e)))?;

        info!(
            "SERVER: Filter-only search returned {} results",
            results.len()
        );
        Ok(results)
    }
}

#[server(DeleteDocument, "/api")]
pub async fn delete_document(doc_id: Uuid) -> Result<u64, ServerFnError> {
    use crate::api::state::AppState;

    let state =
        use_context::<AppState>().ok_or_else(|| ServerFnError::new("AppState not found"))?;

    let rows = crate::infra::db::delete_document(&state.pool, doc_id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(rows)
}

#[server(DeleteDocumentsBatch, "/api")]
pub async fn delete_documents_batch(doc_ids: Vec<Uuid>) -> Result<u64, ServerFnError> {
    use crate::api::state::AppState;

    let state =
        use_context::<AppState>().ok_or_else(|| ServerFnError::new("AppState not found"))?;

    let rows = crate::infra::db::delete_documents_batch(&state.pool, &doc_ids)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(rows)
}
