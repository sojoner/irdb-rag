# Faceted Search Implementation Guide

This document describes the complete faceted search implementation in IRDB-RAG with all available features and how to use them.

## Overview

Faceted search allows users to:
1. Search documents with text queries
2. Apply multiple filters simultaneously
3. View facet aggregations (counts of values for each facet type)
4. Refine searches using facet-based navigation
5. Get normalized relevance scores (0.0 - 1.0 range)

## Facet Types

The system supports the following facet types:

| Facet Type | Description | Source |
|---|---|---|
| `category` | Document categories | `documents.category_id` → `categories.name` |
| `keyword` | Document keywords/tags | `documents.keywords[]` |
| `location` | Geographic locations | `documents.locations[]` |
| `author` | Document authors | `documents.author` |
| `person` | Named persons/entities | `documents.entities->'persons'` (JSONB) |
| `organization` | Named organizations | `documents.entities->'organizations'` (JSONB) |
| `product` | Products mentioned | `documents.entities->'products'` (JSONB) |
| `concept` | Concepts/topics | `documents.entities->'concepts'` (JSONB) |

## API Endpoints

### 1. Basic Search (with scoring)
```
POST /api/search
Content-Type: application/json

{
  "query": "machine learning",
  "limit": 10,
  "bm25_weight": 0.5,
  "vector_weight": 0.5,
  "category_id": "optional-uuid",
  "keywords": ["tag1", "tag2"],
  "locations": ["USA", "Canada"],
  "authors": ["John Doe"],
  "persons": ["Alice Smith"],
  "organizations": ["ACME Corp"],
  "concepts": ["Neural Networks"],
  "products": ["GPU"],
  "date_from": "2023-01-01T00:00:00Z",
  "date_to": "2024-12-31T23:59:59Z"
}
```

**Response:**
```json
[
  {
    "id": "uuid-string",
    "title": "Document Title",
    "content": "Document content preview...",
    "source_path": "/path/to/document",
    "category_name": "Technology",
    "score": 0.85,
    "bm25_score": 0.75,
    "vector_score": 0.92
  }
]
```

### 2. Faceted Search (with facet aggregations)
```
POST /api/search/faceted
Content-Type: application/json

{
  "query": "machine learning",
  "limit": 10,
  "facet_limit": 10,
  "bm25_weight": 0.5,
  "vector_weight": 0.5,
  "keywords": ["important"],
  "locations": ["USA"]
}
```

**Response:**
```json
{
  "results": [
    {
      "id": "uuid",
      "title": "Document Title",
      "content": "...",
      "source_path": "/path",
      "category_name": "Technology",
      "score": 0.85,
      "bm25_score": 0.75,
      "vector_score": 0.92
    }
  ],
  "facets": [
    {
      "facet_name": "category",
      "facet_value": "Technology",
      "count": 5
    },
    {
      "facet_name": "keyword",
      "facet_value": "important",
      "count": 3
    },
    {
      "facet_name": "location",
      "facet_value": "USA",
      "count": 8
    }
  ],
  "total_results": 42
}
```

### 3. Get Facet Values for Specific Type
```
POST /api/facets/values
Content-Type: application/json

{
  "facet_type": "category",
  "query": "machine learning",
  "limit": 20,
  "category_id": "optional-uuid",
  "keywords": ["filter-by-keyword"]
}
```

**Response:**
```json
{
  "facet_type": "category",
  "values": [
    {
      "value": "Technology",
      "count": 45
    },
    {
      "value": "Science",
      "count": 23
    },
    {
      "value": "Business",
      "count": 12
    }
  ]
}
```

### 4. Get Aggregation Statistics
```
GET /api/aggregation-stats
```

Returns counts for all facet types across all documents (no filtering).

**Response:**
```json
{
  "categories": [["Technology", 100], ["Science", 45], ...],
  "keywords": [["important", 78], ["urgent", 45], ...],
  "locations": [["USA", 234], ["UK", 123], ...],
  "persons": [["John Doe", 45], ["Jane Smith", 38], ...],
  "organizations": [["ACME", 56], ["TechCorp", 34], ...],
  "products": [["Processor", 23], ["GPU", 18], ...],
  "concepts": [["ML", 234], ["AI", 189], ...],
  "authors": [["Doc Author", 45], ["Writer", 23], ...],
  "word_count_ranges": []
}
```

## Scoring System

### Score Normalization Fix

**Problem:** Previous implementation produced scores > 1.0 (288%, 386%, etc.) when displaying.

**Solution:** Applied `LEAST(1.0, combined_score)` in SQL to cap scores at 1.0 maximum.

### Score Calculation

The hybrid search uses Reciprocal Rank Fusion (RRF) with weighted combination:

```
individual_score(rank) = 1.0 / (60 + rank)  // RRF formula, gives [0, 0.0167)
combined_score = LEAST(1.0,
  phrase_score * 0.15 +
  bm25_score * bm25_weight +
  boolean_score * 0.15 +
  prefix_score * 0.05 +
  vector_score * vector_weight
)
```

### Weight Parameters

- `bm25_weight`: Controls lexical (BM25) relevance (typical: 0.5)
- `vector_weight`: Controls semantic (embedding) relevance (typical: 0.5)

Both should sum to ≤ 1.0 to ensure normalized scores:
- `0.5 + 0.5 = 1.0` (balanced)
- `0.7 + 0.3` (BM25-heavy)
- `0.3 + 0.7` (vector-heavy)

### Display Format

Frontend converts to percentage: `score * 100.0`

- Score 0.85 → displays as "85%"
- Score 0.50 → displays as "50%"
- Score 0.01 → displays as "1%"

All scores are now guaranteed to be in range [0.0, 1.0].

## Filter Combinations

Filters use **AND logic** - all matching filters must match:

```javascript
{
  "query": "test",
  "keywords": ["important", "urgent"],  // AND: has BOTH keywords
  "locations": ["USA", "Canada"],        // AND: in USA OR Canada (OR within array)
  "authors": ["John"],                   // AND: written by John
  "category_id": "uuid"                  // AND: in this category
}
```

## SQL Functions

### `get_facet_aggregations()`
```sql
SELECT facet_name, facet_value, count
FROM get_facet_aggregations(
  query_text := 'search term',
  query_embedding := NULL,
  filter_category_id := NULL,
  filter_date_from := NULL,
  filter_date_to := NULL,
  filter_locations := ARRAY['USA'],
  filter_keywords := ARRAY['important'],
  filter_authors := ARRAY['John']
)
```

### `get_facet_values()`
```sql
SELECT value, count, selected
FROM get_facet_values(
  facet_type := 'category',
  query_text := 'search term',
  filter_category_id := NULL,
  filter_date_from := NULL,
  filter_date_to := NULL,
  filter_locations := ARRAY['USA'],
  filter_keywords := ARRAY['important'],
  filter_authors := ARRAY['John'],
  limit_results := 20
)
```

### `search_with_facets()`
```sql
SELECT result_type, id, title, content, source_path, category_name,
       bm25_score, vector_score, combined_score,
       facet_name, facet_value, facet_count
FROM search_with_facets(
  query_text := 'search term',
  query_embedding := NULL,
  match_count := 10,
  bm25_weight := 0.5,
  vector_weight := 0.5,
  filter_category_id := NULL,
  filter_date_from := NULL,
  filter_date_to := NULL,
  filter_locations := ARRAY['USA'],
  filter_keywords := ARRAY['important'],
  filter_authors := ARRAY['John'],
  facet_limit := 10
)
```

Returns mixed result set with `result_type` = 'result' or 'facet'

## Testing

### Run Curl Test Suite

```bash
# Make test script executable
chmod +x tests/faceted_search.sh

# Run with verbose output
VERBOSE=true bash tests/faceted_search.sh

# Or just run
bash tests/faceted_search.sh
```

### Individual Test Examples

#### Test 1: Get Aggregation Stats
```bash
curl -s http://localhost:3000/api/aggregation-stats | jq .
```

#### Test 2: Basic Search
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "machine learning",
    "limit": 5,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

#### Test 3: Search with Keywords Filter
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "test",
    "keywords": ["important"],
    "limit": 5,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

#### Test 4: Faceted Search
```bash
curl -s -X POST http://localhost:3000/api/search/faceted \
  -H "Content-Type: application/json" \
  -d '{
    "query": "important",
    "limit": 10,
    "facet_limit": 10,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

#### Test 5: Get Facet Values
```bash
curl -s -X POST http://localhost:3000/api/facets/values \
  -H "Content-Type: application/json" \
  -d '{
    "facet_type": "category",
    "query": "test",
    "limit": 20
  }' | jq .
```

## Rust API Usage

### Basic Search with Scoring
```rust
let filters = db::SearchFilters {
    category_id: None,
    date_from: None,
    date_to: None,
    locations: None,
    keywords: None,
    source_types: None,
    authors: None,
    concepts: None,
    organizations: None,
    persons: None,
    products: None,
    word_count_min: None,
    word_count_max: None,
};

let results = db::hybrid_search(
    &pool,
    "test query",
    &embedding_vector,
    &filters,
    10,  // limit
    0.5, // bm25_weight
    0.5, // vector_weight
    None, // reranker
).await?;

// Results now have normalized scores [0.0, 1.0]
for result in results {
    println!("{}: {} (score: {:.1}%)",
        result.id, result.title, result.score * 100.0);
}
```

### Faceted Search
```rust
let (results, facets) = db::search_with_facets(
    &pool,
    "machine learning",
    &embedding,
    &filters,
    10,  // limit
    0.5, // bm25_weight
    0.5, // vector_weight
    10,  // facet_limit
    None, // reranker
).await?;

println!("Results: {}", results.len());
println!("Facets:");
for facet in facets {
    println!("  {}: {} ({})",
        facet.facet_name, facet.facet_value, facet.count);
}
```

### Get Facet Values
```rust
let facet_values = db::get_facet_values(
    &pool,
    "category",
    Some("search term"),
    &filters,
    20  // limit
).await?;

for value in facet_values {
    println!("{}: {}", value.value, value.count);
}
```

## Key Files

| File | Purpose |
|---|---|
| [sql/init.sql](sql/init.sql) | SQL schema with faceted search functions |
| [src/infra/db.rs](src/infra/db.rs) | Database layer with faceted search functions |
| [src/domain/dtos.rs](src/domain/dtos.rs) | Faceted search request/response types |
| [src/api/handlers.rs](src/api/handlers.rs) | Faceted search API handlers |
| [src/api/routes.rs](src/api/routes.rs) | Faceted search route definitions |
| [tests/faceted_search.sh](tests/faceted_search.sh) | Curl test suite |

## Troubleshooting

### Scores Still > 1.0

Make sure database was reinitialized with the updated schema:

```bash
make gpu-up && make test-db-reset
```

The `LEAST(1.0, ...)` normalization in SQL requires database recreation.

### No Facet Results

Check that:
1. Documents have the required fields (keywords, locations, entities JSONB)
2. At least one document matches the current filters
3. `facet_limit` is > 0

### Query Matches Nothing

Try:
1. Simplifying the query
2. Removing or loosening filters
3. Checking database has indexed documents

## Performance Considerations

### Index Strategy

- **BM25**: ParadeDB index on `content`, `title`, `summary`
- **Vector**: HNSW index on `embedding` column
- **Array Fields**: GIN indexes on `keywords`, `locations`
- **Entities**: Functional indexes for JSONB array expansion

### Query Optimization

Faceted search performs:
1. **Result query**: Hybrid search (BM25 + vector)
2. **Facet query**: UNNEST + GROUP BY for array fields, JSONB expansion for entities

For large document sets, consider:
- Limiting `facet_limit` (10-20 is typical)
- Using more restrictive filters before faceting
- Caching facet aggregations if query pattern is fixed

## Future Enhancements

- [ ] Range facets (price ranges, date ranges)
- [ ] Hierarchical facets (category > subcategory)
- [ ] Facet drilling (drill down from summary facets)
- [ ] Facet exclusion (NOT filters)
- [ ] Custom facet definitions
- [ ] Facet analytics (popular searches, common filter combinations)
