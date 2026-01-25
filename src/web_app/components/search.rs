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
#[serde(default)]
pub struct SearchRequest {
    pub query: String,
    pub limit: i32,
    pub search_fields: Vec<String>,
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

impl Default for SearchRequest {
    fn default() -> Self {
        Self {
            query: String::new(),
            limit: 20,
            search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
            bm25_weight: 0.5,
            vector_weight: 0.5,
            category_id: None,
            keywords: None,
            concepts: None,
            locations: None,
            persons: None,
            organizations: None,
            authors: None,
        }
    }
}

#[server(SearchDocuments, "/api")]
pub async fn search_documents(request: SearchRequest) -> Result<Vec<SearchResult>, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;
    use tracing::info;

    let query = request.query;
    let limit = request.limit;
    let search_fields = request.search_fields;

    info!("========== SIMPLE BM25 SEARCH ==========");
    info!("Query: '{}', Fields: {:?}", query, search_fields);

    // Extract the AppState from context
    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    // Check if we have any search criteria
    let trimmed_query = query.trim();
    let has_query = !trimmed_query.is_empty() && trimmed_query != "*";
    let has_filters = request.category_id.is_some()
        || (request.keywords.as_ref().map_or(false, |k| !k.is_empty()))
        || (request.concepts.as_ref().map_or(false, |c| !c.is_empty()))
        || (request.locations.as_ref().map_or(false, |l| !l.is_empty()))
        || (request.persons.as_ref().map_or(false, |p| !p.is_empty()))
        || (request.organizations.as_ref().map_or(false, |o| !o.is_empty()))
        || (request.authors.as_ref().map_or(false, |a| !a.is_empty()));

    if !has_query && !has_filters {
        info!("SERVER: No query or filters provided, returning empty results");
        return Ok(Vec::new());
    }

    // Build filter object
    let db_filters = db::SearchFilters {
        category_id: request.category_id,
        date_from: None,
        date_to: None,
        locations: request.locations.filter(|v| !v.is_empty()),
        keywords: request.keywords.filter(|v| !v.is_empty()),
        source_types: None,
        authors: request.authors.filter(|v| !v.is_empty()),
        concepts: request.concepts.filter(|v| !v.is_empty()),
        organizations: request.organizations.filter(|v| !v.is_empty()),
        persons: request.persons.filter(|v| !v.is_empty()),
        products: None,
        word_count_min: None,
        word_count_max: None,
    };

    // If we have a query, use fast BM25 search; otherwise use filter-only search
    if has_query {
        // Use the optimized BM25 search (searches documents table directly)
        let results = db::bm25_search(&state.pool, &query, &db_filters, limit)
            .await
            .map_err(|e| ServerFnError::new(format!("Search failed: {}", e)))?;

        info!("SERVER: BM25 search returned {} results", results.len());
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

#[server(SearchDocumentsDynamic, "/api")]
pub async fn search_documents_dynamic(
    request: crate::domain::dtos::DynamicQueryRequest,
) -> Result<Vec<SearchResult>, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;
    use crate::infra::query_compiler::QueryCompiler;
    use tracing::info;

    let query = request.query.clone();
    let limit = request.limit;
    let _bm25_weight = request.bm25_weight;
    let _vector_weight = request.vector_weight;

    info!("========== DYNAMIC QUERY SEARCH ==========");
    info!("Query: {:?}, Filters: {:?}", query, request.filters);

    // Extract the AppState from context
    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    // Check if we have any search criteria
    let has_query = query.as_ref().map_or(false, |q| !q.trim().is_empty());
    let has_filters = request.filters.is_some();

    if !has_query && !has_filters {
        info!("SERVER: No query or filters provided, returning empty results");
        return Ok(Vec::new());
    }

    // Compile filter condition to SQL WHERE clause
    let where_clause = if let Some(filter_condition) = &request.filters {
        let compiled = QueryCompiler::compile_where_clause(filter_condition);
        info!("Compiled WHERE clause: {}", compiled);
        compiled
    } else {
        String::new()
    };

    // Use the dynamic_search function from db
    let results = db::dynamic_search(
        &state.pool,
        query.as_deref(),
        None, // No embedding for now, text-only search
        &where_clause,
        limit,
    )
    .await
    .map_err(|e| ServerFnError::new(format!("Search failed: {}", e)))?;

    info!("SERVER: Dynamic search returned {} results", results.len());
    Ok(results)
}
