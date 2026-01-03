//! Database operations for RAG Chat
//! 
//! Handles PostgreSQL connections and hybrid search queries using
//! ParadeDB's pg_search (BM25) and pgvector.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, FromRow};
use uuid::Uuid;

/// Create a database connection pool
pub async fn create_pool() -> Result<PgPool> {
    use std::time::Duration;

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string());

    // Log connection attempt (masking password for security)
    let masked_url = if let Some(start) = database_url.find("://") {
        if let Some(end) = database_url[start+3..].find('@') {
            format!("{}://****@{}", &database_url[..start], &database_url[start+3+end+1..])
        } else {
            "postgres://****@...".to_string()
        }
    } else {
        "postgres://****@...".to_string()
    };
    tracing::info!("Connecting to database at {}", masked_url);

    // Get pool configuration from environment or use defaults
    let max_connections = std::env::var("DB_MAX_CONNECTIONS")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(20); // Increased from 10 to handle concurrent indexing

    let acquire_timeout = std::env::var("DB_ACQUIRE_TIMEOUT_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30);

    let pool = PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(acquire_timeout))
        .idle_timeout(Some(Duration::from_secs(600))) // 10 minutes
        .max_lifetime(Some(Duration::from_secs(1800))) // 30 minutes
        .connect(&database_url)
        .await?;

    tracing::info!(
        "Connected to database (max_connections: {}, acquire_timeout: {}s)",
        max_connections,
        acquire_timeout
    );
    Ok(pool)
}

// ============================================
// Data Models
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Document {
    pub id: Uuid,
    pub title: String,
    pub source_path: Option<String>,
    pub source_type: String,
    pub content: String,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub category_id: Option<Uuid>,
    pub keywords: Option<Vec<String>>,
    pub locations: Option<Vec<String>>,
    pub created_at: DateTime<Utc>,
    pub word_count: Option<i32>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DocumentChunk {
    pub id: Uuid,
    pub document_id: Uuid,
    pub chunk_index: i32,
    pub content: String,
    pub page_number: Option<i32>,
    pub section_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DocumentAsset {
    pub id: Uuid,
    pub document_id: Uuid,
    pub asset_type: String,
    pub page_number: Option<i32>,
    pub alt_text: Option<String>,
    pub caption: Option<String>,
    pub content_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Category {
    pub id: Uuid,
    pub name: String,
    pub parent_id: Option<Uuid>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SearchResult {
    pub id: Uuid,
    pub title: String,
    pub content: String,
    pub source_path: Option<String>,
    pub category_name: Option<String>,
    pub bm25_score: f64,
    pub vector_score: f64,
    pub combined_score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilters {
    pub category_id: Option<Uuid>,
    pub date_from: Option<DateTime<Utc>>,
    pub date_to: Option<DateTime<Utc>>,
    pub locations: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub source_types: Option<Vec<String>>,
}

// ============================================
// Hybrid Search
// ============================================

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
    // Sanitize query to handle wildcards and internal ParadeDB query representations
    // that cause parsing errors. "*" seems to be converted to "id:(*)" internally.
    let trimmed = query.trim();
    
    // Check for empty ID queries like "id:()", "id: ()", "id:(*)"
    let is_empty_id = if let Some(stripped) = trimmed.strip_prefix("id:") {
        let rest = stripped.trim();
        if rest.starts_with('(') && rest.ends_with(')') {
            let inside = rest[1..rest.len()-1].trim();
            inside.is_empty() || inside == "*" || inside == "**"
        } else {
            false
        }
    } else {
        false
    };

    let sanitized_query = if trimmed.is_empty() || trimmed == "*" || is_empty_id {
        "id:__no_match__"
    } else {
        query
    };

    // Convert embedding to PostgreSQL vector format
    let embedding_str = format!(
        "[{}]",
        embedding.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    );
    
    let results = sqlx::query_as::<_, SearchResult>(
        r#"
        SELECT * FROM hybrid_search(
            $1::TEXT,
            $2::vector(1024),
            $3::INTEGER,
            $4::FLOAT,
            $5::FLOAT,
            $6::UUID,
            $7::TIMESTAMPTZ,
            $8::TIMESTAMPTZ,
            $9::TEXT[],
            $10::TEXT[]
        )
        "#
    )
    .bind(sanitized_query)
    .bind(&embedding_str)
    .bind(limit)
    .bind(bm25_weight)
    .bind(vector_weight)
    .bind(filters.category_id)
    .bind(filters.date_from)
    .bind(filters.date_to)
    .bind(&filters.locations)
    .bind(&filters.keywords)
    .fetch_all(pool)
    .await?;

    Ok(results)
}

/// Simple BM25-only search
#[allow(dead_code)]
pub async fn bm25_search(
    pool: &PgPool,
    query: &str,
    limit: i32,
) -> Result<Vec<Document>> {
    let trimmed = query.trim();
    
    // Check for empty ID queries like "id:()", "id: ()", "id:(*)"
    let is_empty_id = if let Some(stripped) = trimmed.strip_prefix("id:") {
        let rest = stripped.trim();
        if rest.starts_with('(') && rest.ends_with(')') {
            let inside = rest[1..rest.len()-1].trim();
            inside.is_empty() || inside == "*" || inside == "**"
        } else {
            false
        }
    } else {
        false
    };

    let sanitized_query = if trimmed.is_empty() || trimmed == "*" || is_empty_id {
        "id:__no_match__"
    } else {
        query
    };

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
    let embedding_str = format!(
        "[{}]",
        embedding.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    );

    let results = sqlx::query_as::<_, Document>(
        r#"
        SELECT * FROM documents
        WHERE embedding IS NOT NULL
        ORDER BY embedding <=> $1::vector(1024)
        LIMIT $2
        "#
    )
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
}

pub async fn insert_document(
    pool: &PgPool,
    params: InsertDocumentParams<'_>,
) -> Result<Uuid> {
    let embedding_str = format!(
        "[{}]",
        params.embedding.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    );

    let word_count = params.content.split_whitespace().count() as i32;

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO documents (
            title, content, source_path, source_type, embedding,
            summary, keywords, entities, author, word_count, status, indexed_at
        )
        VALUES ($1, $2, $3, $4, $5::vector(1024), $6, $7, $8, $9, $10, 'indexed', NOW())
        RETURNING id
        "#
    )
    .bind(params.title)
    .bind(params.content)
    .bind(params.source_path)
    .bind(params.source_type)
    .bind(&embedding_str)
    .bind(params.summary)
    .bind(&params.keywords)
    .bind(&params.entities)
    .bind(params.author)
    .bind(word_count)
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
    let embedding_str = format!(
        "[{}]",
        embedding.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    );

    // Estimate token count (rough approximation: ~4 chars per token)
    let token_count = (content.len() / 4) as i32;

    let id = sqlx::query_scalar::<_, Uuid>(
        r#"
        INSERT INTO document_chunks (document_id, chunk_index, content, embedding, page_number, token_count)
        VALUES ($1, $2, $3, $4::vector(1024), $5, $6)
        RETURNING id
        "#
    )
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
    let embedding_str = format!(
        "[{}]",
        embedding.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    );

    let chunks = sqlx::query_as::<_, DocumentChunk>(
        r#"
        SELECT id, document_id, chunk_index, content, page_number, section_title
        FROM document_chunks
        WHERE embedding IS NOT NULL
        AND ($3::UUID[] IS NULL OR document_id = ANY($3))
        ORDER BY embedding <=> $1::vector(1024)
        LIMIT $2
        "#
    )
    .bind(&embedding_str)
    .bind(limit)
    .bind(document_ids)
    .fetch_all(pool)
    .await?;

    Ok(chunks)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DbStats {
    pub document_count: i64,
    pub chunk_count: i64,
}

pub async fn get_db_stats(pool: &PgPool) -> Result<DbStats> {
    let stats = sqlx::query_as::<_, DbStats>(
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
