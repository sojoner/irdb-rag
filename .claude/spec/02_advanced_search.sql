-- Advanced Search Functions for RAG Chat
-- Implements sophisticated search with:
-- - Phrase matching (exact sequences)
-- - Longest prefix matching
-- - Boolean operators (AND, OR, NOT)
-- - Faceted aggregations
-- - Optimized tokenization
-- - Separate functions for chat vs traditional search

SET search_path TO public, paradedb;

-- ============================================================================
-- TOKENIZATION & QUERY NORMALIZATION
-- ============================================================================

-- Improved tokenizer that handles compound words, hyphens, and special cases
CREATE OR REPLACE FUNCTION tokenize_query(query_text TEXT)
RETURNS TEXT[] AS $$
DECLARE
    tokens TEXT[];
    token TEXT;
BEGIN
    -- Convert to lowercase
    query_text := LOWER(query_text);

    -- Remove extra whitespace
    query_text := TRIM(BOTH ' ' FROM query_text);
    query_text := REGEXP_REPLACE(query_text, '\s+', ' ', 'g');

    -- Preserve hyphenated terms by replacing hyphens temporarily
    query_text := REGEXP_REPLACE(query_text, '-', '~HYPHEN~', 'g');

    -- Split into tokens
    tokens := STRING_TO_ARRAY(query_text, ' ');

    -- Restore hyphens and filter empty tokens
    tokens := ARRAY(
        SELECT REGEXP_REPLACE(t, '~HYPHEN~', '-', 'g')
        FROM UNNEST(tokens) AS t
        WHERE TRIM(t) != ''
        AND LENGTH(t) > 1  -- Filter single characters (noise)
    );

    RETURN tokens;
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Build different query types from tokens
CREATE OR REPLACE FUNCTION build_phrase_query(tokens TEXT[])
RETURNS TEXT AS $$
BEGIN
    -- Phrase queries must match tokens in exact order, adjacent
    IF array_length(tokens, 1) IS NULL OR array_length(tokens, 1) = 0 THEN
        RETURN 'id:__no_match__';
    END IF;

    -- Return ARRAY format for paradedb.phrase()
    RETURN FORMAT('PHRASE(%s)', ARRAY_TO_STRING(tokens, ' '));
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Build longest matching prefix query for fuzzy matching
CREATE OR REPLACE FUNCTION build_prefix_query(query_text TEXT)
RETURNS TEXT AS $$
DECLARE
    tokens TEXT[];
    prefix_terms TEXT[];
    i INT;
BEGIN
    tokens := tokenize_query(query_text);

    IF array_length(tokens, 1) IS NULL THEN
        RETURN 'id:__no_match__';
    END IF;

    -- Append * to each token for prefix matching
    prefix_terms := ARRAY(
        SELECT t || '*'
        FROM UNNEST(tokens) AS t
    );

    -- Join with OR operator (|||)
    RETURN ARRAY_TO_STRING(prefix_terms, ' ||| ');
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- Build boolean query with AND semantics
CREATE OR REPLACE FUNCTION build_boolean_query(query_text TEXT)
RETURNS TEXT AS $$
DECLARE
    tokens TEXT[];
BEGIN
    tokens := tokenize_query(query_text);

    IF array_length(tokens, 1) IS NULL THEN
        RETURN 'id:__no_match__';
    END IF;

    -- Join with AND operator (&&&) for strict matching
    RETURN ARRAY_TO_STRING(tokens, ' &&& ');
END;
$$ LANGUAGE plpgsql IMMUTABLE;

-- ============================================================================
-- ADVANCED HYBRID SEARCH (Better F-measure through sophisticated ranking)
-- ============================================================================

-- Advanced hybrid search combining multiple strategies:
-- 1. Phrase matching (exact phrases get high weight)
-- 2. Boolean matching (AND semantics for precision)
-- 3. Prefix/fuzzy matching (recall with flexibility)
-- 4. Vector semantic search (lower weight initially)
-- 5. Faceted filtering for precision
CREATE OR REPLACE FUNCTION hybrid_search_advanced(
    query_text TEXT,
    query_embedding VECTOR,
    match_count INT DEFAULT 10,
    bm25_weight FLOAT DEFAULT 0.6,
    vector_weight FLOAT DEFAULT 0.2,
    phrase_weight FLOAT DEFAULT 0.15,
    prefix_weight FLOAT DEFAULT 0.05,
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
    phrase_score FLOAT,
    prefix_score FLOAT,
    vector_score FLOAT,
    combined_score FLOAT,
    reranker_score FLOAT
) AS $$
DECLARE
    phrase_query TEXT;
    boolean_query TEXT;
    prefix_query TEXT;
    sanitized_query TEXT;
BEGIN
    -- Sanitize and tokenize input
    sanitized_query := CASE
        WHEN TRIM(query_text) = '' THEN 'id:__no_match__'
        WHEN TRIM(query_text) = '*' THEN 'id:__no_match__'
        ELSE TRIM(query_text)
    END;

    -- Build specialized queries
    phrase_query := build_phrase_query(tokenize_query(query_text));
    boolean_query := build_boolean_query(query_text);
    prefix_query := build_prefix_query(query_text);

    RETURN QUERY
    WITH phrase_results AS (
        -- Match exact phrases: high precision
        SELECT
            d.id,
            2.0 * paradedb.score(d.id) AS phrase_score
        FROM documents d
        WHERE d.content @@@ phrase_query OR d.title @@@ phrase_query
        LIMIT match_count * 4
    ),
    bm25_results AS (
        -- Full BM25 search: standard lexical matching
        SELECT
            d.id,
            ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as bm25_rank
        FROM documents d
        WHERE d.id @@@ sanitized_query OR d.title @@@ sanitized_query
        LIMIT match_count * 3
    ),
    boolean_results AS (
        -- Boolean AND matching: requires all terms present
        SELECT
            d.id,
            1.5 * paradedb.score(d.id) AS boolean_score
        FROM documents d
        WHERE d.content @@@ boolean_query AND d.id @@@ sanitized_query
        LIMIT match_count * 3
    ),
    prefix_results AS (
        -- Prefix/fuzzy matching: recall with flexibility
        SELECT
            d.id,
            ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as prefix_rank
        FROM documents d
        WHERE d.content @@@ prefix_query OR d.title @@@ prefix_query
        LIMIT match_count * 2
    ),
    vector_results AS (
        -- Vector semantic search: low weight for initial ranking
        SELECT
            d.id,
            ROW_NUMBER() OVER (ORDER BY d.embedding <=> query_embedding) as vector_rank
        FROM documents d
        WHERE d.embedding IS NOT NULL
        ORDER BY d.embedding <=> query_embedding
        LIMIT match_count * 3
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
        COALESCE(ar.phrase_score, 0.0)::FLOAT as phrase_score,
        COALESCE(ar.prefix_score, 0.0)::FLOAT as prefix_score,
        COALESCE(1.0 - (d.embedding <=> query_embedding), 0.0)::FLOAT as vector_score,
        (
            COALESCE(ar.phrase_score, 0.0) * phrase_weight +
            COALESCE(ar.bm25_score, 0.0) * bm25_weight +
            COALESCE(ar.boolean_score, 0.0) * 0.15 +
            COALESCE(ar.prefix_score, 0.0) * prefix_weight +
            COALESCE(1.0 - (d.embedding <=> query_embedding), 0.0) * vector_weight
        )::FLOAT as combined_score,
        NULL::FLOAT as reranker_score
    FROM all_results ar
    JOIN documents d ON ar.result_id = d.id
    LEFT JOIN categories c ON d.category_id = c.id
    WHERE
        (filter_category_id IS NULL OR d.category_id = filter_category_id)
        AND (filter_date_from IS NULL OR d.created_at >= filter_date_from)
        AND (filter_date_to IS NULL OR d.created_at <= filter_date_to)
        AND (filter_locations IS NULL OR d.locations && filter_locations)
        AND (filter_keywords IS NULL OR d.keywords && filter_keywords)
    ORDER BY combined_score DESC
    LIMIT match_count;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- SPECIALIZED SEARCH FOR CHAT (LLM Context Retrieval)
-- ============================================================================

-- Optimized for LLM context retrieval: emphasizes semantic similarity over lexical matching
-- Lower precision threshold, broader recall for conversational context
CREATE OR REPLACE FUNCTION search_for_chat(
    query_text TEXT,
    query_embedding VECTOR,
    match_count INT DEFAULT 5,
    vector_weight FLOAT DEFAULT 0.7,
    bm25_weight FLOAT DEFAULT 0.3,
    similarity_threshold FLOAT DEFAULT 0.3
) RETURNS TABLE (
    id UUID,
    title TEXT,
    content TEXT,
    source_path TEXT,
    category_name TEXT,
    semantic_score FLOAT,
    lexical_score FLOAT,
    combined_score FLOAT,
    relevance_reason TEXT
) AS $$
BEGIN
    RETURN QUERY
    WITH vector_ranked AS (
        SELECT
            d.id,
            1.0 - (d.embedding <=> query_embedding) as semantic_score,
            ROW_NUMBER() OVER (ORDER BY d.embedding <=> query_embedding) as rank
        FROM documents d
        WHERE d.embedding IS NOT NULL
        AND (1.0 - (d.embedding <=> query_embedding)) >= similarity_threshold
        ORDER BY d.embedding <=> query_embedding
        LIMIT match_count * 2
    ),
    bm25_ranked AS (
        SELECT
            d.id,
            paradedb.score(d.id) as bm25_score,
            ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as bm25_rank
        FROM documents d
        WHERE d.id @@@ LOWER(query_text)
        LIMIT match_count * 2
    )
    SELECT
        d.id,
        d.title,
        d.content,
        d.source_path,
        c.name as category_name,
        COALESCE(v.semantic_score, 0.0)::FLOAT as semantic_score,
        COALESCE(1.0 / (60 + b.bm25_rank), 0.0)::FLOAT as lexical_score,
        (
            COALESCE(v.semantic_score, 0.0) * vector_weight +
            COALESCE(1.0 / (60 + b.bm25_rank), 0.0) * bm25_weight
        )::FLOAT as combined_score,
        CASE
            WHEN COALESCE(v.semantic_score, 0.0) > 0.7 THEN 'Strong semantic match'
            WHEN COALESCE(v.semantic_score, 0.0) > 0.5 THEN 'Moderate semantic match'
            WHEN COALESCE(1.0 / (60 + b.bm25_rank), 0.0) > 0.5 THEN 'Strong lexical match'
            ELSE 'Contextual match'
        END::TEXT as relevance_reason
    FROM vector_ranked v
    FULL OUTER JOIN bm25_ranked b ON v.id = b.id
    JOIN documents d ON COALESCE(v.id, b.id) = d.id
    LEFT JOIN categories c ON d.category_id = c.id
    ORDER BY combined_score DESC, semantic_score DESC
    LIMIT match_count;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- TRADITIONAL SEARCH (Search Bar / Discovery)
-- ============================================================================

-- Optimized for traditional search UI: emphasizes precision and exact matches
-- Higher precision threshold, strict ranking for explicit queries
CREATE OR REPLACE FUNCTION search_traditional(
    query_text TEXT,
    query_embedding VECTOR,
    match_count INT DEFAULT 20,
    bm25_weight FLOAT DEFAULT 0.7,
    vector_weight FLOAT DEFAULT 0.2,
    phrase_weight FLOAT DEFAULT 0.1
) RETURNS TABLE (
    id UUID,
    title TEXT,
    content TEXT,
    source_path TEXT,
    category_name TEXT,
    rank_score FLOAT,
    match_type TEXT,
    matched_terms TEXT[],
    combined_score FLOAT
) AS $$
DECLARE
    tokens TEXT[];
    phrase_query TEXT;
BEGIN
    tokens := tokenize_query(query_text);
    phrase_query := build_phrase_query(tokens);

    RETURN QUERY
    WITH phrase_matches AS (
        SELECT
            d.id,
            2.0 * paradedb.score(d.id) as phrase_score,
            'phrase'::TEXT as match_type,
            tokens as matched_terms
        FROM documents d
        WHERE (d.content @@@ phrase_query OR d.title @@@ phrase_query)
        LIMIT match_count * 2
    ),
    exact_matches AS (
        SELECT
            d.id,
            paradedb.score(d.id) as exact_score,
            'exact'::TEXT as match_type,
            tokens as matched_terms
        FROM documents d
        WHERE d.id @@@ LOWER(query_text)
        LIMIT match_count * 2
    ),
    prefix_matches AS (
        SELECT
            d.id,
            paradedb.score(d.id) * 0.7 as prefix_score,
            'prefix'::TEXT as match_type,
            tokens as matched_terms
        FROM documents d
        WHERE d.content @@@ build_prefix_query(query_text)
        LIMIT match_count * 2
    ),
    semantic_matches AS (
        SELECT
            d.id,
            1.0 - (d.embedding <=> query_embedding) as semantic_score,
            'semantic'::TEXT as match_type,
            tokens as matched_terms
        FROM documents d
        WHERE d.embedding IS NOT NULL
        ORDER BY d.embedding <=> query_embedding
        LIMIT match_count
    )
    SELECT
        d.id,
        d.title,
        d.content,
        d.source_path,
        c.name as category_name,
        COALESCE(
            ph.phrase_score,
            ex.exact_score,
            pr.prefix_score,
            sm.semantic_score,
            0.0
        )::FLOAT as rank_score,
        COALESCE(
            ph.match_type,
            ex.match_type,
            pr.match_type,
            sm.match_type,
            'unknown'
        )::TEXT as match_type,
        COALESCE(
            ph.matched_terms,
            ex.matched_terms,
            pr.matched_terms,
            sm.matched_terms,
            '{}'::TEXT[]
        ) as matched_terms,
        (
            COALESCE(ph.phrase_score, 0.0) * phrase_weight +
            COALESCE(ex.exact_score, 0.0) * bm25_weight +
            COALESCE(pr.prefix_score, 0.0) * 0.1 +
            COALESCE(sm.semantic_score, 0.0) * vector_weight
        )::FLOAT as combined_score
    FROM phrase_matches ph
    FULL OUTER JOIN exact_matches ex ON ph.id = ex.id
    FULL OUTER JOIN prefix_matches pr ON COALESCE(ph.id, ex.id) = pr.id
    FULL OUTER JOIN semantic_matches sm ON COALESCE(ph.id, ex.id, pr.id) = sm.id
    JOIN documents d ON COALESCE(ph.id, ex.id, pr.id, sm.id) = d.id
    LEFT JOIN categories c ON d.category_id = c.id
    ORDER BY combined_score DESC, rank_score DESC
    LIMIT match_count;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- FACETED SEARCH WITH AGGREGATIONS
-- ============================================================================

-- Get faceted results with aggregated metadata for filtering
CREATE OR REPLACE FUNCTION search_with_facets(
    query_text TEXT,
    match_count INT DEFAULT 20
) RETURNS TABLE (
    id UUID,
    title TEXT,
    content TEXT,
    source_path TEXT,
    category_name TEXT,
    combined_score FLOAT,
    author TEXT,
    keywords TEXT[],
    locations TEXT[],
    entities JSONB
) AS $$
DECLARE
    sanitized_query TEXT;
BEGIN
    sanitized_query := CASE
        WHEN TRIM(query_text) = '' THEN 'id:__no_match__'
        WHEN TRIM(query_text) = '*' THEN 'id:__no_match__'
        ELSE LOWER(TRIM(query_text))
    END;

    RETURN QUERY
    WITH bm25_ranked AS (
        SELECT
            d.id,
            ROW_NUMBER() OVER (ORDER BY paradedb.score(d.id) DESC) as rank
        FROM documents d
        WHERE d.id @@@ sanitized_query
        LIMIT match_count * 2
    )
    SELECT
        d.id,
        d.title,
        d.content,
        d.source_path,
        c.name as category_name,
        (1.0 / (60 + b.rank))::FLOAT as combined_score,
        d.author,
        d.keywords,
        d.locations,
        d.entities
    FROM bm25_ranked b
    JOIN documents d ON b.id = d.id
    LEFT JOIN categories c ON d.category_id = c.id
    ORDER BY combined_score DESC
    LIMIT match_count;
END;
$$ LANGUAGE plpgsql;

-- Get category facets for search results
CREATE OR REPLACE FUNCTION get_category_facets(query_text TEXT)
RETURNS TABLE (
    category_name TEXT,
    count BIGINT,
    avg_score FLOAT
) AS $$
DECLARE
    sanitized_query TEXT;
BEGIN
    sanitized_query := CASE
        WHEN TRIM(query_text) = '' THEN 'id:__no_match__'
        WHEN TRIM(query_text) = '*' THEN 'id:__no_match__'
        ELSE LOWER(TRIM(query_text))
    END;

    RETURN QUERY
    SELECT
        COALESCE(c.name, 'Uncategorized') as category_name,
        COUNT(d.id) as count,
        AVG(paradedb.score(d.id))::FLOAT as avg_score
    FROM documents d
    LEFT JOIN categories c ON d.category_id = c.id
    WHERE d.id @@@ sanitized_query
    GROUP BY c.name
    ORDER BY count DESC
    LIMIT 20;
END;
$$ LANGUAGE plpgsql;

-- Get keyword/entity facets
CREATE OR REPLACE FUNCTION get_keyword_facets(query_text TEXT)
RETURNS TABLE (
    keyword TEXT,
    count BIGINT
) AS $$
DECLARE
    sanitized_query TEXT;
BEGIN
    sanitized_query := CASE
        WHEN TRIM(query_text) = '' THEN 'id:__no_match__'
        WHEN TRIM(query_text) = '*' THEN 'id:__no_match__'
        ELSE LOWER(TRIM(query_text))
    END;

    RETURN QUERY
    SELECT
        keyword,
        COUNT(*) as count
    FROM documents d, UNNEST(d.keywords) AS keyword
    WHERE d.id @@@ sanitized_query
    GROUP BY keyword
    ORDER BY count DESC
    LIMIT 20;
END;
$$ LANGUAGE plpgsql;

-- Get location facets
CREATE OR REPLACE FUNCTION get_location_facets(query_text TEXT)
RETURNS TABLE (
    location TEXT,
    count BIGINT
) AS $$
DECLARE
    sanitized_query TEXT;
BEGIN
    sanitized_query := CASE
        WHEN TRIM(query_text) = '' THEN 'id:__no_match__'
        WHEN TRIM(query_text) = '*' THEN 'id:__no_match__'
        ELSE LOWER(TRIM(query_text))
    END;

    RETURN QUERY
    SELECT
        location,
        COUNT(*) as count
    FROM documents d, UNNEST(d.locations) AS location
    WHERE d.id @@@ sanitized_query
    GROUP BY location
    ORDER BY count DESC
    LIMIT 20;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- PERFORMANCE & MAINTENANCE
-- ============================================================================

-- Create additional indexes for better performance on advanced searches
CREATE INDEX IF NOT EXISTS idx_documents_embedding_hnsw
ON documents USING hnsw (embedding vector_cosine_ops)
WITH (m = 16, ef_construction = 64);

-- Index for date range queries
CREATE INDEX IF NOT EXISTS idx_documents_created_at
ON documents(created_at DESC);

-- Index for category filtering
CREATE INDEX IF NOT EXISTS idx_documents_category_id
ON documents(category_id);

-- Index on keywords array
CREATE INDEX IF NOT EXISTS idx_documents_keywords_gin
ON documents USING gin(keywords);

-- Index on locations array
CREATE INDEX IF NOT EXISTS idx_documents_locations_gin
ON documents USING gin(locations);

-- Analyze performance configuration for HNSW search
-- Higher ef_search = more accurate but slower
-- Recommended: 100-200 for balanced performance
SET hnsw.ef_search = 100;

-- ============================================================================
-- VACUUM & ANALYZE (Run periodically for maintenance)
-- ============================================================================

-- Optimize indexes after bulk operations
-- VACUUM ANALYZE documents;
-- REINDEX INDEX documents_search_idx;
-- REINDEX INDEX idx_documents_embedding_hnsw;
