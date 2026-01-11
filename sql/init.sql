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
CREATE INDEX documents_search_idx ON documents USING bm25 (id, content, title, summary)
WITH (key_field='id');

-- Vector indexes created after data insertion when dimensions are known
-- (HNSW requires dimension specification, so we skip dynamic dimension creation)

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
    (
      COALESCE(bm25_weight * (1.0 / (60 + b.rank)), 0.0) +
      COALESCE(vector_weight * (1.0 / (60 + v.rank)), 0.0)
    )::FLOAT as combined_score,
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
