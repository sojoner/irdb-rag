//! Database operations for RAG Chat
//!
//! Handles PostgreSQL connections and hybrid search queries using
//! ParadeDB's pg_search (BM25) and pgvector.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use crate::config::DatabaseConfig;
use crate::domain::models::{
    Category, Document, DocumentAsset, DocumentChunk, ImportItem, ImportJob, SearchResult,
};
use crate::infra::db_utils::{
    embedding_to_string, extract_unique_ids, has_entity_or_wordcount_filters,
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
        if let Some(end) = config.url[start + 3..].find('@') {
            format!(
                "{}://****@{}",
                &config.url[..start],
                &config.url[start + 3 + end + 1..]
            )
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
    authors
        .as_ref()
        .map(|filter_authors| {
            doc.author
                .as_ref()
                .is_some_and(|author| filter_authors.iter().any(|a| a == author))
        })
        .unwrap_or(true)
}

/// Check array field filter (locations, keywords) - pure predicate
fn matches_array_filter(
    doc_array: &Option<Vec<String>>,
    filter: &Option<Vec<String>>,
    _field_name: &str,
) -> bool {
    let Some(filter_vals) = filter else {
        return true;
    };

    doc_array
        .as_ref()
        .map(|arr| arr.iter().any(|s| filter_vals.iter().any(|fv| fv == s)))
        .unwrap_or(false)
}

/// Check single entity filter - pure predicate
fn matches_entity_filter(doc: &Document, filter: &Option<Vec<String>>, entity_key: &str) -> bool {
    let Some(filter_vals) = filter else {
        return true;
    };

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

/// Fast BM25-only search for instant results (no embedding required)
/// Searches across document_chunks using multiple BM25 strategies
pub async fn fast_bm25_search(
    pool: &PgPool,
    query: &str,
    filters: &SearchFilters,
    limit: i32,
) -> Result<Vec<SearchResult>> {
    let tokens = crate::infra::db_utils::tokenize_query(query);

    // Build content-only queries for chunks
    let chunk_phrase_query = if tokens.is_empty() {
        "id:__no_match__".to_string()
    } else {
        let quoted = tokens.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(" ");
        format!("content:({})", quoted)
    };

    let chunk_boolean_query = if tokens.is_empty() {
        "id:__no_match__".to_string()
    } else {
        format!("content:({})", tokens.join(" &&& "))
    };

    let chunk_prefix_query = if tokens.is_empty() {
        "id:__no_match__".to_string()
    } else {
        let prefix_terms = tokens.iter().map(|t| format!("{}*", t)).collect::<Vec<_>>().join(" ||| ");
        format!("content:({})", prefix_terms)
    };

    let chunk_sanitized = if query.trim().is_empty() {
        "id:__no_match__".to_string()
    } else {
        format!("content:({})", query.trim())
    };

    tracing::info!("=== FAST BM25 SEARCH ===");
    tracing::info!("Query: '{}'", query);

    let sql = r#"
        WITH phrase_results AS (
            SELECT
                dc.document_id AS id,
                2.0 * paradedb.score(dc.id) AS phrase_score
            FROM document_chunks dc
            WHERE dc.id @@@ $1
            LIMIT $2 * 4
        ),
        bm25_results AS (
            SELECT
                dc.document_id AS id,
                ROW_NUMBER() OVER (ORDER BY paradedb.score(dc.id) DESC) as bm25_rank
            FROM document_chunks dc
            WHERE dc.id @@@ $3
            LIMIT $2 * 3
        ),
        boolean_results AS (
            SELECT
                dc.document_id AS id,
                1.5 * paradedb.score(dc.id) AS boolean_score
            FROM document_chunks dc
            WHERE dc.id @@@ $4
            LIMIT $2 * 3
        ),
        prefix_results AS (
            SELECT
                dc.document_id AS id,
                ROW_NUMBER() OVER (ORDER BY paradedb.score(dc.id) DESC) as prefix_rank
            FROM document_chunks dc
            WHERE dc.id @@@ $5
            LIMIT $2 * 2
        ),
        combined_ids AS (
            SELECT DISTINCT id FROM phrase_results
            UNION
            SELECT DISTINCT id FROM bm25_results
            UNION
            SELECT DISTINCT id FROM boolean_results
            UNION
            SELECT DISTINCT id FROM prefix_results
        )
        SELECT
            d.id,
            d.title,
            d.content,
            d.source_path,
            c.name as category_name,
            COALESCE(p.phrase_score, 0.0)::FLOAT8 as bm25_score,
            0.0::FLOAT8 as vector_score,
            ((1.0 / (60 + COALESCE(b.bm25_rank, 1000))) +
            (COALESCE(p.phrase_score, 0.0) * 0.3) +
            (COALESCE(bool.boolean_score, 0.0) * 0.2) +
            (1.0 / (60 + COALESCE(pr.prefix_rank, 1000)) * 0.5))::FLOAT8 as combined_score,
            NULL::FLOAT8 as reranker_score,
            CASE
                WHEN d.content @@@ $3 THEN paradedb.snippet(d.content, start_tag => '<mark>', end_tag => '</mark>', max_num_chars => 300)
                ELSE NULL
            END as snippet
        FROM documents d
        JOIN combined_ids ci ON d.id = ci.id
        LEFT JOIN categories c ON d.category_id = c.id
        LEFT JOIN phrase_results p ON d.id = p.id
        LEFT JOIN bm25_results b ON d.id = b.id
        LEFT JOIN boolean_results bool ON d.id = bool.id
        LEFT JOIN prefix_results pr ON d.id = pr.id
        WHERE ($6::UUID IS NULL OR d.category_id = $6)
          AND ($7::TIMESTAMPTZ IS NULL OR d.created_at >= $7)
          AND ($8::TIMESTAMPTZ IS NULL OR d.created_at <= $8)
          AND ($9::TEXT[] IS NULL OR d.locations && $9)
          AND ($10::TEXT[] IS NULL OR d.keywords && $10)
        ORDER BY combined_score DESC
        LIMIT $2
        "#;

    let mut results = sqlx::query_as::<_, SearchResult>(sql)
        .bind(&chunk_phrase_query)
        .bind(limit * 3)
        .bind(&chunk_sanitized)
        .bind(&chunk_boolean_query)
        .bind(&chunk_prefix_query)
        .bind(filters.category_id)
        .bind(filters.date_from)
        .bind(filters.date_to)
        .bind(&filters.locations)
        .bind(&filters.keywords)
        .fetch_all(pool)
        .await?;

    // Apply entity and word count filters if needed
    if has_entity_or_wordcount_filters(filters) {
        let result_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let unique_ids = extract_unique_ids(&result_ids);

        if !unique_ids.is_empty() {
            let documents = get_documents_by_ids(pool, &unique_ids).await?;

            results.retain(|result| {
                documents.iter().any(|doc| {
                    doc.id == result.id
                        && matches_all_filters(doc, filters)
                })
            });
        }
    }

    tracing::info!("Fast BM25 search returned {} results", results.len());
    Ok(results.into_iter().take(limit as usize).collect())
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

    // For document_chunks: simple content-only queries
    let chunk_phrase_query = if tokens.is_empty() {
        "id:__no_match__".to_string()
    } else {
        let quoted = tokens.iter().map(|t| format!("\"{}\"", t)).collect::<Vec<_>>().join(" ");
        format!("content:({})", quoted)
    };

    let chunk_boolean_query = if tokens.is_empty() {
        "id:__no_match__".to_string()
    } else {
        format!("content:({})", tokens.join(" &&& "))
    };

    let chunk_prefix_query = if tokens.is_empty() {
        "id:__no_match__".to_string()
    } else {
        let prefix_terms = tokens.iter().map(|t| format!("{}*", t)).collect::<Vec<_>>().join(" ||| ");
        format!("content:({})", prefix_terms)
    };

    let chunk_sanitized = if query.trim().is_empty() {
        "id:__no_match__".to_string()
    } else {
        format!("content:({})", query.trim())
    };

    // Log query building details
    tracing::info!("=== HYBRID SEARCH QUERY BUILDING ===");
    tracing::info!("Original query: '{}'", query);
    tracing::info!("Tokenized: {:?}", tokens);
    tracing::info!("Chunk phrase: {}", chunk_phrase_query);
    tracing::info!("Chunk boolean: {}", chunk_boolean_query);
    tracing::info!("Chunk prefix: {}", chunk_prefix_query);
    tracing::info!("Chunk sanitized: {}", chunk_sanitized);
    tracing::info!(
        "Search weights - BM25: {}, Vector: {}",
        bm25_weight,
        vector_weight
    );
    tracing::info!("=====================================");

    // Convert embedding to PostgreSQL vector format
    let embedding_str = embedding_to_string(embedding);

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        WITH phrase_results AS (
            -- Phrase matching: exact sequences get 2.0x boost
            SELECT
                dc.document_id AS id,
                2.0 * paradedb.score(dc.id) AS phrase_score
            FROM document_chunks dc
            WHERE dc.id @@@ $13
            LIMIT $3 * 4
        ),
        bm25_results AS (
            -- Full BM25 search: standard lexical matching
            SELECT
                dc.document_id AS id,
                ROW_NUMBER() OVER (ORDER BY paradedb.score(dc.id) DESC) as bm25_rank
            FROM document_chunks dc
            WHERE dc.id @@@ $1
            LIMIT $3 * 3
        ),
        boolean_results AS (
            -- Boolean AND matching: all terms required for precision
            SELECT
                dc.document_id AS id,
                1.5 * paradedb.score(dc.id) AS boolean_score
            FROM document_chunks dc
            WHERE dc.id @@@ $14
            LIMIT $3 * 3
        ),
        prefix_results AS (
            -- Prefix/fuzzy matching: flexibility with wildcards
            SELECT
                dc.document_id AS id,
                ROW_NUMBER() OVER (ORDER BY paradedb.score(dc.id) DESC) as prefix_rank
            FROM document_chunks dc
            WHERE dc.id @@@ $15
            LIMIT $3 * 2
        ),
        vector_results AS (
            -- Vector semantic search: contextual similarity
            SELECT
                d.id,
                ROW_NUMBER() OVER (ORDER BY dc.embedding <=> $2::vector({})) as vector_rank
            FROM documents d
            JOIN document_chunks dc ON d.id = dc.document_id
            WHERE dc.embedding IS NOT NULL
            ORDER BY dc.embedding <=> $2::vector({})
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
            LEAST(1.0, (
                COALESCE(ar.phrase_score, 0.0) * 0.15 +
                COALESCE(ar.bm25_score, 0.0) * $4 +
                COALESCE(ar.boolean_score, 0.0) * 0.15 +
                COALESCE(ar.prefix_score, 0.0) * 0.05 +
                COALESCE(ar.vector_score, 0.0) * $5
            ))::FLOAT as combined_score,
            NULL::FLOAT as reranker_score,
            CASE
                WHEN d.content @@@ $1 THEN paradedb.snippet(d.content, start_tag => '<mark>', end_tag => '</mark>', max_num_chars => 300)
                ELSE NULL
            END as snippet
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
        .bind(&chunk_sanitized) // $1: BM25 query for chunks
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
        .bind(&chunk_phrase_query) // $13: phrase query for chunks
        .bind(&chunk_boolean_query) // $14: boolean query for chunks
        .bind(&chunk_prefix_query) // $15: prefix query for chunks
        .fetch_all(pool)
        .await?;

    tracing::debug!(
        "Hybrid search: phrase='{}', boolean='{}', prefix='{}'",
        chunk_phrase_query,
        chunk_boolean_query,
        chunk_prefix_query
    );

    // Apply entity and word count filters using functional composition
    if has_entity_or_wordcount_filters(filters) {
        let result_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let docs = get_documents_by_ids(pool, &result_ids).await?;
        let doc_map: std::collections::HashMap<Uuid, &Document> =
            docs.iter().map(|d| (d.id, d)).collect();

        results.retain(|r| {
            doc_map
                .get(&r.id)
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
                    b.combined_score
                        .partial_cmp(&a.combined_score)
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
        "#,
    )
    .bind(ids)
    .fetch_all(pool)
    .await?;

    Ok(docs)
}

/// Fast BM25-only search for UI search interface
/// Searches documents table directly (564 docs vs 138K chunks) for 10x speed improvement
/// Uses multi-field BM25: content, title, summary
pub async fn bm25_search(
    pool: &PgPool,
    query: &str,
    filters: &SearchFilters,
    limit: i32,
) -> Result<Vec<SearchResult>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Multi-field query: search across content, title, and summary for best relevance
    let multi_field_query = format!(
        "(content:({}) OR title:({}) OR summary:({}))",
        query.trim(),
        query.trim(),
        query.trim()
    );

    tracing::info!("BM25 search (documents): query='{}', limit={}", query, limit);

    let mut results = sqlx::query_as::<_, SearchResult>(
        r#"
        SELECT
            d.id,
            d.title,
            LEFT(d.content, 300) as content,
            d.source_path,
            c.name as category_name,
            paradedb.score(d.id)::FLOAT as bm25_score,
            0.0::FLOAT as vector_score,
            paradedb.score(d.id)::FLOAT as combined_score,
            NULL::FLOAT as reranker_score,
            NULL as snippet
        FROM documents d
        LEFT JOIN categories c ON d.category_id = c.id
        WHERE d.id @@@ $1
            AND ($2::UUID IS NULL OR d.category_id = $2)
            AND ($3::TIMESTAMPTZ IS NULL OR d.created_at >= $3)
            AND ($4::TIMESTAMPTZ IS NULL OR d.created_at <= $4)
            AND ($5::TEXT[] IS NULL OR d.locations && $5)
            AND ($6::TEXT[] IS NULL OR d.keywords && $6)
        ORDER BY paradedb.score(d.id) DESC
        LIMIT $7
        "#,
    )
    .bind(&multi_field_query)
    .bind(filters.category_id)
    .bind(filters.date_from)
    .bind(filters.date_to)
    .bind(&filters.locations)
    .bind(&filters.keywords)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    // Apply entity and word count filters if needed
    if has_entity_or_wordcount_filters(filters) {
        let result_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let docs = get_documents_by_ids(pool, &result_ids).await?;
        let doc_map: std::collections::HashMap<Uuid, &Document> =
            docs.iter().map(|d| (d.id, d)).collect();

        results.retain(|r| {
            doc_map
                .get(&r.id)
                .map(|doc| matches_all_filters(doc, filters))
                .unwrap_or(true)
        });
    }

    // Normalize scores to 0-1 range based on max score in result set
    // This makes percentages meaningful (100% = best match in this query)
    if !results.is_empty() {
        let max_score = results.iter()
            .map(|r| r.combined_score)
            .fold(0.0f64, f64::max);

        if max_score > 0.0 {
            for result in &mut results {
                result.bm25_score = result.bm25_score / max_score;
                result.combined_score = result.combined_score / max_score;
            }
            tracing::debug!("Normalized BM25 scores by max_score: {}", max_score);
        }
    }

    tracing::info!("BM25 search completed: {} results", results.len());
    Ok(results)
}

/// Fast vector-only similarity search for chat/RAG interface
/// Uses HNSW index for fast semantic search and returns SearchResult format
pub async fn vector_search(
    pool: &PgPool,
    embedding: &[f32],
    filters: &SearchFilters,
    limit: i32,
) -> Result<Vec<SearchResult>> {
    let embedding_str = embedding_to_string(embedding);

    tracing::info!("Vector search: limit={}", limit);

    let dims = get_embedding_dimensions()?;
    let sql = format!(
        r#"
        WITH vector_results AS (
            SELECT
                d.id,
                1.0 - (dc.embedding <=> $1::vector({})) AS similarity,
                ROW_NUMBER() OVER (PARTITION BY d.id ORDER BY dc.embedding <=> $1::vector({}) ASC) as rank_per_doc
            FROM documents d
            JOIN document_chunks dc ON d.id = dc.document_id
            WHERE dc.embedding IS NOT NULL
            ORDER BY dc.embedding <=> $1::vector({})
            LIMIT $2 * 2
        )
        SELECT
            d.id,
            d.title,
            d.content,
            d.source_path,
            c.name as category_name,
            0.0::FLOAT as bm25_score,
            vr.similarity::FLOAT as vector_score,
            vr.similarity::FLOAT as combined_score,
            NULL::FLOAT as reranker_score,
            NULL as snippet
        FROM vector_results vr
        JOIN documents d ON vr.id = d.id
        LEFT JOIN categories c ON d.category_id = c.id
        WHERE vr.rank_per_doc = 1
            AND ($3::UUID IS NULL OR d.category_id = $3)
            AND ($4::TIMESTAMPTZ IS NULL OR d.created_at >= $4)
            AND ($5::TIMESTAMPTZ IS NULL OR d.created_at <= $5)
            AND ($6::TEXT[] IS NULL OR d.locations && $6)
            AND ($7::TEXT[] IS NULL OR d.keywords && $7)
        ORDER BY vr.similarity DESC
        LIMIT $2
        "#,
        dims, dims, dims
    );

    let results = sqlx::query_as::<_, SearchResult>(&sql)
        .bind(&embedding_str)
        .bind(limit)
        .bind(filters.category_id)
        .bind(filters.date_from)
        .bind(filters.date_to)
        .bind(&filters.locations)
        .bind(&filters.keywords)
        .fetch_all(pool)
        .await?;

    // Apply entity and word count filters if needed
    let mut results = results;
    if has_entity_or_wordcount_filters(filters) {
        let result_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let docs = get_documents_by_ids(pool, &result_ids).await?;
        let doc_map: std::collections::HashMap<Uuid, &Document> =
            docs.iter().map(|d| (d.id, d)).collect();

        results.retain(|r| {
            doc_map
                .get(&r.id)
                .map(|doc| matches_all_filters(doc, filters))
                .unwrap_or(true)
        });
    }

    tracing::info!("Vector search completed: {} results", results.len());
    Ok(results)
}

/// Dynamic search with structured filter conditions from query builder
/// Supports combinations of text search, vector search, and structured metadata filters
pub async fn dynamic_search(
    pool: &PgPool,
    query: Option<&str>,
    embedding: Option<&[f32]>,
    where_clause: &str,
    limit: i32,
    bm25_weight: f64,
    vector_weight: f64,
) -> Result<Vec<SearchResult>> {
    tracing::info!(
        "Dynamic search: has_query={}, has_embedding={}, limit={}",
        query.is_some(),
        embedding.is_some(),
        limit
    );

    if where_clause.is_empty() && query.is_none() && embedding.is_none() {
        tracing::warn!("Dynamic search: no search criteria provided");
        return Ok(Vec::new());
    }

    let dims = get_embedding_dimensions()?;

    // Build the main search SQL based on what we have
    let mut results = Vec::new();

    // If we have a text query, do BM25 search
    if let Some(q) = query {
        let tokens = crate::infra::db_utils::tokenize_query(q);
        let bm25_query = if tokens.is_empty() {
            "id:__no_match__".to_string()
        } else {
            format!("content:({})", tokens.join(" &&& "))
        };

        let filter_where = if where_clause.is_empty() {
            String::new()
        } else {
            format!("AND {}", where_clause)
        };

        let sql = format!(
            r#"
            SELECT
                d.id,
                d.title,
                d.content,
                d.source_path,
                c.name as category_name,
                paradedb.score(dc.id)::FLOAT as bm25_score,
                0.0::FLOAT as vector_score,
                paradedb.score(dc.id)::FLOAT as combined_score,
                NULL::FLOAT as reranker_score,
                NULL as snippet
            FROM document_chunks dc
            JOIN documents d ON dc.document_id = d.id
            LEFT JOIN categories c ON d.category_id = c.id
            WHERE dc.id @@@ $1
                {filter_where}
            ORDER BY paradedb.score(dc.id) DESC
            LIMIT $2
            "#
        );

        results = sqlx::query_as::<_, SearchResult>(&sql)
            .bind(&bm25_query)
            .bind(limit)
            .fetch_all(pool)
            .await?;
    }

    // If we have an embedding, do vector search and combine results
    if let Some(emb) = embedding {
        let embedding_str = embedding_to_string(emb);

        let filter_where = if where_clause.is_empty() {
            String::new()
        } else {
            format!("AND {}", where_clause)
        };

        let sql = format!(
            r#"
            WITH vector_results AS (
                SELECT
                    d.id,
                    1.0 - (dc.embedding <=> $1::vector({})) AS similarity,
                    ROW_NUMBER() OVER (PARTITION BY d.id ORDER BY dc.embedding <=> $1::vector({}) ASC) as rank_per_doc
                FROM documents d
                JOIN document_chunks dc ON d.id = dc.document_id
                WHERE dc.embedding IS NOT NULL
                    {filter_where}
                ORDER BY dc.embedding <=> $1::vector({})
                LIMIT $2 * 2
            )
            SELECT
                d.id,
                d.title,
                d.content,
                d.source_path,
                c.name as category_name,
                0.0::FLOAT as bm25_score,
                vr.similarity::FLOAT as vector_score,
                vr.similarity::FLOAT as combined_score,
                NULL::FLOAT as reranker_score,
                NULL as snippet
            FROM vector_results vr
            JOIN documents d ON vr.id = d.id
            LEFT JOIN categories c ON d.category_id = c.id
            WHERE vr.rank_per_doc = 1
            ORDER BY vr.similarity DESC
            LIMIT $2
            "#,
            dims, dims, dims
        );

        let vector_results = sqlx::query_as::<_, SearchResult>(&sql)
            .bind(&embedding_str)
            .bind(limit)
            .fetch_all(pool)
            .await?;

        // If we have both BM25 and vector results, prefer vector results
        // In a production system, you might implement RRF fusion here
        if !vector_results.is_empty() {
            results = vector_results;
        }
    } else if results.is_empty() && !where_clause.is_empty() {
        // If we only have filters, do a filter-only search
        let filter_where = format!("WHERE {}", where_clause);

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
                NULL::FLOAT as reranker_score,
                NULL as snippet
            FROM documents d
            LEFT JOIN categories c ON d.category_id = c.id
            {filter_where}
            LIMIT $1
            "#
        );

        results = sqlx::query_as::<_, SearchResult>(&sql)
            .bind(limit)
            .fetch_all(pool)
            .await?;
    }

    tracing::info!("Dynamic search completed: {} results", results.len());
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

pub async fn insert_document(pool: &PgPool, params: InsertDocumentParams<'_>) -> Result<Uuid> {
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
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(doc)
}

pub async fn get_document_assets(pool: &PgPool, document_id: Uuid) -> Result<Vec<DocumentAsset>> {
    let assets = sqlx::query_as::<_, DocumentAsset>(
        "SELECT * FROM document_assets WHERE document_id = $1 ORDER BY page_number, id",
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

pub async fn list_documents(pool: &PgPool, limit: i32, offset: i32) -> Result<Vec<Document>> {
    let docs = sqlx::query_as::<_, Document>(
        r#"
        SELECT
            id, title, content, source_path, source_type, summary, author,
            category_id, keywords, locations, created_at, status, entities,
            metadata, content_hash, embedding::FLOAT4[] as embedding
        FROM documents ORDER BY created_at DESC LIMIT $1 OFFSET $2
        "#,
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

    let limit_param = if filters.category_id.is_some() {
        "$2"
    } else {
        "$1"
    };

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
            NULL::FLOAT as reranker_score,
            CASE
                WHEN d.content IS NOT NULL THEN substring(d.content, 1, 300)
                ELSE NULL
            END as snippet
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
    let categories = sqlx::query_as::<_, Category>("SELECT * FROM categories ORDER BY name")
        .fetch_all(pool)
        .await?;

    Ok(categories)
}

/// Get or create a category by name
/// If the category doesn't exist, it will be created
pub async fn get_or_create_category(pool: &PgPool, name: &str) -> Result<Uuid> {
    // First, try to get existing category
    let existing =
        sqlx::query_scalar::<_, Uuid>("SELECT id FROM categories WHERE LOWER(name) = LOWER($1)")
            .bind(name)
            .fetch_optional(pool)
            .await?;

    if let Some(id) = existing {
        return Ok(id);
    }

    // Category doesn't exist, create it
    let new_id = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO categories (id, name, description) VALUES ($1, $2, NULL) RETURNING id",
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
        "#,
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

    tracing::info!(
        "Deleted document {}: {} rows affected",
        id,
        result.rows_affected()
    );
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
        "#,
    )
    .bind(source_path)
    .bind(content_hash)
    .fetch_optional(pool)
    .await?;

    Ok(existing.map(|(id,)| id))
}

/// Check if source_path already exists (for quick skip)
pub async fn document_exists_by_path(pool: &PgPool, source_path: &str) -> Result<bool> {
    let exists: (bool,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM documents WHERE source_path = $1)")
            .bind(source_path)
            .fetch_one(pool)
            .await?;

    Ok(exists.0)
}

/// Find document by source path
pub async fn find_document_by_path(pool: &PgPool, source_path: &str) -> Result<Option<Uuid>> {
    let existing: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM documents WHERE source_path = $1 LIMIT 1")
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
        "#,
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
        "#,
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
        "#,
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
        "#,
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
        "#,
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
// Faceted Search
// ============================================

/// Get all facet aggregations for the current search context
pub async fn get_facet_aggregations(
    pool: &PgPool,
    query: Option<&str>,
    filters: &SearchFilters,
) -> Result<Vec<crate::domain::dtos::FacetAggregate>> {
    let category_id = filters.category_id;
    let date_from = filters.date_from;
    let date_to = filters.date_to;
    let locations = filters.locations.clone();
    let keywords = filters.keywords.clone();
    let authors = filters.authors.clone();

    let facets = sqlx::query_as::<_, (String, String, i64)>(
        r#"
        SELECT facet_name, facet_value, count
        FROM get_facet_aggregations($1, $2::UUID, $3::TIMESTAMPTZ, $4::TIMESTAMPTZ, $5::TEXT[], $6::TEXT[], $7::TEXT[])
        ORDER BY facet_name, count DESC
        "#
    )
    .bind(query)
    .bind(category_id)
    .bind(date_from)
    .bind(date_to)
    .bind(locations.as_deref())
    .bind(keywords.as_deref())
    .bind(authors.as_deref())
    .fetch_all(pool)
    .await?;

    let aggregates = facets
        .into_iter()
        .map(
            |(facet_name, facet_value, count)| crate::domain::dtos::FacetAggregate {
                facet_name,
                facet_value,
                count,
            },
        )
        .collect();

    Ok(aggregates)
}

/// Get specific facet values with counts
pub async fn get_facet_values(
    pool: &PgPool,
    facet_type: &str,
    query: Option<&str>,
    filters: &SearchFilters,
    limit: i32,
) -> Result<Vec<crate::domain::dtos::FacetValue>> {
    let category_id = filters.category_id;
    let date_from = filters.date_from;
    let date_to = filters.date_to;
    let locations = filters.locations.clone();
    let keywords = filters.keywords.clone();
    let authors = filters.authors.clone();

    let values = sqlx::query_as::<_, (String, i64)>(
        r#"
        SELECT value, count
        FROM get_facet_values($1, $2, $3::UUID, $4::TIMESTAMPTZ, $5::TIMESTAMPTZ, $6::TEXT[], $7::TEXT[], $8::TEXT[], $9::INT)
        "#
    )
    .bind(facet_type)
    .bind(query)
    .bind(category_id)
    .bind(date_from)
    .bind(date_to)
    .bind(locations.as_deref())
    .bind(keywords.as_deref())
    .bind(authors.as_deref())
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let facet_values = values
        .into_iter()
        .map(|(value, count)| crate::domain::dtos::FacetValue { value, count })
        .collect();

    Ok(facet_values)
}

/// Execute search with faceted results
pub async fn search_with_facets(
    pool: &PgPool,
    query: &str,
    embedding: &[f32],
    filters: &SearchFilters,
    limit: i32,
    bm25_weight: f64,
    vector_weight: f64,
    facet_limit: i32,
    reranker: Option<&crate::infra::reranker::Reranker>,
) -> Result<(Vec<SearchResult>, Vec<crate::domain::dtos::FacetAggregate>)> {
    let embedding_str = embedding_to_string(embedding);

    let category_id = filters.category_id;
    let date_from = filters.date_from;
    let date_to = filters.date_to;
    let locations = filters.locations.clone();
    let keywords = filters.keywords.clone();
    let authors = filters.authors.clone();

    // Call the SQL function that returns both results and facets
    let raw_results = sqlx::query_as::<
        _,
        (
            String,         // result_type
            Option<Uuid>,   // id
            Option<String>, // title
            Option<String>, // content
            Option<String>, // source_path
            Option<String>, // category_name
            Option<f32>,    // bm25_score
            Option<f32>,    // vector_score
            Option<f32>,    // combined_score
            Option<String>, // snippet
            Option<String>, // facet_name
            Option<String>, // facet_value
            Option<i64>,    // facet_count
        ),
    >(
        r#"
        SELECT result_type, id, title, content, source_path, category_name,
               bm25_score, vector_score, combined_score, snippet,
               facet_name, facet_value, facet_count
        FROM search_with_facets($1, $2::VECTOR, $3::INT, $4::FLOAT, $5::FLOAT,
                                $6::UUID, $7::TIMESTAMPTZ, $8::TIMESTAMPTZ,
                                $9::TEXT[], $10::TEXT[], $11::TEXT[], $12::INT)
        "#,
    )
    .bind(&query)
    .bind(&embedding_str)
    .bind(limit)
    .bind(bm25_weight)
    .bind(vector_weight)
    .bind(category_id)
    .bind(date_from)
    .bind(date_to)
    .bind(locations.as_deref())
    .bind(keywords.as_deref())
    .bind(authors.as_deref())
    .bind(facet_limit)
    .fetch_all(pool)
    .await?;

    // Separate results and facets
    let mut search_results = Vec::new();
    let mut facets = Vec::new();

    for row in raw_results {
        match row.0.as_str() {
            "result" => {
                if let (Some(id), Some(title), Some(content), combined_score) =
                    (row.1, row.2, row.3, row.8)
                {
                    search_results.push(SearchResult {
                        id,
                        title,
                        content,
                        source_path: row.4,
                        category_name: row.5,
                        bm25_score: row.6.map(|v| v as f64).unwrap_or(0.0),
                        vector_score: row.7.map(|v| v as f64).unwrap_or(0.0),
                        combined_score: combined_score.map(|v| v as f64).unwrap_or(0.0),
                        reranker_score: None,
                        snippet: row.9,
                    });
                }
            }
            "facet" => {
                if let (Some(facet_name), Some(facet_value), Some(count)) = (row.10, row.11, row.12)
                {
                    facets.push(crate::domain::dtos::FacetAggregate {
                        facet_name,
                        facet_value,
                        count,
                    });
                }
            }
            _ => {}
        }
    }

    // Apply reranking if available
    if let Some(reranker) = reranker {
        let chunk_contents: Vec<&str> = search_results.iter().map(|r| r.content.as_str()).collect();

        if let Ok(ranked) = reranker.rerank_and_sort(query, &chunk_contents).await {
            let mut reranked = Vec::new();
            for doc in ranked {
                if let Some(result) = search_results.get(doc.index) {
                    reranked.push(result.clone());
                }
            }
            search_results = reranked;
        }
    }

    Ok((search_results, facets))
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
    let doc_ids_to_fetch: Vec<Uuid> = doc_ids.into_iter().take(limit as usize).collect();

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
        "#,
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
    let job = sqlx::query_as::<_, ImportJob>("SELECT * FROM import_jobs WHERE id = $1")
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
        "#,
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
    let total: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM import_items WHERE job_id = $1")
        .bind(job_id)
        .fetch_one(pool)
        .await?;

    let items = sqlx::query_as::<_, ImportItem>(
        r#"
        SELECT * FROM import_items
        WHERE job_id = $1
        ORDER BY created_at DESC
        LIMIT $2 OFFSET $3
        "#,
    )
    .bind(job_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((items, total.0))
}

/// Get failed or skipped items for a job (for retry)
pub async fn get_failed_items(pool: &PgPool, job_id: Uuid) -> Result<Vec<ImportItem>> {
    let items = sqlx::query_as::<_, ImportItem>(
        r#"
        SELECT * FROM import_items
        WHERE job_id = $1 AND status IN ('failed', 'skipped')
        ORDER BY created_at
        "#,
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
        "#,
    )
    .bind(job_id)
    .fetch_one(pool)
    .await?;

    Ok(stats)
}

/// Get a single import item by ID
pub async fn get_import_item(pool: &PgPool, item_id: Uuid) -> Result<Option<ImportItem>> {
    let item = sqlx::query_as::<_, ImportItem>("SELECT * FROM import_items WHERE id = $1")
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

    fn create_test_document(id: Uuid, author: Option<&str>, entities: Option<Value>) -> Document {
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

/// Simple BM25 search with field selection
/// Returns documents with pure BM25 scores (no hybrid complexity)
/// Scores are normalized to 0-1 range for consistent display
pub async fn simple_bm25_search(
    pool: &PgPool,
    query: &str,
    search_fields: &[&str], // e.g., ["content", "title", "summary", "author"]
    filters: &SearchFilters,
    limit: i32,
) -> Result<Vec<SearchResult>> {
    tracing::info!("=== SIMPLE BM25 SEARCH ===");
    tracing::info!("Query: '{}', Fields: {:?}", query, search_fields);

    if search_fields.is_empty() {
        tracing::warn!("No search fields selected, returning empty results");
        return Ok(Vec::new());
    }

    // Build field-qualified query: (content:(query) OR title:(query) OR ...)
    let field_queries: Vec<String> = search_fields
        .iter()
        .map(|field| format!("{}:({})", field, query))
        .collect();
    let bm25_query = field_queries.join(" OR ");

    tracing::debug!("BM25 Query: {}", bm25_query);

    // Simple query: just use ParadeDB BM25 score directly on documents table
    let sql = r#"
        SELECT
            d.id,
            d.title,
            d.content,
            d.source_path,
            c.name as category_name,
            paradedb.score(d.id)::FLOAT8 as bm25_score,
            0.0::FLOAT8 as vector_score,
            paradedb.score(d.id)::FLOAT8 as combined_score,
            NULL::FLOAT8 as reranker_score,
            paradedb.snippet(d.content, start_tag => '<mark>', end_tag => '</mark>', max_num_chars => 300) as snippet
        FROM documents d
        LEFT JOIN categories c ON d.category_id = c.id
        WHERE d.id @@@ $1
          AND ($2::UUID IS NULL OR d.category_id = $2)
          AND ($3::TIMESTAMPTZ IS NULL OR d.created_at >= $3)
          AND ($4::TIMESTAMPTZ IS NULL OR d.created_at <= $4)
          AND ($5::TEXT[] IS NULL OR d.locations && $5)
          AND ($6::TEXT[] IS NULL OR d.keywords && $6)
        ORDER BY paradedb.score(d.id) DESC
        LIMIT $7
        "#;

    let mut results = sqlx::query_as::<_, SearchResult>(sql)
        .bind(&bm25_query)
        .bind(filters.category_id)
        .bind(filters.date_from)
        .bind(filters.date_to)
        .bind(&filters.locations)
        .bind(&filters.keywords)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    // Apply entity and word count filters if needed
    if has_entity_or_wordcount_filters(filters) {
        let result_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let unique_ids = extract_unique_ids(&result_ids);

        if !unique_ids.is_empty() {
            let documents = get_documents_by_ids(pool, &unique_ids).await?;

            results.retain(|result| {
                documents.iter().any(|doc| {
                    doc.id == result.id && matches_all_filters(doc, filters)
                })
            });
        }
    }

    // Normalize scores to 0-1 range based on max score in result set
    // This makes percentages meaningful (100% = best match in this query)
    if !results.is_empty() {
        let max_score = results.iter()
            .map(|r| r.combined_score)
            .fold(0.0f64, f64::max);

        if max_score > 0.0 {
            for result in &mut results {
                result.bm25_score = result.bm25_score / max_score;
                result.combined_score = result.combined_score / max_score;
            }
            tracing::debug!("Normalized scores by max_score: {}", max_score);
        }
    }

    tracing::info!("Simple BM25 search returned {} results", results.len());
    Ok(results)
}

/// Pure vector similarity search for chat context retrieval
/// No BM25, no hybrid - just semantic similarity
pub async fn pure_vector_search(
    pool: &PgPool,
    query_embedding: &[f32],
    limit: i32,
    filters: &SearchFilters,
) -> Result<Vec<SearchResult>> {
    tracing::info!("=== PURE VECTOR SEARCH ===");
    tracing::info!("Limit: {}", limit);

    let sql = r#"
        SELECT
            d.id,
            d.title,
            d.content,
            d.source_path,
            c.name as category_name,
            0.0::FLOAT8 as bm25_score,
            (1.0 - (d.embedding <=> $1::vector))::FLOAT8 as vector_score,
            (1.0 - (d.embedding <=> $1::vector))::FLOAT8 as combined_score,
            NULL::FLOAT8 as reranker_score,
            substring(d.content, 1, 300) as snippet
        FROM documents d
        LEFT JOIN categories c ON d.category_id = c.id
        WHERE d.embedding IS NOT NULL
          AND ($2::UUID IS NULL OR d.category_id = $2)
          AND ($3::TEXT[] IS NULL OR d.locations && $3)
          AND ($4::TEXT[] IS NULL OR d.keywords && $4)
        ORDER BY d.embedding <=> $1::vector
        LIMIT $5
        "#;

    let results = sqlx::query_as::<_, SearchResult>(sql)
        .bind(query_embedding)
        .bind(filters.category_id)
        .bind(&filters.locations)
        .bind(&filters.keywords)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    tracing::info!("Pure vector search returned {} results", results.len());
    Ok(results)
}

// ============================================
// Conversation & Message Persistence
// ============================================

/// Create a new conversation
pub async fn create_conversation(pool: &PgPool, title: &str) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO conversations (id, title, created_at, updated_at)
        VALUES ($1, $2, NOW(), NOW())
        "#,
    )
    .bind(&id)
    .bind(title)
    .execute(pool)
    .await?;

    tracing::debug!("Created conversation: {} with title: {}", id, title);
    Ok(id)
}

/// Save a message to a conversation
pub async fn save_message(
    pool: &PgPool,
    conversation_id: Uuid,
    role: &str,
    content: &str,
) -> Result<Uuid> {
    let id = Uuid::new_v4();
    sqlx::query(
        r#"
        INSERT INTO messages (id, conversation_id, role, content, created_at)
        VALUES ($1, $2, $3, $4, NOW())
        "#,
    )
    .bind(&id)
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .execute(pool)
    .await?;

    tracing::debug!(
        "Saved message to conversation {}: role={}, content_len={}",
        conversation_id,
        role,
        content.len()
    );
    Ok(id)
}

/// Load all messages for a conversation
pub async fn load_conversation(
    pool: &PgPool,
    conversation_id: Uuid,
) -> Result<Vec<crate::domain::dtos::ChatMessage>> {
    let messages = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT role, content
        FROM messages
        WHERE conversation_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(conversation_id)
    .fetch_all(pool)
    .await?;

    let chat_messages: Vec<crate::domain::dtos::ChatMessage> = messages
        .into_iter()
        .map(|(role, content)| crate::domain::dtos::ChatMessage { role, content })
        .collect();

    tracing::debug!(
        "Loaded {} messages for conversation {}",
        chat_messages.len(),
        conversation_id
    );
    Ok(chat_messages)
}

/// Update conversation title
pub async fn update_conversation_title(
    pool: &PgPool,
    conversation_id: Uuid,
    title: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE conversations
        SET title = $1, updated_at = NOW()
        WHERE id = $2
        "#,
    )
    .bind(title)
    .bind(conversation_id)
    .execute(pool)
    .await?;

    tracing::debug!(
        "Updated conversation {} title to: {}",
        conversation_id,
        title
    );
    Ok(())
}

/// Get conversation by ID
pub async fn get_conversation(
    pool: &PgPool,
    conversation_id: Uuid,
) -> Result<Option<(Uuid, Option<String>, chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)>> {
    let row = sqlx::query_as::<_, (Uuid, Option<String>, DateTime<Utc>, DateTime<Utc>)>(
        r#"
        SELECT id, title, created_at, updated_at
        FROM conversations
        WHERE id = $1
        "#,
    )
    .bind(conversation_id)
    .fetch_optional(pool)
    .await?;

    Ok(row)
}
