-- Enable extensions
CREATE EXTENSION IF NOT EXISTS vector;
DROP EXTENSION IF EXISTS pg_search CASCADE;
CREATE EXTENSION pg_search;

SET search_path TO public, paradedb;

-- Drop existing tables and functions to recreate with correct schema
DROP FUNCTION IF EXISTS hybrid_search(text,vector,integer,double precision,double precision,uuid,timestamp with time zone,timestamp with time zone,text[],text[]);
DROP TABLE IF EXISTS document_chunks CASCADE;
DROP TABLE IF EXISTS document_assets CASCADE;
DROP TABLE IF EXISTS documents CASCADE;
DROP TABLE IF EXISTS categories CASCADE;

-- Categories table (created first, referenced by documents)
CREATE TABLE IF NOT EXISTS categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    parent_id UUID REFERENCES categories(id)
);

-- Documents table
-- Note: embedding vector will store embeddings of configured dimension (768 for nomic-embed-text-v2-moe)
CREATE TABLE IF NOT EXISTS documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    source_path TEXT,
    source_type TEXT NOT NULL,
    summary TEXT,
    author TEXT,
    category_id UUID REFERENCES categories(id) ON DELETE SET NULL,
    keywords TEXT[],
    locations TEXT[],
    created_at TIMESTAMPTZ DEFAULT NOW(),
    status TEXT DEFAULT 'pending',
    entities JSONB,
    metadata JSONB,
    content_hash TEXT,
    embedding VECTOR(768) NULL
);

-- Document chunks
CREATE TABLE IF NOT EXISTS document_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    page_number INTEGER,
    section_title TEXT,
    token_count INTEGER,
    embedding VECTOR(768) NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Document assets
CREATE TABLE IF NOT EXISTS document_assets (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID REFERENCES documents(id) ON DELETE CASCADE,
    asset_type TEXT NOT NULL,
    page_number INTEGER,
    alt_text TEXT,
    caption TEXT,
    content_base64 TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Categories
-- Conversations (for chat history)
CREATE TABLE IF NOT EXISTS conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Messages
CREATE TABLE IF NOT EXISTS messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Import Jobs (for tracking batch import operations)
CREATE TABLE IF NOT EXISTS import_jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, running, completed, failed, cancelled
    source_type TEXT NOT NULL,               -- folder, url, file_upload
    source_path TEXT,
    total_items INTEGER DEFAULT 0,
    processed_items INTEGER DEFAULT 0,
    failed_items INTEGER DEFAULT 0,
    skipped_items INTEGER DEFAULT 0,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error_message TEXT
);

-- Import Items (individual files/URLs within a job)
CREATE TABLE IF NOT EXISTS import_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES import_jobs(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, processing, completed, failed, skipped
    retry_count INTEGER DEFAULT 0,
    error_message TEXT,
    error_type TEXT,                         -- transient, permanent
    document_id UUID REFERENCES documents(id) ON DELETE SET NULL,
    file_size_bytes BIGINT DEFAULT 0,        -- for prioritizing processing (smallest first)
    created_at TIMESTAMPTZ DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);

-- Indexes

-- BM25 Index on documents
-- ParadeDB full-text search index for hybrid search
-- Note: key_field='id' is required to link BM25 scores back to document rows
-- This enables field-qualified queries like: content:(query) OR title:(query) OR summary:(query)
--
-- The primary searchable fields indexed by ParadeDB BM25:
-- - content: Main document body text (highest relevance weight)
-- - title: Document title (medium relevance)
-- - summary: Document summary/abstract (medium relevance)
--
-- Additional filtering is handled in application code:
-- - author, keywords, locations: Filtered in Rust code (src/infra/db.rs)
-- - entities (concepts, persons, organizations, products): JSONB filtering
--
-- See HYBRID SEARCH STRATEGY section below for query building details
CREATE INDEX documents_search_idx ON documents USING bm25 (id, content, title, summary)
WITH (key_field='id');

-- HNSW Vector index (optional - created separately for performance)
-- Note: HNSW requires dimension specification, which depends on embedding model
-- Vector similarity is computed in application code as part of hybrid search

-- ============================================
-- HYBRID SEARCH STRATEGY
-- ============================================
-- The system implements 5-strategy hybrid search combining:
-- 1. Phrase matching: exact phrase sequences (15% weight, 2.0x boost)
-- 2. BM25 lexical: standard full-text relevance (60% weight)
-- 3. Boolean AND: all terms required (15% weight, 1.5x boost)
-- 4. Prefix/fuzzy: wildcard matching for typos (5% weight)
-- 5. Vector semantic: embedding similarity (10% weight by default)
--
-- Query building in Rust (src/infra/db_utils.rs):
-- - tokenize_query(): Normalize and filter single-char noise
-- - build_phrase_query(): Create "quoted phrase" searches across fields
-- - build_boolean_query(): Create AND-required term searches
-- - build_prefix_query(): Create wildcard term* searches
-- - sanitize_bm25_query(): Add field qualification for multi-field search
--
-- All queries use field-qualified syntax:
--   (content:(term) OR title:(term) OR summary:(term) OR author:(term) OR source_path:(term))
--
-- Results are combined with FULL OUTER JOIN and scored with Reciprocal Rank Fusion:
--   RRF(rank) = 1.0 / (60 + rank)
--   combined_score = Σ(weight_i * RRF_i)

-- Unique index for idempotency (source_path + content_hash)
CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_source_content_hash
ON documents (source_path, content_hash)
WHERE source_path IS NOT NULL AND content_hash IS NOT NULL;

-- Import Job Indexes
CREATE INDEX IF NOT EXISTS idx_import_jobs_status ON import_jobs(status);
CREATE INDEX IF NOT EXISTS idx_import_items_job_id ON import_items(job_id);
CREATE INDEX IF NOT EXISTS idx_import_items_status ON import_items(status);
CREATE INDEX IF NOT EXISTS idx_import_items_job_status_size ON import_items(job_id, status, file_size_bytes);

-- Hybrid Search Function
CREATE OR REPLACE FUNCTION hybrid_search(
  query_text TEXT,
  query_embedding VECTOR,
  match_count INT,
  bm25_weight FLOAT,
  vector_weight FLOAT,
  filter_category_id UUID DEFAULT NULL,
  filter_date_from TIMESTAMPTZ DEFAULT NULL,
  filter_date_to TIMESTAMPTZ DEFAULT NULL,
  filter_locations TEXT[] DEFAULT NULL,
  filter_keywords TEXT[] DEFAULT NULL
) RETURNS TABLE (
  id UUID,
  title TEXT,
  content TEXT,
  source_path TEXT,
  category_name TEXT,
  bm25_score FLOAT,
  vector_score FLOAT,
  combined_score FLOAT,
  reranker_score FLOAT
) AS $$
BEGIN
  RETURN QUERY
  WITH bm25_results AS (
    SELECT
      d.id,
      ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as rank
    FROM documents d
    WHERE d.id @@@ query_text
    LIMIT match_count * 2
  ),
  vector_results AS (
    SELECT
      d.id,
      ROW_NUMBER() OVER (ORDER BY d.embedding <=> query_embedding) as rank
    FROM documents d
    WHERE d.embedding IS NOT NULL
    ORDER BY d.embedding <=> query_embedding
    LIMIT match_count * 2
  )
  SELECT
    d.id,
    d.title,
    d.content,
    d.source_path,
    c.name as category_name,
    COALESCE(1.0 / (60 + b.rank), 0.0)::FLOAT as bm25_score,
    COALESCE(1.0 / (60 + v.rank), 0.0)::FLOAT as vector_score,
    LEAST(1.0, (
      COALESCE(bm25_weight * (1.0 / (60 + b.rank)), 0.0) +
      COALESCE(vector_weight * (1.0 / (60 + v.rank)), 0.0)
    ))::FLOAT as combined_score,
    NULL::FLOAT as reranker_score
  FROM documents d
  LEFT JOIN categories c ON d.category_id = c.id
  LEFT JOIN bm25_results b ON d.id = b.id
  LEFT JOIN vector_results v ON d.id = v.id
  WHERE
    (b.id IS NOT NULL OR v.id IS NOT NULL)
    AND (filter_category_id IS NULL OR d.category_id = filter_category_id)
    AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
    AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
    AND (filter_locations IS NULL OR d.locations && filter_locations)
    AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
  ORDER BY combined_score DESC
  LIMIT match_count;
END;
$$ LANGUAGE plpgsql;

-- ============================================
-- FACETED SEARCH FUNCTIONS
-- ============================================

-- Get facet aggregations (counts for filter values)
CREATE OR REPLACE FUNCTION get_facet_aggregations(
  query_text TEXT DEFAULT NULL,
  query_embedding VECTOR DEFAULT NULL,
  filter_category_id UUID DEFAULT NULL,
  filter_date_from TIMESTAMPTZ DEFAULT NULL,
  filter_date_to TIMESTAMPTZ DEFAULT NULL,
  filter_locations TEXT[] DEFAULT NULL,
  filter_keywords TEXT[] DEFAULT NULL,
  filter_authors TEXT[] DEFAULT NULL
)
RETURNS TABLE (
  facet_name TEXT,
  facet_value TEXT,
  count BIGINT
) AS $$
BEGIN
  -- Base query to find matching documents
  WITH matching_docs AS (
    SELECT d.id, d.category_id, d.keywords, d.locations, d.author,
           d.created_at, d.entities
    FROM documents d
    WHERE
      (query_text IS NULL OR d.id @@@ query_text)
      AND (filter_category_id IS NULL OR d.category_id = filter_category_id)
      AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
      AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
      AND (filter_locations IS NULL OR d.locations && filter_locations)
      AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
      AND (filter_authors IS NULL OR d.author = ANY(filter_authors))
  )
  SELECT * FROM (
    -- Categories facet
    SELECT 'category' as facet_name, c.name as facet_value, COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md
    LEFT JOIN categories c ON md.category_id = c.id
    WHERE c.name IS NOT NULL
    GROUP BY facet_name, c.name

    UNION ALL

    -- Keywords facet
    SELECT 'keyword' as facet_name, keyword as facet_value, COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md,
         LATERAL UNNEST(md.keywords) as keyword
    WHERE md.keywords IS NOT NULL
    GROUP BY facet_name, keyword

    UNION ALL

    -- Locations facet
    SELECT 'location' as facet_name, location as facet_value, COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md,
         LATERAL UNNEST(md.locations) as location
    WHERE md.locations IS NOT NULL
    GROUP BY facet_name, location

    UNION ALL

    -- Authors facet
    SELECT 'author' as facet_name, md.author as facet_value, COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md
    WHERE md.author IS NOT NULL AND md.author != ''
    GROUP BY facet_name, md.author

    UNION ALL

    -- Persons (from entities)
    SELECT 'person' as facet_name,
           jsonb_array_elements(md.entities->'persons')::text as facet_value,
           COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md
    WHERE md.entities->'persons' IS NOT NULL
    GROUP BY facet_name, jsonb_array_elements(md.entities->'persons')::text

    UNION ALL

    -- Organizations (from entities)
    SELECT 'organization' as facet_name,
           jsonb_array_elements(md.entities->'organizations')::text as facet_value,
           COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md
    WHERE md.entities->'organizations' IS NOT NULL
    GROUP BY facet_name, jsonb_array_elements(md.entities->'organizations')::text

    UNION ALL

    -- Products (from entities)
    SELECT 'product' as facet_name,
           jsonb_array_elements(md.entities->'products')::text as facet_value,
           COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md
    WHERE md.entities->'products' IS NOT NULL
    GROUP BY facet_name, jsonb_array_elements(md.entities->'products')::text

    UNION ALL

    -- Concepts (from entities)
    SELECT 'concept' as facet_name,
           jsonb_array_elements(md.entities->'concepts')::text as facet_value,
           COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md
    WHERE md.entities->'concepts' IS NOT NULL
    GROUP BY facet_name, jsonb_array_elements(md.entities->'concepts')::text
  ) facets
  ORDER BY facet_name, count DESC;
END;
$$ LANGUAGE plpgsql;

-- Get facet values with counts for a specific facet type
CREATE OR REPLACE FUNCTION get_facet_values(
  facet_type TEXT,
  query_text TEXT DEFAULT NULL,
  filter_category_id UUID DEFAULT NULL,
  filter_date_from TIMESTAMPTZ DEFAULT NULL,
  filter_date_to TIMESTAMPTZ DEFAULT NULL,
  filter_locations TEXT[] DEFAULT NULL,
  filter_keywords TEXT[] DEFAULT NULL,
  filter_authors TEXT[] DEFAULT NULL,
  limit_results INT DEFAULT 20
)
RETURNS TABLE (
  value TEXT,
  count BIGINT,
  selected BOOLEAN DEFAULT FALSE
)
AS $$
BEGIN
  RETURN QUERY
  WITH matching_docs AS (
    SELECT d.id, d.category_id, d.keywords, d.locations, d.author,
           d.created_at, d.entities
    FROM documents d
    WHERE
      (query_text IS NULL OR d.id @@@ query_text)
      AND (filter_category_id IS NULL OR d.category_id = filter_category_id)
      AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
      AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
      AND (filter_locations IS NULL OR d.locations && filter_locations)
      AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
      AND (filter_authors IS NULL OR d.author = ANY(filter_authors))
  )
  SELECT CASE
    WHEN facet_type = 'category' THEN
      SELECT c.name, COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md
      LEFT JOIN categories c ON md.category_id = c.id
      WHERE c.name IS NOT NULL
      GROUP BY c.name
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results

    WHEN facet_type = 'keyword' THEN
      SELECT keyword, COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md,
           LATERAL UNNEST(md.keywords) as keyword
      WHERE md.keywords IS NOT NULL
      GROUP BY keyword
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results

    WHEN facet_type = 'location' THEN
      SELECT location, COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md,
           LATERAL UNNEST(md.locations) as location
      WHERE md.locations IS NOT NULL
      GROUP BY location
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results

    WHEN facet_type = 'author' THEN
      SELECT md.author, COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md
      WHERE md.author IS NOT NULL AND md.author != ''
      GROUP BY md.author
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results

    WHEN facet_type = 'person' THEN
      SELECT jsonb_array_elements(md.entities->'persons')::text,
             COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md
      WHERE md.entities->'persons' IS NOT NULL
      GROUP BY jsonb_array_elements(md.entities->'persons')::text
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results

    WHEN facet_type = 'organization' THEN
      SELECT jsonb_array_elements(md.entities->'organizations')::text,
             COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md
      WHERE md.entities->'organizations' IS NOT NULL
      GROUP BY jsonb_array_elements(md.entities->'organizations')::text
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results

    WHEN facet_type = 'product' THEN
      SELECT jsonb_array_elements(md.entities->'products')::text,
             COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md
      WHERE md.entities->'products' IS NOT NULL
      GROUP BY jsonb_array_elements(md.entities->'products')::text
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results

    WHEN facet_type = 'concept' THEN
      SELECT jsonb_array_elements(md.entities->'concepts')::text,
             COUNT(DISTINCT md.id)::BIGINT, FALSE
      FROM matching_docs md
      WHERE md.entities->'concepts' IS NOT NULL
      GROUP BY jsonb_array_elements(md.entities->'concepts')::text
      ORDER BY COUNT(DISTINCT md.id) DESC
      LIMIT limit_results
  END;
END;
$$ LANGUAGE plpgsql;

-- Search with facets: returns both results and facet aggregations
CREATE OR REPLACE FUNCTION search_with_facets(
  query_text TEXT,
  query_embedding VECTOR,
  match_count INT,
  bm25_weight FLOAT,
  vector_weight FLOAT,
  filter_category_id UUID DEFAULT NULL,
  filter_date_from TIMESTAMPTZ DEFAULT NULL,
  filter_date_to TIMESTAMPTZ DEFAULT NULL,
  filter_locations TEXT[] DEFAULT NULL,
  filter_keywords TEXT[] DEFAULT NULL,
  filter_authors TEXT[] DEFAULT NULL,
  facet_limit INT DEFAULT 10
)
RETURNS TABLE (
  result_type TEXT,
  -- For search results
  id UUID,
  title TEXT,
  content TEXT,
  source_path TEXT,
  category_name TEXT,
  bm25_score FLOAT,
  vector_score FLOAT,
  combined_score FLOAT,
  -- For facets
  facet_name TEXT,
  facet_value TEXT,
  facet_count BIGINT
) AS $$
BEGIN
  -- First return search results
  RETURN QUERY
  SELECT 'result'::TEXT, d.id, d.title, d.content, d.source_path, c.name,
         COALESCE(1.0 / (60 + b.rank), 0.0)::FLOAT,
         COALESCE(1.0 / (60 + v.rank), 0.0)::FLOAT,
         LEAST(1.0, (COALESCE(bm25_weight * (1.0 / (60 + b.rank)), 0.0) +
          COALESCE(vector_weight * (1.0 / (60 + v.rank)), 0.0)))::FLOAT,
         NULL::TEXT, NULL::TEXT, NULL::BIGINT
  FROM (
    WITH bm25_results AS (
      SELECT d.id, ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as rank
      FROM documents d
      WHERE d.id @@@ query_text LIMIT match_count * 2
    ),
    vector_results AS (
      SELECT d.id, ROW_NUMBER() OVER (ORDER BY d.embedding <=> query_embedding) as rank
      FROM documents d
      WHERE d.embedding IS NOT NULL
      ORDER BY d.embedding <=> query_embedding LIMIT match_count * 2
    )
    SELECT d.id, d.title, d.content, d.source_path, d.category_id, b.rank, v.rank
    FROM documents d
    LEFT JOIN bm25_results b ON d.id = b.id
    LEFT JOIN vector_results v ON d.id = v.id
    WHERE
      (b.id IS NOT NULL OR v.id IS NOT NULL)
      AND (filter_category_id IS NULL OR d.category_id = filter_category_id)
      AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
      AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
      AND (filter_locations IS NULL OR d.locations && filter_locations)
      AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
      AND (filter_authors IS NULL OR d.author = ANY(filter_authors))
    ORDER BY LEAST(1.0, (COALESCE(bm25_weight * (1.0 / (60 + b.rank)), 0.0) +
              COALESCE(vector_weight * (1.0 / (60 + v.rank)), 0.0))) DESC
    LIMIT match_count
  ) results
  LEFT JOIN categories c ON results.category_id = c.id;

  -- Then return facet aggregations
  RETURN QUERY
  WITH matching_docs AS (
    SELECT d.id, d.category_id, d.keywords, d.locations, d.author,
           d.created_at, d.entities
    FROM documents d
    WHERE
      (query_text IS NULL OR d.id @@@ query_text)
      AND (filter_category_id IS NULL OR d.category_id = filter_category_id)
      AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
      AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
      AND (filter_locations IS NULL OR d.locations && filter_locations)
      AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
      AND (filter_authors IS NULL OR d.author = ANY(filter_authors))
  )
  SELECT 'facet'::TEXT, NULL::UUID, NULL::TEXT, NULL::TEXT, NULL::TEXT, NULL::TEXT,
         NULL::FLOAT, NULL::FLOAT, NULL::FLOAT, facet_name, facet_value, count
  FROM (
    SELECT 'category' as facet_name, c.name as facet_value, COUNT(DISTINCT md.id)::BIGINT as count
    FROM matching_docs md
    LEFT JOIN categories c ON md.category_id = c.id
    WHERE c.name IS NOT NULL
    GROUP BY c.name
    ORDER BY count DESC LIMIT facet_limit

    UNION ALL

    SELECT 'keyword', keyword, COUNT(DISTINCT md.id)::BIGINT
    FROM matching_docs md, LATERAL UNNEST(md.keywords) as keyword
    WHERE md.keywords IS NOT NULL
    GROUP BY keyword
    ORDER BY COUNT(DISTINCT md.id) DESC LIMIT facet_limit

    UNION ALL

    SELECT 'location', location, COUNT(DISTINCT md.id)::BIGINT
    FROM matching_docs md, LATERAL UNNEST(md.locations) as location
    WHERE md.locations IS NOT NULL
    GROUP BY location
    ORDER BY COUNT(DISTINCT md.id) DESC LIMIT facet_limit

    UNION ALL

    SELECT 'author', md.author, COUNT(DISTINCT md.id)::BIGINT
    FROM matching_docs md
    WHERE md.author IS NOT NULL AND md.author != ''
    GROUP BY md.author
    ORDER BY COUNT(DISTINCT md.id) DESC LIMIT facet_limit

    UNION ALL

    SELECT 'person', jsonb_array_elements(md.entities->'persons')::text, COUNT(DISTINCT md.id)::BIGINT
    FROM matching_docs md
    WHERE md.entities->'persons' IS NOT NULL
    GROUP BY jsonb_array_elements(md.entities->'persons')::text
    ORDER BY COUNT(DISTINCT md.id) DESC LIMIT facet_limit

    UNION ALL

    SELECT 'organization', jsonb_array_elements(md.entities->'organizations')::text, COUNT(DISTINCT md.id)::BIGINT
    FROM matching_docs md
    WHERE md.entities->'organizations' IS NOT NULL
    GROUP BY jsonb_array_elements(md.entities->'organizations')::text
    ORDER BY COUNT(DISTINCT md.id) DESC LIMIT facet_limit

    UNION ALL

    SELECT 'product', jsonb_array_elements(md.entities->'products')::text, COUNT(DISTINCT md.id)::BIGINT
    FROM matching_docs md
    WHERE md.entities->'products' IS NOT NULL
    GROUP BY jsonb_array_elements(md.entities->'products')::text
    ORDER BY COUNT(DISTINCT md.id) DESC LIMIT facet_limit

    UNION ALL

    SELECT 'concept', jsonb_array_elements(md.entities->'concepts')::text, COUNT(DISTINCT md.id)::BIGINT
    FROM matching_docs md
    WHERE md.entities->'concepts' IS NOT NULL
    GROUP BY jsonb_array_elements(md.entities->'concepts')::text
    ORDER BY COUNT(DISTINCT md.id) DESC LIMIT facet_limit
  ) facets;
END;
$$ LANGUAGE plpgsql;
