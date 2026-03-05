-- ============================================
-- Migration: Add Embedding Status Tracking
-- ============================================
-- Adds embedding_status column to documents and document_chunks tables
-- to track asynchronous embedding generation state.
--
-- SAFETY: This script is idempotent.
-- - Uses IF NOT EXISTS and ALTER TABLE only if column missing
-- - Safe to run multiple times on existing databases
-- - Will NOT drop or overwrite existing data
--

-- Create ENUM type for embedding status (idempotent with IF NOT EXISTS)
DO $$ 
BEGIN 
  IF NOT EXISTS (SELECT 1 FROM pg_type WHERE typname = 'embedding_status') THEN 
    CREATE TYPE embedding_status AS ENUM ('pending', 'processing', 'completed', 'failed');
  END IF;
END $$;

-- Add embedding_status column to documents table
ALTER TABLE IF EXISTS documents 
ADD COLUMN IF NOT EXISTS embedding_status embedding_status DEFAULT 'pending';

-- Add embedding_status column to document_chunks table
ALTER TABLE IF EXISTS document_chunks 
ADD COLUMN IF NOT EXISTS embedding_status embedding_status DEFAULT 'pending';

-- Create indexes on embedding_status for efficient filtering
CREATE INDEX IF NOT EXISTS idx_documents_embedding_status 
ON documents(embedding_status);

CREATE INDEX IF NOT EXISTS idx_document_chunks_embedding_status 
ON document_chunks(embedding_status);
