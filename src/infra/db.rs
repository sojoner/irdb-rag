//! Database operations for RAG Chat
//!
//! Handles PostgreSQL connections and hybrid search queries using
//! ParadeDB's pg_search (BM25) and pgvector.

use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use crate::domain::models::{Document, DocumentChunk, DocumentAsset, Category, SearchResult};

/// Get embedding dimensions from environment (required)
/// This must match the embedding model's output dimension
fn get_embedding_dimensions() -> u32 {
    std::env::var("EMBEDDING_DIMENSIONS")
        .expect("EMBEDDING_DIMENSIONS environment variable must be set")
        .parse::<u32>()
        .expect("EMBEDDING_DIMENSIONS must be a valid u32")
}

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

    let dims = get_embedding_dimensions();
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

    // Apply entity and word count filters by fetching full documents and filtering in-memory
    if filters.authors.is_some()
        || filters.concepts.is_some()
        || filters.organizations.is_some()
        || filters.persons.is_some()
        || filters.products.is_some()
        || filters.word_count_min.is_some()
        || filters.word_count_max.is_some() {

        // Get full documents for filtering
        let result_ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        let docs = get_documents_by_ids(pool, &result_ids).await?;

        results.retain(|r| {
            if let Some(doc) = docs.iter().find(|d| d.id == r.id) {
                // Check author filter
                if let Some(ref authors) = filters.authors {
                    if let Some(ref author) = doc.author {
                        if !authors.iter().any(|a| a == author) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }

                // Check word count filter
                if let Some(min) = filters.word_count_min {
                    if let Some(wc) = doc.word_count {
                        if wc < min {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                if let Some(max) = filters.word_count_max {
                    if let Some(wc) = doc.word_count {
                        if wc > max {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }

                // Check entity filters (concepts, organizations, persons, products)
                if let Some(ref entities_obj) = doc.entities {
                    if let Some(ref concepts) = filters.concepts {
                        if let Some(entity_concepts) = entities_obj.get("concepts").and_then(|v| v.as_array()) {
                            let entity_strs: Vec<String> = entity_concepts.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect();
                            if !concepts.iter().any(|c| entity_strs.contains(c)) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }

                    if let Some(ref orgs) = filters.organizations {
                        if let Some(entity_orgs) = entities_obj.get("organizations").and_then(|v| v.as_array()) {
                            let entity_strs: Vec<String> = entity_orgs.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect();
                            if !orgs.iter().any(|o| entity_strs.contains(o)) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }

                    if let Some(ref persons) = filters.persons {
                        if let Some(entity_persons) = entities_obj.get("persons").and_then(|v| v.as_array()) {
                            let entity_strs: Vec<String> = entity_persons.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect();
                            if !persons.iter().any(|p| entity_strs.contains(p)) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }

                    if let Some(ref products) = filters.products {
                        if let Some(entity_products) = entities_obj.get("products").and_then(|v| v.as_array()) {
                            let entity_strs: Vec<String> = entity_products.iter()
                                .filter_map(|v| v.as_str())
                                .map(|s| s.to_string())
                                .collect();
                            if !products.iter().any(|pr| entity_strs.contains(pr)) {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                }

                true
            } else {
                true // Keep results that we couldn't fetch full documents for
            }
        });
    }

    // Truncate to requested limit
    results.truncate(limit as usize);

    Ok(results)
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

    let dims = get_embedding_dimensions();
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

    let dims = get_embedding_dimensions();
    let sql = format!(
        r#"
        INSERT INTO documents (
            title, content, source_path, source_type, embedding,
            summary, keywords, entities, author, category_id, word_count, status, indexed_at
        )
        VALUES ($1, $2, $3, $4, $5::vector({}), $6, $7, $8, $9, $10, $11, 'indexed', NOW())
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

    let dims = get_embedding_dimensions();
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
    let embedding_str = format!(
        "[{}]",
        embedding.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
    );

    let dims = get_embedding_dimensions();
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
