use leptos::prelude::*;
use crate::domain::models::SearchResult;
use uuid::Uuid;
use serde::{Deserialize, Serialize};

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
pub async fn search_documents(
    request: SearchRequest,
) -> Result<Vec<SearchResult>, ServerFnError> {
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
    use crate::infra::db;
    use crate::api::state::AppState;
    use tracing::info;

    info!("SERVER: SearchDocuments called. Query: '{}'", query);

    // Extract the AppState from context
    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    // Generate embedding for the query
    let embedding = state.embedder.embed(&query).await
        .map_err(|e| ServerFnError::new(format!("Embedding failed: {}", e)))?;

    // Perform hybrid search with provided filters
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

    let results = db::hybrid_search(
        &state.pool,
        &query,
        &embedding,
        &db_filters,
        limit,
        bm25_weight,
        vector_weight,
    ).await.map_err(|e| ServerFnError::new(format!("Search failed: {}", e)))?;

    Ok(results)
}
