pub use crate::domain::models::{SearchResult, SortOrder};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Response containing search results and metadata for query explanation
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
    /// The actual BM25 query that was executed (after transformation)
    pub executed_query: String,
    /// Whether the query was detected as field-qualified
    pub was_field_qualified: bool,
    /// Number of results returned in this page
    pub result_count: usize,
    /// Total matching documents (for pagination)
    pub total_count: i64,
    /// Current page (0-indexed)
    pub page: i32,
    /// Page size
    pub page_size: i32,
    /// Search duration in milliseconds
    pub duration_ms: u128,
}

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
    pub offset: i32,              // Pagination offset
    pub sort: SortOrder,          // Sort order
    pub search_fields: Vec<String>,
    pub bm25_weight: f64,
    pub vector_weight: f64,
    pub category_id: Option<Uuid>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
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
            offset: 0,
            sort: SortOrder::Relevance,
            search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
            bm25_weight: 0.5,
            vector_weight: 0.5,
            category_id: None,
            date_from: None,
            date_to: None,
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
pub async fn search_documents(request: SearchRequest) -> Result<SearchResponse, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;
    use std::time::Instant;
    use tracing::info;

    let start_time = Instant::now();
    let query = request.query;
    let limit = request.limit;
    let offset = request.offset;
    let sort = request.sort.clone();
    let search_fields = request.search_fields;

    info!("========== SEARCH REQUEST ==========");
    info!("Query: '{}', Fields: {:?}, Limit: {}, Offset: {}, Sort: {:?}",
          query, search_fields, limit, offset, sort);

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

    // Parse dates from YYYY-MM-DD format to DateTime<Utc>
    let date_from = request.date_from.as_ref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .map(|nd| nd.and_hms_opt(0, 0, 0).unwrap().and_utc())
    });
    let date_to = request.date_to.as_ref().and_then(|d| {
        chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
            .ok()
            .map(|nd| nd.and_hms_opt(23, 59, 59).unwrap().and_utc())
    });

    let db_filters = db::SearchFilters {
        category_id: request.category_id,
        date_from,
        date_to,
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
    let (results, total_count, executed_query, was_field_qualified) = if has_query {
        // Use the optimized BM25 search (searches documents table directly)
        let results = db::bm25_search(&state.pool, &query, &db_filters, limit, offset, &sort)
            .await
            .map_err(|e| ServerFnError::new(format!("Search failed: {}", e)))?;

        let total_count = db::bm25_search_count(&state.pool, &query, &db_filters)
            .await
            .map_err(|e| ServerFnError::new(format!("Count failed: {}", e)))?;

        info!("SERVER: BM25 search returned {} of {} total results", results.len(), total_count);
        (results, total_count, query.clone(), false)
    } else if has_filters || trimmed_query == "*" {
        // Filter-only or browse all documents (query = "*")
        let results = db::filter_only_search(&state.pool, &db_filters, limit, offset, &sort)
            .await
            .map_err(|e| ServerFnError::new(format!("Search failed: {}", e)))?;

        let total_count = db::filter_only_search_count(&state.pool, &db_filters)
            .await
            .map_err(|e| ServerFnError::new(format!("Count failed: {}", e)))?;

        info!("SERVER: Filter-only search returned {} of {} total results", results.len(), total_count);
        (results, total_count, "*".to_string(), false)
    } else {
        info!("SERVER: No query or filters provided, returning empty results");
        return Ok(SearchResponse {
            results: Vec::new(),
            executed_query: String::new(),
            was_field_qualified: false,
            result_count: 0,
            total_count: 0,
            page: 0,
            page_size: limit,
            duration_ms: start_time.elapsed().as_millis(),
        });
    };

    let duration_ms = start_time.elapsed().as_millis();
    let result_count = results.len();
    let page = offset / limit;

    Ok(SearchResponse {
        results,
        executed_query,
        was_field_qualified,
        result_count,
        total_count,
        page,
        page_size: limit,
        duration_ms,
    })
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
