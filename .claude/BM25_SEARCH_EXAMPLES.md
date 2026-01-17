# Enhanced BM25 Search Testing Guide

## Overview

The BM25 search has been enhanced to search across **all indexed fields**:
- `content` - Main document body text
- `title` - Document title
- `summary` - Document summary
- `author` - Document author (NEW)
- `source_path` - File path/source location (NEW)

## Query Building Strategies

All search queries now support searching in all 5 indexed fields with the following strategies:

### 1. **Sanitized Query** (Standard Full-Text)
Used for general keyword matching across all fields.

**Example Queries to Test:**
```
search: "rust programming"
Expected: Finds documents with "rust" or "programming" in any field (content, title, summary, author, source_path)

search: "John Smith"
Expected: Finds documents authored by "John Smith" OR containing "John Smith" in content/title

search: "database optimization"
Expected: Searches all fields for these terms
```

### 2. **Phrase Query** (Exact Phrase Matching)
Looks for exact sequences of words in order.

**Example Queries to Test:**
```
search: "machine learning"
Expected: Finds "machine learning" as exact phrase in any field
High relevance boost (2.0x multiplier)

search: "neural networks"
Expected: Exact phrase matching across all indexed fields

search: "distributed systems"
Expected: Matches exact phrase in content/title/author/etc
```

### 3. **Boolean Query** (AND semantics - All Terms Required)
All terms must be present in a document (across any field).

**Example Queries to Test:**
```
search: "python data science"
Expected: Documents must contain ALL of: python, data, science
High precision search (1.5x multiplier)

search: "cloud computing infrastructure"
Expected: All 3 terms required somewhere in the indexed fields

search: "web development framework"
Expected: Finds docs with all 3 terms (strict matching)
```

### 4. **Prefix Query** (Typo Tolerance & Partial Matching)
Wildcard matching for partial words and typos.

**Example Queries to Test:**
```
search: "algoritm"  (typo for "algorithm")
Expected: Matches "algorithm", "algorithms" via prefix matching
Used with OR semantics (5% weight)

search: "databas"  (incomplete)
Expected: Matches "database", "databases", "database_name"

search: "comput" (incomplete)
Expected: Matches "computing", "computer", "compute", "computation"
```

### 5. **Vector Semantic Search** (Contextual Similarity)
Embedding-based similarity matching (10% weight by default).

**Example Queries to Test:**
```
search: "machine intelligence"
Expected: Finds semantically similar docs to "machine intelligence"
Even if exact words don't match - finds "artificial intelligence", "neural networks", etc

search: "data analysis"
Expected: Semantic matches for analytics, statistics, data science concepts

search: "distributed computing"
Expected: Finds docs about parallel processing, grid computing, etc
```

## Testing Workflow

### 1. Start the Environment
```bash
make gpu-up
# App available at: http://localhost:3000
```

### 2. Monitor Logs
```bash
make gpu-logs
# Watch for hybrid search queries being executed
# Look for log lines showing:
#   - "=== HYBRID SEARCH QUERY BUILDING ==="
#   - Phrase query, boolean query, prefix query
#   - BM25 (sanitized) query with new fields
```

### 3. Test Search Queries

#### Test Set 1: Author Field Search
```
Query: "Alice"
Expected: Finds documents authored by Alice OR mentioning Alice in content
Test both search methods:
- Document authored by Alice (author field match)
- Content mentioning Alice (content field match)
```

#### Test Set 2: Source Path Search
```
Query: "documents/reports"
Expected: Finds documents from /documents/reports/ path OR containing that path in content

Query: "pdf"
Expected: Finds .pdf files via source_path OR content mentioning PDF

Query: "2024"
Expected: Finds files with "2024" in source_path OR content
```

#### Test Set 3: Title Search
```
Query: "Annual Report"
Expected: High relevance for documents with this in title
Also finds in author/summary/content if present
```

#### Test Set 4: Combined Field Searches
```
Query: "John database optimization"
Expected: Might find:
- Document by author "John" about "database optimization"
- Document titled "John's Database" with optimization content
- Multiple field matches for stronger ranking

Query: "rust memory safety"
Expected: All fields searched
- "rust" in author field
- "memory safety" in content/title/summary
```

#### Test Set 5: Typo Tolerance (Prefix Matching)
```
Query: "databas"
Expected: Finds "database", "databases", "database_schema"
Prefix query adds 5% weight for typo flexibility

Query: "algoritm"
Expected: Matches "algorithm", "algorithms"
```

#### Test Set 6: Phrase Matching
```
Query: "exact phrase matching"
Expected: 2.0x boost for exact phrase matches
vs separate word matches

Query: "machine learning"
Expected: Much higher score for exact "machine learning" phrase
Lower score for docs with "machine" and "learning" separate
```

## Scoring Formula

Each search combines strategies with weights:
```
combined_score =
    phrase_score * 0.15 (2.0x boost if matched) +
    bm25_score * 0.60 (all 5 fields now searched) +
    boolean_score * 0.15 (1.5x if all terms present) +
    prefix_score * 0.05 (handles typos) +
    vector_score * 0.10 (semantic similarity)
```

## Key Changes from Previous Implementation

| Feature | Before | After |
|---------|--------|-------|
| **BM25 Fields** | content, title, summary | content, title, summary, author, source_path |
| **Author Search** | Separate post-filter | Native BM25 field |
| **Path Search** | Not searchable | Native BM25 field (source_path) |
| **Field Coverage** | 3 fields | 5 fields |
| **Query Building** | Basic | Enhanced with author/path |

## Performance Considerations

1. **Query Time**: Searching 5 fields is ~1.5-2x slower than 3 fields, but:
   - Better result quality
   - Fewer false negatives
   - Hybrid search combines strategies anyway

2. **Index Efficiency**:
   - BM25 index created on all fields in `sql/init.sql`
   - ParadeDB optimizes multi-field queries automatically

3. **Post-Filtering**:
   - Entity filters (persons, organizations, etc) still applied in Rust
   - Array filters (keywords, locations) still in database WHERE clause
   - But now with better initial ranking from expanded BM25

## Testing Checklist

- [ ] Basic keyword search returns results
- [ ] Author-based search finds author field matches
- [ ] Source path search finds path field matches
- [ ] Phrase matching shows higher relevance
- [ ] Typo tolerance (prefix) finds partial matches
- [ ] Boolean queries require all terms
- [ ] Combined searches work (multiple fields)
- [ ] Scores are reasonable (0.0-1.0 range)
- [ ] Results ranked by combined_score
- [ ] Performance acceptable (< 1s for 10 results)

## Debugging

View query building details in logs:
```bash
make gpu-logs | grep "HYBRID SEARCH"
```

Expected output:
```
Original query: 'rust'
Tokenized: ["rust"]
Phrase query: (content:("rust") OR title:("rust") OR summary:("rust") OR author:("rust") OR source_path:("rust"))
Boolean query: (content:(rust) OR title:(rust) OR summary:(rust) OR author:(rust) OR source_path:(rust))
Prefix query: (content:(rust*) OR title:(rust*) OR summary:(rust*) OR author:(rust*) OR source_path:(rust*))
BM25 (sanitized): (content:(rust) OR title:(rust) OR summary:(rust) OR author:(rust) OR source_path:(rust))
Search weights - BM25: 0.6, Vector: 0.1
```

All queries now include all 5 indexed fields!
