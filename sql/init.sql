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

-- Documents table
-- Note: EMBEDDING_DIMENSIONS should match the embedding model's output dimension
-- Default: 4096 for Qwen3-Embedding-8B (supports MRL for custom dimensions 32-4096)
CREATE TABLE documents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    content TEXT NOT NULL,
    source_path TEXT,
    source_type TEXT NOT NULL,
    summary TEXT,
    author TEXT,
    category_id UUID,
    keywords TEXT[],
    locations TEXT[],
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    indexed_at TIMESTAMPTZ,
    word_count INTEGER,
    status TEXT DEFAULT 'pending',
    entities JSONB,
    metadata JSONB,
    embedding VECTOR(${EMBEDDING_DIMENSIONS})
);

-- Document chunks
CREATE TABLE document_chunks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    document_id UUID REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    page_number INTEGER,
    section_title TEXT,
    token_count INTEGER,
    embedding VECTOR(${EMBEDDING_DIMENSIONS}),
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Document assets
CREATE TABLE document_assets (
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
CREATE TABLE categories (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL UNIQUE,
    description TEXT,
    parent_id UUID REFERENCES categories(id)
);

-- Conversations (for chat history)
CREATE TABLE conversations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- Messages
CREATE TABLE messages (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    conversation_id UUID REFERENCES conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- Import Jobs (for tracking batch import operations)
CREATE TABLE import_jobs (
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
CREATE TABLE import_items (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID REFERENCES import_jobs(id) ON DELETE CASCADE,
    source_path TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',  -- pending, processing, completed, failed, skipped
    retry_count INTEGER DEFAULT 0,
    error_message TEXT,
    error_type TEXT,                         -- transient, permanent
    document_id UUID REFERENCES documents(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    processed_at TIMESTAMPTZ
);

-- Indexes

-- BM25 Index on documents
CREATE INDEX documents_search_idx ON documents USING bm25 (id, content, title, summary)
WITH (key_field='id');

-- Vector Index on documents
CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops);

-- Vector Index on chunks
CREATE INDEX ON document_chunks USING hnsw (embedding vector_cosine_ops);

-- Import Job Indexes
CREATE INDEX idx_import_jobs_status ON import_jobs(status);
CREATE INDEX idx_import_items_job_id ON import_items(job_id);
CREATE INDEX idx_import_items_status ON import_items(status);

-- Hybrid Search Function
CREATE OR REPLACE FUNCTION hybrid_search(
  query_text TEXT,
  query_embedding VECTOR(${EMBEDDING_DIMENSIONS}),
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
  combined_score FLOAT
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
    )::FLOAT as combined_score
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
