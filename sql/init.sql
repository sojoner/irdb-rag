-- ============================================
-- IRDB-RAG DATABASE INITIALIZATION
-- ============================================
-- Unified Database Schema with ParadeDB BM25 + pgvector
--
-- Key Performance Features:
-- 1. BM25 Full-Text Search (ParadeDB) - Fast keyword matching for text search
-- 2. pgvector HNSW Indexes - Semantic similarity search for chat/RAG
-- 3. JSONB GIN Indexes - Entity/facet aggregations
-- 4. Array GIN Indexes - Keywords and locations facets
-- 5. Composite Indexes - Document chunks optimization
-- 6. Comprehensive Metadata Indexing for faceted filtering
--
-- This file combines the complete schema initialization and metadata
-- indexing setup. It includes all tables, indexes, and SQL functions.

-- Enable extensions
CREATE EXTENSION IF NOT EXISTS vector;
DROP EXTENSION IF EXISTS pg_search CASCADE;
CREATE EXTENSION pg_search;

SET search_path TO public, paradedb;

-- Drop existing tables and functions to recreate with correct schema
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

-- ============================================
-- INDEXES - SEARCH & RETRIEVAL
-- ============================================

-- BM25 Index on documents
-- ParadeDB full-text search index for hybrid search
-- Note: key_field='id' is required to link BM25 scores back to document rows
CREATE INDEX documents_search_idx ON documents USING bm25 (id, content, title, summary)
WITH (key_field='id');

-- BM25 index for document_chunks (actual searchable content)
CREATE INDEX chunks_search_idx ON document_chunks USING bm25 (id, content)
WITH (key_field='id');

-- HNSW Vector indexes for semantic similarity search
CREATE INDEX IF NOT EXISTS idx_documents_embedding ON documents
USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

CREATE INDEX IF NOT EXISTS idx_document_chunks_embedding ON document_chunks
USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- ============================================
-- INDEXES - FILTERING & AGGREGATION
-- ============================================

-- JSONB GIN indexes for entity facets
CREATE INDEX IF NOT EXISTS idx_documents_entities_persons ON documents USING GIN ((entities->'persons'));
CREATE INDEX IF NOT EXISTS idx_documents_entities_organizations ON documents USING GIN ((entities->'organizations'));
CREATE INDEX IF NOT EXISTS idx_documents_entities_products ON documents USING GIN ((entities->'products'));
CREATE INDEX IF NOT EXISTS idx_documents_entities_concepts ON documents USING GIN ((entities->'concepts'));
CREATE INDEX IF NOT EXISTS idx_documents_entities_questions ON documents USING GIN ((entities->'questions'));
CREATE INDEX IF NOT EXISTS idx_documents_entities_jsonb ON documents USING GIN (entities);

-- Array GIN indexes for keywords and locations
CREATE INDEX IF NOT EXISTS idx_documents_keywords ON documents USING GIN (keywords);
CREATE INDEX IF NOT EXISTS idx_documents_locations ON documents USING GIN (locations);

-- Metadata JSONB index
CREATE INDEX IF NOT EXISTS idx_documents_metadata_jsonb ON documents USING GIN (metadata);

-- Scalar field indexes for filtering
CREATE INDEX IF NOT EXISTS idx_documents_author ON documents(author) WHERE author IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_documents_source_type ON documents(source_type);
CREATE INDEX IF NOT EXISTS idx_documents_status ON documents(status);
CREATE INDEX IF NOT EXISTS idx_documents_category_id ON documents(category_id);

-- Date range query optimization
CREATE INDEX IF NOT EXISTS idx_documents_created_at ON documents(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_documents_created_at_status ON documents(created_at DESC, status);

-- Document chunks optimization
CREATE INDEX IF NOT EXISTS idx_document_chunks_document_id ON document_chunks(document_id);
CREATE INDEX IF NOT EXISTS idx_document_chunks_document_chunk_idx ON document_chunks(document_id, chunk_index);

-- Unique index for idempotency
CREATE UNIQUE INDEX IF NOT EXISTS idx_documents_source_content_hash
ON documents (source_path, content_hash)
WHERE source_path IS NOT NULL AND content_hash IS NOT NULL;

-- Import Job Indexes
CREATE INDEX IF NOT EXISTS idx_import_jobs_status ON import_jobs(status);
CREATE INDEX IF NOT EXISTS idx_import_items_job_id ON import_items(job_id);
CREATE INDEX IF NOT EXISTS idx_import_items_status ON import_items(status);
CREATE INDEX IF NOT EXISTS idx_import_items_job_status_size ON import_items(job_id, status, file_size_bytes);

-- ============================================
-- METADATA FIELD DISCOVERY
-- ============================================

-- Get all unique entity types in entities JSONB
CREATE OR REPLACE FUNCTION get_entity_types()
RETURNS TABLE (
    entity_type TEXT
) AS $$
    SELECT DISTINCT key::TEXT as entity_type
    FROM documents,
         LATERAL jsonb_object_keys(entities) as key
    WHERE entities IS NOT NULL AND entities != 'null'::jsonb
    ORDER BY entity_type;
$$ LANGUAGE SQL;

-- Get all unique metadata keys
CREATE OR REPLACE FUNCTION get_metadata_keys()
RETURNS TABLE (
    metadata_key TEXT
) AS $$
    SELECT DISTINCT key::TEXT as metadata_key
    FROM documents,
         LATERAL jsonb_object_keys(metadata) as key
    WHERE metadata IS NOT NULL AND metadata != 'null'::jsonb
    ORDER BY metadata_key;
$$ LANGUAGE SQL;

-- ============================================
-- FACETED AGGREGATION FUNCTIONS
-- ============================================

-- Get all metadata facets with counts (comprehensive)
CREATE OR REPLACE FUNCTION get_all_metadata_facets(
    search_query TEXT DEFAULT NULL,
    search_embedding VECTOR DEFAULT NULL
)
RETURNS TABLE (
    facet_type TEXT,
    facet_value TEXT,
    count BIGINT
) AS $$
WITH matching_docs AS (
    SELECT DISTINCT d.id
    FROM documents d
    WHERE
        (search_query IS NULL OR (
            d.id @@@ search_query OR
            d.title @@ plainto_tsquery(search_query) OR
            d.content @@ plainto_tsquery(search_query)
        ))
        AND (search_embedding IS NULL OR
             (1.0 - (d.embedding <=> search_embedding)) > 0.3)
)
SELECT * FROM (
    -- Keywords facet
    SELECT 'keyword'::TEXT as facet_type, keyword as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN LATERAL UNNEST(doc.keywords) keyword ON TRUE
    WHERE doc.keywords IS NOT NULL AND keyword IS NOT NULL
    GROUP BY keyword

    UNION ALL

    -- Locations facet
    SELECT 'location'::TEXT as facet_type, location as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN LATERAL UNNEST(doc.locations) location ON TRUE
    WHERE doc.locations IS NOT NULL AND location IS NOT NULL
    GROUP BY location

    UNION ALL

    -- Author facet
    SELECT 'author'::TEXT as facet_type, doc.author as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    WHERE doc.author IS NOT NULL AND doc.author != ''
    GROUP BY doc.author

    UNION ALL

    -- Source type facet
    SELECT 'source_type'::TEXT as facet_type, doc.source_type as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    WHERE doc.source_type IS NOT NULL
    GROUP BY doc.source_type

    UNION ALL

    -- Status facet
    SELECT 'status'::TEXT as facet_type, doc.status as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    WHERE doc.status IS NOT NULL
    GROUP BY doc.status

    UNION ALL

    -- Category facet
    SELECT 'category'::TEXT as facet_type, c.name as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN categories c ON doc.category_id = c.id
    WHERE c.name IS NOT NULL
    GROUP BY c.name

    UNION ALL

    -- Persons entities facet
    SELECT 'person'::TEXT as facet_type, person as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN LATERAL jsonb_array_elements_text(doc.entities->'persons') person ON TRUE
    WHERE doc.entities->'persons' IS NOT NULL
    GROUP BY person

    UNION ALL

    -- Organizations entities facet
    SELECT 'organization'::TEXT as facet_type, org as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN LATERAL jsonb_array_elements_text(doc.entities->'organizations') org ON TRUE
    WHERE doc.entities->'organizations' IS NOT NULL
    GROUP BY org

    UNION ALL

    -- Products entities facet
    SELECT 'product'::TEXT as facet_type, product as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN LATERAL jsonb_array_elements_text(doc.entities->'products') product ON TRUE
    WHERE doc.entities->'products' IS NOT NULL
    GROUP BY product

    UNION ALL

    -- Concepts entities facet
    SELECT 'concept'::TEXT as facet_type, concept as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN LATERAL jsonb_array_elements_text(doc.entities->'concepts') concept ON TRUE
    WHERE doc.entities->'concepts' IS NOT NULL
    GROUP BY concept

    UNION ALL

    -- Questions entities facet
    SELECT 'question'::TEXT as facet_type, question as facet_value, COUNT(DISTINCT d.id)::BIGINT as count
    FROM matching_docs d
    JOIN documents doc ON d.id = doc.id
    LEFT JOIN LATERAL jsonb_array_elements_text(doc.entities->'questions') question ON TRUE
    WHERE doc.entities->'questions' IS NOT NULL
    GROUP BY question
) facets
ORDER BY facet_type, count DESC;
$$ LANGUAGE SQL;

-- Get facet values for a specific facet type
CREATE OR REPLACE FUNCTION get_facet_values(
    in_facet_type TEXT,
    search_query TEXT DEFAULT NULL,
    search_embedding VECTOR DEFAULT NULL,
    limit_count INT DEFAULT 20
)
RETURNS TABLE (
    facet_value TEXT,
    count BIGINT
) AS $$
SELECT f.facet_value, f.count FROM get_all_metadata_facets(search_query, search_embedding) f
WHERE f.facet_type = in_facet_type
ORDER BY f.count DESC
LIMIT limit_count;
$$ LANGUAGE SQL;

-- ============================================
-- SEARCH FUNCTIONS
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
  ORDER BY facet_name, count DESC
$$ LANGUAGE SQL;

-- Flexible search with dynamic metadata filtering
CREATE OR REPLACE FUNCTION flexible_filter_search(
    search_query TEXT DEFAULT NULL,
    search_embedding VECTOR DEFAULT NULL,
    match_count INT DEFAULT 20,
    bm25_weight FLOAT DEFAULT 0.6,
    vector_weight FLOAT DEFAULT 0.4,
    -- Array filters
    filter_keywords TEXT[] DEFAULT NULL,
    filter_locations TEXT[] DEFAULT NULL,
    filter_authors TEXT[] DEFAULT NULL,
    filter_source_types TEXT[] DEFAULT NULL,
    filter_statuses TEXT[] DEFAULT NULL,
    filter_categories TEXT[] DEFAULT NULL,
    -- Entity filters (from JSONB)
    filter_persons TEXT[] DEFAULT NULL,
    filter_organizations TEXT[] DEFAULT NULL,
    filter_products TEXT[] DEFAULT NULL,
    filter_concepts TEXT[] DEFAULT NULL,
    filter_questions TEXT[] DEFAULT NULL,
    -- Date range filters
    filter_date_from TIMESTAMPTZ DEFAULT NULL,
    filter_date_to TIMESTAMPTZ DEFAULT NULL
)
RETURNS TABLE (
    id UUID,
    title TEXT,
    content TEXT,
    source_path TEXT,
    source_type TEXT,
    author TEXT,
    status TEXT,
    category_name TEXT,
    keywords TEXT[],
    locations TEXT[],
    entities JSONB,
    bm25_score FLOAT,
    vector_score FLOAT,
    combined_score FLOAT
) AS $$
WITH bm25_results AS (
    SELECT
        d.id,
        ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as rank
    FROM documents d
    WHERE
        (search_query IS NULL OR d.id @@@ search_query)
        AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
        AND (filter_locations IS NULL OR d.locations && filter_locations)
        AND (filter_authors IS NULL OR d.author = ANY(filter_authors))
        AND (filter_source_types IS NULL OR d.source_type = ANY(filter_source_types))
        AND (filter_statuses IS NULL OR d.status = ANY(filter_statuses))
        AND (filter_categories IS NULL OR EXISTS (
            SELECT 1 FROM categories c WHERE c.id = d.category_id AND c.name = ANY(filter_categories)
        ))
        AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
        AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
        -- Entity filters
        AND (filter_persons IS NULL OR d.entities->'persons' ?| filter_persons)
        AND (filter_organizations IS NULL OR d.entities->'organizations' ?| filter_organizations)
        AND (filter_products IS NULL OR d.entities->'products' ?| filter_products)
        AND (filter_concepts IS NULL OR d.entities->'concepts' ?| filter_concepts)
        AND (filter_questions IS NULL OR d.entities->'questions' ?| filter_questions)
    LIMIT match_count * 3
),
vector_results AS (
    SELECT
        d.id,
        ROW_NUMBER() OVER (ORDER BY d.embedding <=> search_embedding) as rank
    FROM documents d
    WHERE
        d.embedding IS NOT NULL
        AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
        AND (filter_locations IS NULL OR d.locations && filter_locations)
        AND (filter_authors IS NULL OR d.author = ANY(filter_authors))
        AND (filter_source_types IS NULL OR d.source_type = ANY(filter_source_types))
        AND (filter_statuses IS NULL OR d.status = ANY(filter_statuses))
        AND (filter_categories IS NULL OR EXISTS (
            SELECT 1 FROM categories c WHERE c.id = d.category_id AND c.name = ANY(filter_categories)
        ))
        AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
        AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
        -- Entity filters
        AND (filter_persons IS NULL OR d.entities->'persons' ?| filter_persons)
        AND (filter_organizations IS NULL OR d.entities->'organizations' ?| filter_organizations)
        AND (filter_products IS NULL OR d.entities->'products' ?| filter_products)
        AND (filter_concepts IS NULL OR d.entities->'concepts' ?| filter_concepts)
        AND (filter_questions IS NULL OR d.entities->'questions' ?| filter_questions)
    ORDER BY d.embedding <=> search_embedding
    LIMIT match_count * 3
)
SELECT
    d.id,
    d.title,
    d.content,
    d.source_path,
    d.source_type,
    d.author,
    d.status,
    c.name as category_name,
    d.keywords,
    d.locations,
    d.entities,
    COALESCE(1.0 / (60 + b.rank), 0.0)::FLOAT as bm25_score,
    COALESCE(1.0 / (60 + v.rank), 0.0)::FLOAT as vector_score,
    LEAST(1.0, (
        COALESCE(bm25_weight * (1.0 / (60 + b.rank)), 0.0) +
        COALESCE(vector_weight * (1.0 / (60 + v.rank)), 0.0)
    ))::FLOAT as combined_score
FROM documents d
LEFT JOIN categories c ON d.category_id = c.id
LEFT JOIN bm25_results b ON d.id = b.id
LEFT JOIN vector_results v ON d.id = v.id
WHERE (b.id IS NOT NULL OR v.id IS NOT NULL)
ORDER BY combined_score DESC, bm25_score DESC
LIMIT match_count;
$$ LANGUAGE SQL;

-- ============================================
-- MONITORING & MAINTENANCE VIEWS
-- ============================================

-- View all available filter values across the database
CREATE OR REPLACE VIEW metadata_filter_catalog AS
WITH all_facets AS (
    SELECT 'keywords' as field_type, keyword as field_value
    FROM documents, LATERAL UNNEST(keywords) keyword
    WHERE keywords IS NOT NULL

    UNION ALL

    SELECT 'locations' as field_type, location as field_value
    FROM documents, LATERAL UNNEST(locations) location
    WHERE locations IS NOT NULL

    UNION ALL

    SELECT 'authors' as field_type, author as field_value
    FROM documents
    WHERE author IS NOT NULL

    UNION ALL

    SELECT 'source_types' as field_type, source_type as field_value
    FROM documents
    WHERE source_type IS NOT NULL

    UNION ALL

    SELECT 'statuses' as field_type, status as field_value
    FROM documents
    WHERE status IS NOT NULL

    UNION ALL

    SELECT 'categories' as field_type, c.name as field_value
    FROM documents d
    LEFT JOIN categories c ON d.category_id = c.id
    WHERE c.name IS NOT NULL

    UNION ALL

    SELECT 'persons' as field_type, person as field_value
    FROM documents, LATERAL jsonb_array_elements_text(entities->'persons') person
    WHERE entities->'persons' IS NOT NULL

    UNION ALL

    SELECT 'organizations' as field_type, org as field_value
    FROM documents, LATERAL jsonb_array_elements_text(entities->'organizations') org
    WHERE entities->'organizations' IS NOT NULL

    UNION ALL

    SELECT 'products' as field_type, product as field_value
    FROM documents, LATERAL jsonb_array_elements_text(entities->'products') product
    WHERE entities->'products' IS NOT NULL

    UNION ALL

    SELECT 'concepts' as field_type, concept as field_value
    FROM documents, LATERAL jsonb_array_elements_text(entities->'concepts') concept
    WHERE entities->'concepts' IS NOT NULL

    UNION ALL

    SELECT 'questions' as field_type, question as field_value
    FROM documents, LATERAL jsonb_array_elements_text(entities->'questions') question
    WHERE entities->'questions' IS NOT NULL
)
SELECT
    field_type,
    field_value,
    COUNT(*) as frequency
FROM all_facets
GROUP BY field_type, field_value
ORDER BY field_type, frequency DESC;

-- Index statistics function
CREATE OR REPLACE FUNCTION show_metadata_index_stats()
RETURNS TABLE (
    index_name TEXT,
    table_name TEXT,
    index_type TEXT,
    size_mb FLOAT
) AS $$
SELECT
    indexname,
    tablename,
    CASE
        WHEN indexdef::TEXT ~ 'USING gin' THEN 'GIN'
        WHEN indexdef::TEXT ~ 'USING bm25' THEN 'BM25'
        WHEN indexdef::TEXT ~ 'USING hnsw' THEN 'HNSW'
        ELSE 'OTHER'
    END as index_type,
    ROUND(pg_relation_size(indexname::regclass) / 1024.0 / 1024.0, 2)::FLOAT
FROM pg_indexes
WHERE tablename IN ('documents', 'document_chunks')
ORDER BY pg_relation_size(indexname::regclass) DESC;
$$ LANGUAGE SQL;

-- ============================================
-- PARADEDB CONFIGURATION & PERFORMANCE TUNING
-- ============================================
-- Configure ParadeDB for optimal performance with large document sets
-- These settings should be applied via docker-compose POSTGRES_INITDB_ARGS
--
-- CRITICAL SETTINGS:
-- 1. paradedb.enable_aggregate_custom_scan = on
-- 2. paradedb.enable_custom_scan_without_operator = on
-- 3. paradedb.per_tuple_cost = 100
-- 4. paradedb.limit_fetch_multiplier = 2
--
-- INDEX MAINTENANCE (run periodically):
-- - VACUUM ANALYZE documents;
-- - REINDEX INDEX documents_search_idx;
-- - REINDEX INDEX chunks_search_idx;
--
-- MONITORING (check query performance):
-- - EXPLAIN ANALYZE SELECT * FROM hybrid_search(...);
-- - SELECT * FROM show_metadata_index_stats();
