-- docker-entrypoint-initdb.d/01-ai-extensions.sql
-- Create extensions for AI/ML workloads
-- Fixed version that uses rag_user instead of postgres

-- Skip ai_data schema creation - this project uses public schema with its own tables
-- The schema and tables from the original script conflict with this RAG project

-- Just ensure we have the required extensions (these are idempotent)
CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS pg_search;

-- Note: The actual schema for this project is defined in init.sql
-- which creates documents, document_chunks, conversations, etc. in public schema
