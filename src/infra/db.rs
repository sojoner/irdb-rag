//! Database operations for RAG Chat
//!
//! Handles PostgreSQL connections and hybrid search queries using
//! ParadeDB's pg_search (BM25) and pgvector.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use crate::config::DatabaseConfig;
use crate::domain::models::{Document, DocumentChunk, DocumentAsset, Category, SearchResult, ImportJob, ImportItem};
use crate::infra::db_utils::{
    sanitize_bm25_query, embedding_to_string, has_entity_or_wordcount_filters,
    extract_unique_ids,
};

/// Get embedding dimensions from configuration
/// This is a helper function for database operations that need to know the vector dimension
pub fn get_embedding_dimensions() -> Result<u32> {
    let settings = crate::config::Settings::new()?;
    Ok(settings.embedding.dimensions)
}

/// Create a database connection pool
pub async fn create_pool(config: &DatabaseConfig) -> Result<PgPool> {
    use std::time::Duration;

    // Log connection attempt (masking password for security)
    let masked_url = if let Some(start) = config.url.find("://") {
        if let Some(end) = config.url[start+3..].find('@') {
            format!("{}://****@{}", &config.url[..start], &config.url[start+3+end+1..])
        } else {
            "postgres://****@...".to_string()
        }
    } else {
        "postgres://****@...".to_string()
    };
    tracing::info!("Connecting to database at {}", masked_url);

    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .acquire_timeout(Duration::from_secs(config.acquire_timeout_seconds))
        .idle_timeout(Some(Duration::from_secs(600))) // 10 minutes
        .max_lifetime(Some(Duration::from_secs(1800))) // 30 minutes
        .connect(&config.url)
        .await?;

    tracing::info!(
        "Connected to database (max_connections: {}, acquire_timeout: {}s)",
        config.max_connections,
        config.acquire_timeout_seconds
    );
    Ok(pool)
}

// ============================================
// Hybrid Search
// ============================================

#[derive(Debug, Clone)]
pub struct SearchFilters {
    pub category_id: Option<Uuid>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub locations: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub source_types: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub organizations: Option<Vec<String>>,
    pub persons: Option<Vec<String>>,
    pub products: Option<Vec<String>>,
    pub word_count_min: Option<i32>,
    pub word_count_max: Option<i32>,
}

/// Check if a document matches all active filters using functional composition
/// Pure function - composable predicate
fn matches_all_filters(doc: &Document, filters: &SearchFilters) -> bool {
    [
        matches_author_filter(doc, &filters.authors),
        matches_word_count_filter(doc, filters.word_count_min, filters.word_count_max),
        matches_entity_filter(doc, &filters.concepts, "concepts"),
        matches_entity_filter(doc, &filters.organizations, "organizations"),
        matches_entity_filter(doc, &filters.persons, "persons"),
        matches_entity_filter(doc, &filters.products, "products"),
    ]
    .iter()
    .all(|&predicate| predicate)
}

/// Check author filter - pure predicate
fn matches_author_filter(doc: &Document, authors: &Option<Vec<String>>) -> bool {
    authors.as_ref()
        .map(|filter_authors| {
            doc.author.as_ref()
                .is_some_and(|author| filter_authors.iter().any(|a| a == author))
        })
        .unwrap_or(true)
}

/// Check word count filter - pure predicate
fn matches_word_count_filter(doc: &Document, min: Option<i32>, max: Option<i32>) -> bool {
    let word_count = doc.word_count.unwrap_or(0);
    min.is_none_or(|m| word_count >= m) && max.is_none_or(|m| word_count <= m)
}

/// Check single entity filter - pure predicate
fn matches_entity_filter(doc: &Document, filter: &Option<Vec<String>>, entity_key: &str) -> bool {
    let Some(filter_vals) = filter else { return true; };

    doc.entities
        .as_ref()
        .and_then(|entities| entities.get(entity_key).and_then(|v| v.as_array()))
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .any(|s| filter_vals.iter().any(|fv| fv == s))
        })
        .unwrap_or(false)
}

/// Perform hybrid search combining BM25 and vector similarity
pub async fn hybrid_search(
    pool: &PgPool,
    query: &str,
    embedding: &[f32],
    filters: &SearchFilters,
    limit: i32,
    bm25_weight: f64,
    vector_weight: f64,
) -> Result<Vec<SearchResult>> {
    // Sanitize query using pure function
    let sanitized_query = sanitize_bm25_query(query);

    // Convert embedding to PostgreSQL vector format using pure function
    let embedding_str = embedding_to_string(embedding);

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        SELECT * FROM hybrid_search(
            $1::TEXT,
            $2::vector({}),
            $3::INTEGER,
            $4::FLOAT,
            $5::FLOAT,
            $6::UUID,
            $7::TIMESTAMPTZ,
            $8::TIMESTAMPTZ,
            $9::TEXT[],
            $10::TEXT[]
        )
        "#,
        dims
    );

    let mut results = sqlx::query_as::<_, SearchResult>(&sql)
        .bind(sanitized_query)
        .bind(&embedding_str)
        .bind(limit * 3) // Get more results for post-filtering
        .bind(bm25_weight)
        .bind(vector_weight)
        .bind(filters.category_id)
        .bind(filters.date_from)
        .bind(filters.date_to)
        .bind(&filters.locations)
        .bind(&filters.keywords)
        .fetch_all(pool)
        .await?;

    // Apply entity and word count filters using functional composition
    if has_entity_or_wordcount_filters(filters) {
        let result_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let docs = get_documents_by_ids(pool, &result_ids).await?;
        let doc_map: std::collections::HashMap<Uuid, &Document> =
            docs.iter().map(|d| (d.id, d)).collect();

        results.retain(|r| {
            doc_map.get(&r.id)
                .map(|doc| matches_all_filters(doc, filters))
                .unwrap_or(true)
        });
    }

    // Truncate to requested limit
    Ok(results.into_iter().take(limit as usize).collect())
}

/// Get documents by list of IDs
async fn get_documents_by_ids(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Document>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let docs = sqlx::query_as::<_, Document>(
        r#"
        SELECT d.* FROM documents d
        WHERE d.id = ANY($1)
        "#
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

    Ok(docs)
}

/// Simple BM25-only search
#[allow(dead_code)]
pub async fn bm25_search(
    pool: &PgPool,
    query: &str,
    limit: i32,
) -> Result<Vec<Document>> {
    let sanitized_query = sanitize_bm25_query(query);

    let results = sqlx::query_as::<_, Document>(
        r#"
        SELECT d.*
        FROM documents d
        WHERE d.id @@@ $1
        ORDER BY paradedb.score(d.id) DESC
        LIMIT $2
        "#
    )
    .bind(sanitized_query)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(results)
}

/// Vector-only similarity search
#[allow(dead_code)]
pub async fn vector_search(
    pool: &PgPool,
    embedding: &[f32],
    limit: i32,
) -> Result<Vec<Document>> {
    let embedding_str = embedding_to_string(embedding);

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        SELECT * FROM documents
        WHERE embedding IS NOT NULL
        ORDER BY embedding <=> $1::vector({})
        LIMIT $2
        "#,
        dims
    );

    let results = sqlx::query_as::<_, Document>(&sql)
        .bind(&embedding_str)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(results)
}

// ============================================
// CRUD Operations
// ============================================

pub struct InsertDocumentParams<'a> {
    pub title: &'a str,
    pub content: &'a str,
    pub source_path: Option<&'a str>,
    pub source_type: &'a str,
    pub embedding: &'a [f32],
    pub summary: Option<&'a str>,
    pub keywords: Option<Vec<String>>,
    pub entities: Option<serde_json::Value>,
    pub author: Option<&'a str>,
    pub category_id: Option<Uuid>,
    pub metadata: Option<serde_json::Value>,
    pub content_hash: Option<&'a str>,
}

pub async fn insert_document(
    pool: &PgPool,
    params: InsertDocumentParams<'_>,
) -> Result<Uuid> {
    let embedding_str = embedding_to_string(params.embedding);
    let word_count = params.content.split_whitespace().count() as i32;

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        INSERT INTO documents (
            title, content, source_path, source_type, embedding,
            summary, keywords, entities, author, category_id, word_count, metadata, content_hash, status, indexed_at
        )
        VALUES ($1, $2, $3, $4, $5::vector({}), $6, $7, $8, $9, $10, $11, $12, $13, 'indexed', NOW())
        RETURNING id
        "#,
        dims
    );

    let id = sqlx::query_scalar::<_, Uuid>(&sql)
        .bind(params.title)
        .bind(params.content)
        .bind(params.source_path)
        .bind(params.source_type)
        .bind(&embedding_str)
        .bind(params.summary)
        .bind(&params.keywords)
        .bind(&params.entities)
        .bind(params.author)
        .bind(params.category_id)
        .bind(word_count)
        .bind(&params.metadata)
        .bind(params.content_hash)
        .fetch_one(pool)
        .await?;

    Ok(id)
}

pub async fn insert_chunk(
    pool: &PgPool,
    document_id: Uuid,
    chunk_index: i32,
    content: &str,
    embedding: &[f32],
    page_number: Option<i32>,
) -> Result<Uuid> {
    let embedding_str = embedding_to_string(embedding);

    // Estimate token count (rough approximation: ~4 chars per token)
    let token_count = (content.len() / 4) as i32;

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        INSERT INTO document_chunks (document_id, chunk_index, content, embedding, page_number, token_count)
        VALUES ($1, $2, $3, $4::vector({}), $5, $6)
        RETURNING id
        "#,
        dims
    );

    let id = sqlx::query_scalar::<_, Uuid>(&sql)
        .bind(document_id)
        .bind(chunk_index)
        .bind(content)
        .bind(&embedding_str)
        .bind(page_number)
        .bind(token_count)
        .fetch_one(pool)
        .await?;

    Ok(id)
}

pub async fn get_document(pool: &PgPool, id: Uuid) -> Result<Option<Document>> {
    let doc = sqlx::query_as::<_, Document>(
        "SELECT * FROM documents WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(doc)
}

pub async fn get_document_assets(pool: &PgPool, document_id: Uuid) -> Result<Vec<DocumentAsset>> {
    let assets = sqlx::query_as::<_, DocumentAsset>(
        "SELECT * FROM document_assets WHERE document_id = $1 ORDER BY page_number, id"
    )
    .bind(document_id)
    .fetch_all(pool)
    .await?;

    Ok(assets)
}

pub async fn list_documents(
    pool: &PgPool,
    limit: i32,
    offset: i32,
) -> Result<Vec<Document>> {
    let docs = sqlx::query_as::<_, Document>(
        "SELECT * FROM documents ORDER BY created_at DESC LIMIT $1 OFFSET $2"
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(docs)
}

pub async fn list_categories(pool: &PgPool) -> Result<Vec<Category>> {
    let categories = sqlx::query_as::<_, Category>(
        "SELECT * FROM categories ORDER BY name"
    )
    .fetch_all(pool)
    .await?;

    Ok(categories)
}

/// Get chunks for context retrieval
pub async fn get_relevant_chunks(
    pool: &PgPool,
    embedding: &[f32],
    limit: i32,
    document_ids: Option<&[Uuid]>,
) -> Result<Vec<DocumentChunk>> {
    let embedding_str = embedding_to_string(embedding);

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        SELECT id, document_id, chunk_index, content, page_number, section_title
        FROM document_chunks
        WHERE embedding IS NOT NULL
        AND ($3::UUID[] IS NULL OR document_id = ANY($3))
        ORDER BY embedding <=> $1::vector({})
        LIMIT $2
        "#,
        dims
    );

    let chunks = sqlx::query_as::<_, DocumentChunk>(&sql)
        .bind(&embedding_str)
        .bind(limit)
        .bind(document_ids)
        .fetch_all(pool)
        .await?;

    Ok(chunks)
}

pub async fn get_db_stats(pool: &PgPool) -> Result<crate::domain::models::DbStats> {
    let stats = sqlx::query_as::<_, crate::domain::models::DbStats>(
        r#"
        SELECT
            (SELECT COUNT(*) FROM documents) as document_count,
            (SELECT COUNT(*) FROM document_chunks) as chunk_count
        "#
    )
    .fetch_one(pool)
    .await?;

    Ok(stats)
}

/// Delete a document and all related data (chunks, assets)
/// Returns the number of rows affected
pub async fn delete_document(pool: &PgPool, id: Uuid) -> Result<u64> {
    // Delete chunks (ON DELETE CASCADE handles this, but explicit is clearer)
    sqlx::query("DELETE FROM document_chunks WHERE document_id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    // Delete assets
    sqlx::query("DELETE FROM document_assets WHERE document_id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    // Delete the document itself
    let result = sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    tracing::info!("Deleted document {}: {} rows affected", id, result.rows_affected());
    Ok(result.rows_affected())
}

/// Bulk delete multiple documents
pub async fn delete_documents_batch(pool: &PgPool, ids: &[Uuid]) -> Result<u64> {
    if ids.is_empty() {
        return Ok(0);
    }

    // Delete chunks
    sqlx::query("DELETE FROM document_chunks WHERE document_id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;

    // Delete assets
    sqlx::query("DELETE FROM document_assets WHERE document_id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;

    // Delete documents
    let result = sqlx::query("DELETE FROM documents WHERE id = ANY($1)")
        .bind(ids)
        .execute(pool)
        .await?;

    tracing::info!("Bulk deleted {} documents", result.rows_affected());
    Ok(result.rows_affected())
}

/// Check if a document with the same source_path and content_hash already exists
/// Returns the existing document ID if found
pub async fn find_duplicate_document(
    pool: &PgPool,
    source_path: &str,
    content_hash: &str,
) -> Result<Option<Uuid>> {
    let existing: Option<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT id FROM documents
        WHERE source_path = $1 AND content_hash = $2
        LIMIT 1
        "#
    )
    .bind(source_path)
    .bind(content_hash)
    .fetch_optional(pool)
    .await?;

    Ok(existing.map(|(id,)| id))
}

/// Check if source_path already exists (for quick skip)
pub async fn document_exists_by_path(
    pool: &PgPool,
    source_path: &str,
) -> Result<bool> {
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS(SELECT 1 FROM documents WHERE source_path = $1)"
    )
    .bind(source_path)
    .fetch_one(pool)
    .await?;

    Ok(exists.0)
}

/// Find document by source path
pub async fn find_document_by_path(
    pool: &PgPool,
    source_path: &str,
) -> Result<Option<Uuid>> {
    let existing: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM documents WHERE source_path = $1 LIMIT 1"
    )
    .bind(source_path)
    .fetch_optional(pool)
    .await?;

    Ok(existing.map(|(id,)| id))
}

pub async fn get_aggregation_stats(pool: &PgPool) -> Result<crate::domain::dtos::AggregationStats> {
    // Get categories with counts
    let categories_rows = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT c.name, COUNT(d.id) as count
        FROM categories c
        LEFT JOIN documents d ON c.id = d.category_id
        GROUP BY c.id, c.name
        ORDER BY count DESC
        LIMIT 10
        "#
    )
    .fetch_all(pool)
    .await?;

    // Get keywords with counts
    let keywords_rows = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT keyword, COUNT(*) as count
        FROM (
            SELECT UNNEST(keywords) as keyword
            FROM documents
            WHERE keywords IS NOT NULL
        ) t
        GROUP BY keyword
        ORDER BY count DESC
        LIMIT 10
        "#
    )
    .fetch_all(pool)
    .await?;

    // Get locations with counts
    let locations_rows = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT location, COUNT(*) as count
        FROM (
            SELECT UNNEST(locations) as location
            FROM documents
            WHERE locations IS NOT NULL
        ) t
        GROUP BY location
        ORDER BY count DESC
        LIMIT 10
        "#
    )
    .fetch_all(pool)
    .await?;

    // Get entity counts from JSONB entities field
    let entities_rows = sqlx::query_as::<_, (String, String, i64)>(
        r#"
        SELECT
            entity_type,
            entity_value,
            COUNT(*) as count
        FROM (
            SELECT
                'persons' as entity_type,
                jsonb_array_elements(entities->'persons')::text as entity_value
            FROM documents
            WHERE entities->'persons' IS NOT NULL
            UNION ALL
            SELECT
                'organizations' as entity_type,
                jsonb_array_elements(entities->'organizations')::text as entity_value
            FROM documents
            WHERE entities->'organizations' IS NOT NULL
            UNION ALL
            SELECT
                'products' as entity_type,
                jsonb_array_elements(entities->'products')::text as entity_value
            FROM documents
            WHERE entities->'products' IS NOT NULL
            UNION ALL
            SELECT
                'concepts' as entity_type,
                jsonb_array_elements(entities->'concepts')::text as entity_value
            FROM documents
            WHERE entities->'concepts' IS NOT NULL
        ) t
        GROUP BY entity_type, entity_value
        ORDER BY entity_type, count DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    // Organize entities by type
    let mut persons = Vec::new();
    let mut organizations = Vec::new();
    let mut products = Vec::new();
    let mut concepts = Vec::new();

    for (entity_type, entity_value, count) in entities_rows {
        let cleaned_value = entity_value.trim_matches('"').to_string();
        match entity_type.as_str() {
            "persons" => persons.push((cleaned_value, count)),
            "organizations" => organizations.push((cleaned_value, count)),
            "products" => products.push((cleaned_value, count)),
            "concepts" => concepts.push((cleaned_value, count)),
            _ => {}
        }
    }

    // Limit to top 10 per entity type
    persons.truncate(10);
    organizations.truncate(10);
    products.truncate(10);
    concepts.truncate(10);

    // Get authors with counts
    let authors_rows = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT author, COUNT(*) as count
        FROM documents
        WHERE author IS NOT NULL AND author != ''
        GROUP BY author
        ORDER BY count DESC
        LIMIT 20
        "#
    )
    .fetch_all(pool)
    .await?;

    // Get word count ranges
    let word_count_rows = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT
            CASE
                WHEN word_count < 500 THEN 'Very Short (< 500 words)'
                WHEN word_count < 2000 THEN 'Short (500-2K words)'
                WHEN word_count < 5000 THEN 'Medium (2K-5K words)'
                WHEN word_count < 10000 THEN 'Long (5K-10K words)'
                ELSE 'Very Long (> 10K words)'
            END as range,
            COUNT(*) as count
        FROM documents
        WHERE word_count IS NOT NULL
        GROUP BY range
        ORDER BY COUNT(*) DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(crate::domain::dtos::AggregationStats {
        categories: categories_rows,
        keywords: keywords_rows,
        locations: locations_rows,
        persons,
        organizations,
        products,
        concepts,
        authors: authors_rows,
        word_count_ranges: word_count_rows,
    })
}

// ============================================
// Search/Display Separation
// ============================================

/// Search for documents using hybrid search, then return full documents
///
/// This separates search (chunks) from display (full documents):
/// 1. Search chunks for relevance using BM25 + vector similarity
/// 2. Extract unique document IDs from results
/// 3. Fetch full documents for display
///
/// Improves display quality by showing complete document context instead of chunks
pub async fn search_and_get_documents(
    pool: &PgPool,
    query: &str,
    embedding: &[f32],
    filters: &SearchFilters,
    limit: i32,
    bm25_weight: f64,
    vector_weight: f64,
) -> Result<Vec<Document>> {
    // Step 1: Search chunks to find relevant documents
    let search_results = hybrid_search(
        pool,
        query,
        embedding,
        filters,
        limit * 3, // Fetch more to get unique documents
        bm25_weight,
        vector_weight,
    )
    .await?;

    // Step 2: Extract unique document IDs, maintaining order by relevance using pure function
    let result_ids: Vec<Uuid> = search_results.iter().map(|r| r.id).collect();
    let doc_ids = extract_unique_ids(&result_ids);

    // Step 3: Fetch full documents (limit to requested count)
    let doc_ids_to_fetch: Vec<Uuid> = doc_ids
        .into_iter()
        .take(limit as usize)
        .collect();

    if doc_ids_to_fetch.is_empty() {
        return Ok(vec![]);
    }

    // Fetch full documents in order
    let documents = sqlx::query_as::<_, Document>(
        r#"
        SELECT id, title, content, source_path, source_type, summary, author,
               category_id, keywords, locations, created_at, word_count, status,
               entities, metadata
        FROM documents
        WHERE id = ANY($1)
        ORDER BY created_at DESC
        "#
    )
    .bind(&doc_ids_to_fetch)
    .fetch_all(pool)
    .await?;

    tracing::info!(
        "Search returned {} documents from {} search results",
        documents.len(),
        doc_ids_to_fetch.len()
    );

    Ok(documents)
}

// ============================================
// Import Operations
// ============================================

/// Get import job by ID
pub async fn get_import_job(pool: &PgPool, job_id: Uuid) -> Result<ImportJob> {
    let job = sqlx::query_as::<_, ImportJob>(
        "SELECT * FROM import_jobs WHERE id = $1"
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;

    Ok(job)
}

/// List import jobs with pagination
pub async fn list_import_jobs(
    pool: &PgPool,
    limit: i32,
    offset: i32,
) -> Result<(Vec<ImportJob>, i64)> {
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM import_jobs")
        .fetch_one(pool)
        .await?;

    let jobs = sqlx::query_as::<_, ImportJob>(
        r#"
        SELECT * FROM import_jobs
        ORDER BY created_at DESC
        LIMIT $1 OFFSET $2
        "#
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((jobs, total.0))
}

/// Get import items for a job with pagination
pub async fn get_import_items(
    pool: &PgPool,
    job_id: Uuid,
    limit: i32,
    offset: i32,
) -> Result<(Vec<ImportItem>, i64)> {
    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM import_items WHERE job_id = $1"
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;

    let items = sqlx::query_as::<_, ImportItem>(
        r#"
        SELECT * FROM import_items
        WHERE job_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#
    )
    .bind(job_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((items, total.0))
}

/// Get failed or skipped items for a job (for retry)
pub async fn get_failed_items(
    pool: &PgPool,
    job_id: Uuid,
) -> Result<Vec<ImportItem>> {
    let items = sqlx::query_as::<_, ImportItem>(
        r#"
        SELECT * FROM import_items
        WHERE job_id = $1 AND status IN ('failed', 'skipped')
        ORDER BY created_at
        "#
    )
    .bind(job_id)
    .fetch_all(pool)
    .await?;

    Ok(items)
}

/// Get job statistics
pub async fn get_import_job_stats(pool: &PgPool, job_id: Uuid) -> Result<(i32, i32, i32, i32)> {
    let stats: (i32, i32, i32, i32) = sqlx::query_as(
        r#"
        SELECT
            COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0),
            COALESCE(SUM(CASE WHEN status = 'skipped' THEN 1 ELSE 0 END), 0),
            COUNT(*)
        FROM import_items
        WHERE job_id = $1
        "#
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;

    Ok(stats)
}

/// Get a single import item by ID
pub async fn get_import_item(pool: &PgPool, item_id: Uuid) -> Result<Option<ImportItem>> {
    let item = sqlx::query_as::<_, ImportItem>(
        "SELECT * FROM import_items WHERE id = $1"
    )
    .bind(item_id)
    .fetch_optional(pool)
    .await?;

    Ok(item)
}

/// Delete a single import item by ID
pub async fn delete_import_item(pool: &PgPool, item_id: Uuid) -> Result<u64> {
    let result = sqlx::query("DELETE FROM import_items WHERE id = $1")
        .bind(item_id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn create_test_document(
        id: Uuid,
        author: Option<&str>,
        word_count: Option<i32>,
        entities: Option<Value>,
    ) -> Document {
        use chrono::Utc;
        Document {
            id,
            title: "Test Doc".to_string(),
            content: "Test content".to_string(),
            source_path: None,
            source_type: "test".to_string(),
            summary: None,
            keywords: None,
            locations: None,
            entities,
            author: author.map(|s| s.to_string()),
            category_id: None,
            word_count,
            metadata: None,
            content_hash: None,
            status: "indexed".to_string(),
            created_at: Utc::now(),
        }
    }

    fn create_test_filters() -> SearchFilters {
        SearchFilters {
            category_id: None,
            date_from: None,
            date_to: None,
            locations: None,
            keywords: None,
            source_types: None,
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: None,
            word_count_max: None,
        }
    }

    #[test]
    fn test_matches_author_filter_matches() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), None, None);
        let mut filters = create_test_filters();
        filters.authors = Some(vec!["John".to_string()]);

        assert!(matches_author_filter(&doc, &filters.authors));
    }

    #[test]
    fn test_matches_author_filter_no_match() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), None, None);
        let mut filters = create_test_filters();
        filters.authors = Some(vec!["Jane".to_string()]);

        assert!(!matches_author_filter(&doc, &filters.authors));
    }

    #[test]
    fn test_matches_author_filter_no_filter() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), None, None);
        assert!(matches_author_filter(&doc, &None));
    }

    #[test]
    fn test_matches_word_count_filter_both_constraints() {
        let doc = create_test_document(Uuid::new_v4(), None, Some(100), None);
        assert!(matches_word_count_filter(&doc, Some(50), Some(150)));
        assert!(!matches_word_count_filter(&doc, Some(150), Some(200)));
        assert!(!matches_word_count_filter(&doc, Some(50), Some(80)));
    }

    #[test]
    fn test_matches_word_count_filter_min_only() {
        let doc = create_test_document(Uuid::new_v4(), None, Some(100), None);
        assert!(matches_word_count_filter(&doc, Some(50), None));
        assert!(!matches_word_count_filter(&doc, Some(150), None));
    }

    #[test]
    fn test_matches_word_count_filter_no_constraints() {
        let doc = create_test_document(Uuid::new_v4(), None, Some(100), None);
        assert!(matches_word_count_filter(&doc, None, None));
    }

    #[test]
    fn test_matches_entity_filter_found() {
        let entities = json!({
            "concepts": ["AI", "Machine Learning"]
        });
        let doc = create_test_document(Uuid::new_v4(), None, None, Some(entities));
        let filter = Some(vec!["AI".to_string()]);

        assert!(matches_entity_filter(&doc, &filter, "concepts"));
    }

    #[test]
    fn test_matches_entity_filter_not_found() {
        let entities = json!({
            "concepts": ["AI", "Machine Learning"]
        });
        let doc = create_test_document(Uuid::new_v4(), None, None, Some(entities));
        let filter = Some(vec!["Blockchain".to_string()]);

        assert!(!matches_entity_filter(&doc, &filter, "concepts"));
    }

    #[test]
    fn test_matches_entity_filter_no_filter() {
        let doc = create_test_document(Uuid::new_v4(), None, None, None);
        assert!(matches_entity_filter(&doc, &None, "concepts"));
    }

    #[test]
    fn test_matches_entity_filter_no_entities() {
        let doc = create_test_document(Uuid::new_v4(), None, None, None);
        let filter = Some(vec!["AI".to_string()]);
        assert!(!matches_entity_filter(&doc, &filter, "concepts"));
    }

    #[test]
    fn test_matches_all_filters_all_pass() {
        let entities = json!({"concepts": ["AI"]});
        let doc = create_test_document(Uuid::new_v4(), Some("John"), Some(100), Some(entities));

        let mut filters = create_test_filters();
        filters.authors = Some(vec!["John".to_string()]);
        filters.word_count_min = Some(50);
        filters.word_count_max = Some(150);
        filters.concepts = Some(vec!["AI".to_string()]);

        assert!(matches_all_filters(&doc, &filters));
    }

    #[test]
    fn test_matches_all_filters_author_fails() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), Some(100), None);

        let mut filters = create_test_filters();
        filters.authors = Some(vec!["Jane".to_string()]);

        assert!(!matches_all_filters(&doc, &filters));
    }

    #[test]
    fn test_matches_all_filters_word_count_fails() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), Some(100), None);

        let mut filters = create_test_filters();
        filters.authors = Some(vec!["John".to_string()]);
        filters.word_count_min = Some(150);

        assert!(!matches_all_filters(&doc, &filters));
    }
}
