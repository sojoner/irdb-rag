-- Schema Migration: Add embedding_status column to document_chunks
-- This migration adds the embedding_status column that was added to init.sql
-- but may be missing from existing databases

-- Create enum type if not exists
CREATE TYPE IF NOT EXISTS embedding_status AS ENUM ('pending', 'processing', 'completed', 'failed');

-- Add embedding_status column to document_chunks if it doesn't exist
ALTER TABLE IF EXISTS document_chunks
ADD COLUMN IF NOT EXISTS embedding_status embedding_status DEFAULT 'pending';

-- Create index on embedding_status for efficient filtering
CREATE INDEX IF NOT EXISTS idx_document_chunks_embedding_status 
ON document_chunks(embedding_status);

-- Add embedding_status column to documents if it doesn't exist
ALTER TABLE IF EXISTS documents
ADD COLUMN IF NOT EXISTS embedding_status embedding_status DEFAULT 'pending';

-- Create index on documents.embedding_status
CREATE INDEX IF NOT EXISTS idx_documents_embedding_status 
ON documents(embedding_status);
