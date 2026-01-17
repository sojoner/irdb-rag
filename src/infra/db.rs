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
        matches_entity_filter(doc, &filters.concepts, "concepts"),
        matches_entity_filter(doc, &filters.organizations, "organizations"),
        matches_entity_filter(doc, &filters.persons, "persons"),
        matches_entity_filter(doc, &filters.products, "products"),
        matches_array_filter(&doc.locations, &filters.locations, "locations"),
        matches_array_filter(&doc.keywords, &filters.keywords, "keywords"),
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

/// Check array field filter (locations, keywords) - pure predicate
fn matches_array_filter(doc_array: &Option<Vec<String>>, filter: &Option<Vec<String>>, _field_name: &str) -> bool {
    let Some(filter_vals) = filter else { return true; };

    doc_array
        .as_ref()
        .map(|arr| {
            arr.iter()
                .any(|s| filter_vals.iter().any(|fv| fv == s))
        })
        .unwrap_or(false)
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

/// Perform advanced hybrid search combining multiple strategies for better F-measure
///
/// This improved search combines:
/// 1. Phrase matching (exact sequences, high weight)
/// 2. Boolean AND matching (all terms required, high precision)
/// 3. BM25 full-text search (standard lexical matching)
/// 4. Prefix/fuzzy matching (handles typos and partial matches)
/// 5. Vector semantic search (contextual similarity, lower weight)
///
/// Weights can be tuned based on corpus characteristics:
/// - More technical docs: increase bm25_weight to 0.7
/// - More conversational: increase vector_weight to 0.4
#[allow(clippy::too_many_arguments)]
pub async fn hybrid_search(
    pool: &PgPool,
    query: &str,
    embedding: &[f32],
    filters: &SearchFilters,
    limit: i32,
    bm25_weight: f64,
    vector_weight: f64,
    reranker: Option<&std::sync::Arc<crate::infra::reranker::Reranker>>,
) -> Result<Vec<SearchResult>> {
    // Build enhanced queries using improved tokenization
    let tokens = crate::infra::db_utils::tokenize_query(query);
    let phrase_query = crate::infra::db_utils::build_phrase_query(&tokens);
    let prefix_query = crate::infra::db_utils::build_prefix_query(query);
    let boolean_query = crate::infra::db_utils::build_boolean_query(query);
    let sanitized_query = crate::infra::db_utils::sanitize_bm25_query(query);

    // Convert embedding to PostgreSQL vector format
    let embedding_str = embedding_to_string(embedding);

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        WITH phrase_results AS (
            -- Phrase matching: exact sequences get 2.0x boost
            SELECT
                d.id,
                2.0 * paradedb.score(d.id) AS phrase_score
            FROM documents d
            WHERE (d.content @@@ $13 OR d.title @@@ $13)
            AND (d.status = 'indexed')
            LIMIT $3 * 4
        ),
        bm25_results AS (
            -- Full BM25 search: standard lexical matching
            SELECT
                d.id,
                ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as bm25_rank
            FROM documents d
            WHERE d.id @@@ $1
            AND (d.status = 'indexed')
            LIMIT $3 * 3
        ),
        boolean_results AS (
            -- Boolean AND matching: all terms required for precision
            SELECT
                d.id,
                1.5 * paradedb.score(d.id) AS boolean_score
            FROM documents d
            WHERE d.content @@@ $14 AND d.id @@@ $1
            AND (d.status = 'indexed')
            LIMIT $3 * 3
        ),
        prefix_results AS (
            -- Prefix/fuzzy matching: flexibility with wildcards
            SELECT
                d.id,
                ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as prefix_rank
            FROM documents d
            WHERE (d.content @@@ $15 OR d.title @@@ $15)
            AND (d.status = 'indexed')
            LIMIT $3 * 2
        ),
        vector_results AS (
            -- Vector semantic search: contextual similarity
            SELECT
                d.id,
                ROW_NUMBER() OVER (ORDER BY d.embedding <=> $2::vector({})) as vector_rank
            FROM documents d
            WHERE d.embedding IS NOT NULL
            AND (d.status = 'indexed')
            ORDER BY d.embedding <=> $2::vector({})
            LIMIT $3 * 3
        ),
        all_results AS (
            SELECT DISTINCT
                COALESCE(ph.id, b.id, bo.id, pr.id, v.id) AS result_id,
                ph.phrase_score,
                COALESCE(1.0 / (60 + b.bm25_rank), 0.0) AS bm25_score,
                bo.boolean_score,
                COALESCE(1.0 / (60 + pr.prefix_rank), 0.0) AS prefix_score,
                COALESCE(1.0 / (60 + v.vector_rank), 0.0) AS vector_score
            FROM phrase_results ph
            FULL OUTER JOIN bm25_results b ON ph.id = b.id
            FULL OUTER JOIN boolean_results bo ON COALESCE(ph.id, b.id) = bo.id
            FULL OUTER JOIN prefix_results pr ON COALESCE(ph.id, b.id, bo.id) = pr.id
            FULL OUTER JOIN vector_results v ON COALESCE(ph.id, b.id, bo.id, pr.id) = v.id
        )
        SELECT
            d.id,
            d.title,
            d.content,
            d.source_path,
            c.name as category_name,
            COALESCE(ar.bm25_score, 0.0)::FLOAT as bm25_score,
            COALESCE(ar.vector_score, 0.0)::FLOAT as vector_score,
            (
                COALESCE(ar.phrase_score, 0.0) * 0.15 +
                COALESCE(ar.bm25_score, 0.0) * $4 +
                COALESCE(ar.boolean_score, 0.0) * 0.15 +
                COALESCE(ar.prefix_score, 0.0) * 0.05 +
                COALESCE(ar.vector_score, 0.0) * $5
            )::FLOAT as combined_score,
            NULL::FLOAT as reranker_score
        FROM all_results ar
        JOIN documents d ON ar.result_id = d.id
        LEFT JOIN categories c ON d.category_id = c.id
        WHERE
            ($6::UUID IS NULL OR d.category_id = $6)
            AND ($7::TIMESTAMPTZ IS NULL OR d.created_at >= $7)
            AND ($8::TIMESTAMPTZ IS NULL OR d.created_at <= $8)
            AND ($9::TEXT[] IS NULL OR d.locations && $9)
            AND ($10::TEXT[] IS NULL OR d.keywords && $10)
        ORDER BY combined_score DESC
        LIMIT $3
        "#,
        dims, dims
    );

    let mut results = sqlx::query_as::<_, SearchResult>(&sql)
        .bind(sanitized_query)
        .bind(&embedding_str)
        .bind(limit * 3) // Fetch more for post-filtering
        .bind(bm25_weight)
        .bind(vector_weight)
        .bind(filters.category_id)
        .bind(filters.date_from)
        .bind(filters.date_to)
        .bind(&filters.locations)
        .bind(&filters.keywords)
        .bind("") // placeholder for additional filtering
        .bind("") // placeholder
        .bind(&phrase_query) // $13: phrase query
        .bind(&boolean_query) // $14: boolean query
        .bind(&prefix_query) // $15: prefix query
        .fetch_all(pool)
        .await?;

    tracing::debug!(
        "Hybrid search: phrase_query='{}', boolean_query='{}', prefix_query='{}'",
        phrase_query, boolean_query, prefix_query
    );

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

    // Apply reranking if enabled
    if let Some(reranker) = reranker {
        let documents: Vec<&str> = results.iter().map(|r| r.content.as_str()).collect();

        match reranker.rerank_batch(query, &documents).await {
            Ok(scores) => {
                for (result, score) in results.iter_mut().zip(scores.iter()) {
                    result.reranker_score = Some(*score);
                    // Blend: 70% reranker + 30% hybrid
                    result.combined_score = 0.7 * score + 0.3 * result.combined_score;
                }

                // Re-sort by new combined score
                results.sort_by(|a, b| {
                    b.combined_score.partial_cmp(&a.combined_score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });

                tracing::debug!("Reranking complete: {} results reranked", results.len());
            }
            Err(e) => {
                tracing::warn!("Reranking failed, using original scores: {}", e);
            }
        }
    }

    // Truncate to requested limit
    Ok(results.into_iter().take(limit as usize).collect())
}

/// Get documents by list of IDs
pub async fn get_documents_by_ids(pool: &PgPool, ids: &[Uuid]) -> Result<Vec<Document>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let docs = sqlx::query_as::<_, Document>(
        r#"
        SELECT id, title, content, source_path, source_type, summary, author,
               category_id, keywords, locations, created_at, status,
               entities, metadata, embedding::FLOAT4[] as embedding, content_hash
        FROM documents
        WHERE id = ANY($1)
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
        SELECT
            id, title, content, source_path, source_type, summary, author,
            category_id, keywords, locations, created_at, status, entities,
            metadata, content_hash, embedding::FLOAT4[] as embedding
        FROM documents
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
    pub locations: Option<Vec<String>>,
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

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        INSERT INTO documents (
            title, content, source_path, source_type, embedding,
            summary, keywords, locations, entities, author, category_id, metadata, content_hash, status
        )
        VALUES ($1, $2, $3, $4, $5::vector({}), $6, $7, $8, $9, $10, $11, $12, $13, 'indexed')
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
        .bind(&params.locations)
        .bind(&params.entities)
        .bind(params.author)
        .bind(params.category_id)
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

pub struct InsertChunkParams {
    pub chunk_index: i32,
    pub content: String,
    pub embedding: Vec<f32>,
    pub page_number: Option<i32>,
}

/// Bulk insert multiple chunks in a single query for better performance
pub async fn insert_chunks_batch(
    pool: &PgPool,
    document_id: Uuid,
    chunks: Vec<InsertChunkParams>,
) -> Result<()> {
    if chunks.is_empty() {
        return Ok(());
    }

    let dims = get_embedding_dimensions()?;
    
    let mut chunk_indices = Vec::with_capacity(chunks.len());
    let mut contents = Vec::with_capacity(chunks.len());
    let mut embeddings = Vec::with_capacity(chunks.len());
    let mut page_numbers = Vec::with_capacity(chunks.len());
    let mut token_counts = Vec::with_capacity(chunks.len());

    for chunk in chunks {
        chunk_indices.push(chunk.chunk_index);
        contents.push(chunk.content.clone());
        embeddings.push(embedding_to_string(&chunk.embedding));
        page_numbers.push(chunk.page_number);
        token_counts.push((chunk.content.len() / 4) as i32);
    }

    // Use UNNEST for bulk insert with vector casting
    let sql = format!(
        r#"
        INSERT INTO document_chunks (document_id, chunk_index, content, embedding, page_number, token_count)
        SELECT $1, t.chunk_index, t.content, t.embedding::vector({}), t.page_number, t.token_count
        FROM UNNEST($2::INTEGER[], $3::TEXT[], $4::TEXT[], $5::INTEGER[], $6::INTEGER[]) 
        AS t(chunk_index, content, embedding, page_number, token_count)
        "#,
        dims
    );

    sqlx::query(&sql)
        .bind(document_id)
        .bind(&chunk_indices)
        .bind(&contents)
        .bind(&embeddings)
        .bind(&page_numbers)
        .bind(&token_counts)
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn get_document(pool: &PgPool, id: Uuid) -> Result<Option<Document>> {
    let doc = sqlx::query_as::<_, Document>(
        r#"
        SELECT
            id, title, content, source_path, source_type, summary, author,
            category_id, keywords, locations, created_at, status, entities,
            metadata, content_hash, embedding::FLOAT4[] as embedding
        FROM documents WHERE id = $1
        "#
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

/// Insert a single document asset (image, figure, etc.)
pub async fn insert_asset(
    pool: &PgPool,
    document_id: Uuid,
    asset_type: &str,
    page_number: Option<i32>,
    alt_text: Option<&str>,
    caption: Option<&str>,
    metadata: Option<&serde_json::Value>,
) -> Result<Uuid> {
    let asset_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO document_assets (id, document_id, asset_type, page_number, alt_text, caption, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#
    )
    .bind(Uuid::new_v4())
    .bind(document_id)
    .bind(asset_type)
    .bind(page_number)
    .bind(alt_text)
    .bind(caption)
    .bind(metadata)
    .fetch_one(pool)
    .await?;

    Ok(asset_id)
}

/// Insert multiple document assets in a batch
pub async fn insert_assets_batch(
    pool: &PgPool,
    document_id: Uuid,
    assets: &[(String, Option<i32>, Option<String>, Option<String>)], // (asset_type, page_number, alt_text, caption)
) -> Result<usize> {
    if assets.is_empty() {
        return Ok(0);
    }

    let mut inserted = 0;
    for (asset_type, page_number, alt_text, caption) in assets {
        insert_asset(
            pool,
            document_id,
            asset_type,
            *page_number,
            alt_text.as_deref(),
            caption.as_deref(),
            None,
        )
        .await?;
        inserted += 1;
    }

    Ok(inserted)
}

pub async fn list_documents(
    pool: &PgPool,
    limit: i32,
    offset: i32,
) -> Result<Vec<Document>> {
    let docs = sqlx::query_as::<_, Document>(
        r#"
        SELECT
            id, title, content, source_path, source_type, summary, author,
            category_id, keywords, locations, created_at, status, entities,
            metadata, content_hash, embedding::FLOAT4[] as embedding
        FROM documents ORDER BY created_at DESC LIMIT $1 OFFSET $2
        "#
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(docs)
}

/// Filter-only search: returns documents matching filters without any text/semantic search
pub async fn filter_only_search(
    pool: &PgPool,
    filters: &SearchFilters,
    limit: i32,
) -> Result<Vec<SearchResult>> {
    // Build SQL with category filter - always use $1 for category, $2 for limit
    let category_clause = if filters.category_id.is_some() {
        "AND d.category_id = $1"
    } else {
        ""
    };

    let limit_param = if filters.category_id.is_some() { "$2" } else { "$1" };

    let sql = format!(
        r#"
        SELECT
            d.id,
            d.title,
            d.content,
            d.source_path,
            c.name as category_name,
            0.0::FLOAT as bm25_score,
            0.0::FLOAT as vector_score,
            0.0::FLOAT as combined_score,
            NULL::FLOAT as reranker_score
        FROM documents d
        LEFT JOIN categories c ON d.category_id = c.id
        WHERE d.status = 'indexed'
        {}
        ORDER BY d.created_at DESC
        LIMIT {}
        "#,
        category_clause, limit_param
    );

    let mut query = sqlx::query_as::<_, SearchResult>(&sql);

    // Bind category_id if present
    if let Some(cat_id) = filters.category_id {
        query = query.bind(cat_id);
    }
    query = query.bind(limit);

    let mut results = query.fetch_all(pool).await?;

    // Apply in-memory entity filters (concepts, organizations, persons, etc.)
    // Fetch full documents for filter matching
    if !results.is_empty() {
        let doc_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let full_docs = get_documents_by_ids(pool, &doc_ids).await?;

        results.retain(|result| {
            full_docs
                .iter()
                .find(|doc| doc.id == result.id)
                .map(|doc| matches_all_filters(doc, filters))
                .unwrap_or(false)
        });
    }

    Ok(results)
}

pub async fn list_categories(pool: &PgPool) -> Result<Vec<Category>> {
    let categories = sqlx::query_as::<_, Category>(
        "SELECT * FROM categories ORDER BY name"
    )
    .fetch_all(pool)
    .await?;

    Ok(categories)
}

/// Get or create a category by name
/// If the category doesn't exist, it will be created
pub async fn get_or_create_category(pool: &PgPool, name: &str) -> Result<Uuid> {
    // First, try to get existing category
    let existing = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM categories WHERE LOWER(name) = LOWER($1)"
    )
    .bind(name)
    .fetch_optional(pool)
    .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    // Category doesn't exist, create it
    let new_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO categories (id, name, description) VALUES ($1, $2, NULL) RETURNING id"
    )
    .bind(Uuid::new_v4())
    .bind(name)
    .fetch_one(pool)
    .await?;

    Ok(new_id)
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

    // Word count ranges are not available (column removed from schema)
    // Return empty vector for compatibility with AggregationStats
    let word_count_rows: Vec<(String, i64)> = Vec::new();

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
        None, // No reranker for internal document search
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
               category_id, keywords, locations, created_at, status,
               entities, metadata, embedding::FLOAT4[] as embedding, content_hash
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
            metadata: None,
            content_hash: None,
            status: "indexed".to_string(),
            created_at: Utc::now(),
            embedding: None,
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
        let doc = create_test_document(Uuid::new_v4(), Some("John"), None);
        let mut filters = create_test_filters();
        filters.authors = Some(vec!["John".to_string()]);

        assert!(matches_author_filter(&doc, &filters.authors));
    }

    #[test]
    fn test_matches_author_filter_no_match() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), None);
        let mut filters = create_test_filters();
        filters.authors = Some(vec!["Jane".to_string()]);

        assert!(!matches_author_filter(&doc, &filters.authors));
    }

    #[test]
    fn test_matches_author_filter_no_filter() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), None);
        assert!(matches_author_filter(&doc, &None));
    }

    #[test]
    fn test_matches_entity_filter_found() {
        let entities = json!({
            "concepts": ["AI", "Machine Learning"]
        });
        let doc = create_test_document(Uuid::new_v4(), None, Some(entities));
        let filter = Some(vec!["AI".to_string()]);

        assert!(matches_entity_filter(&doc, &filter, "concepts"));
    }

    #[test]
    fn test_matches_entity_filter_not_found() {
        let entities = json!({
            "concepts": ["AI", "Machine Learning"]
        });
        let doc = create_test_document(Uuid::new_v4(), None, Some(entities));
        let filter = Some(vec!["Blockchain".to_string()]);

        assert!(!matches_entity_filter(&doc, &filter, "concepts"));
    }

    #[test]
    fn test_matches_entity_filter_no_filter() {
        let doc = create_test_document(Uuid::new_v4(), None, None);
        assert!(matches_entity_filter(&doc, &None, "concepts"));
    }

    #[test]
    fn test_matches_entity_filter_no_entities() {
        let doc = create_test_document(Uuid::new_v4(), None, None);
        let filter = Some(vec!["AI".to_string()]);
        assert!(!matches_entity_filter(&doc, &filter, "concepts"));
    }

    #[test]
    fn test_matches_all_filters_all_pass() {
        let entities = json!({"concepts": ["AI"]});
        let doc = create_test_document(Uuid::new_v4(), Some("John"), Some(entities));

        let mut filters = create_test_filters();
        filters.authors = Some(vec!["John".to_string()]);
        filters.concepts = Some(vec!["AI".to_string()]);

        assert!(matches_all_filters(&doc, &filters));
    }

    #[test]
    fn test_matches_all_filters_author_fails() {
        let doc = create_test_document(Uuid::new_v4(), Some("John"), None);

        let mut filters = create_test_filters();
        filters.authors = Some(vec!["Jane".to_string()]);

        assert!(!matches_all_filters(&doc, &filters));
    }
}
