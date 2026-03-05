# Faceted Search - Quick Reference & Examples

## Running the Test Suite

```bash
# Make script executable
chmod +x tests/faceted_search.sh

# Run all tests
bash tests/faceted_search.sh

# Run with verbose output to see actual responses
VERBOSE=true bash tests/faceted_search.sh

# Change API URL if not local
API_URL=http://production-server:3000/api bash tests/faceted_search.sh
```

## Quick Curl Examples

All examples assume API is running at `http://localhost:3000/api`

### 1. Get Statistics (no search)
```bash
curl -s http://localhost:3000/api/aggregation-stats | jq .
```

### 2. Simple Text Search
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

### 3. Search with Single Filter (Keywords)
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "neural networks",
    "keywords": ["important"],
    "limit": 10,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

### 4. Search with Multiple Filters
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "AI",
    "keywords": ["urgent", "research"],
    "locations": ["USA", "Canada"],
    "limit": 5,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

### 5. Faceted Search (with Facets)
```bash
curl -s -X POST http://localhost:3000/api/search/faceted \
  -H "Content-Type: application/json" \
  -d '{
    "query": "deep learning",
    "limit": 10,
    "facet_limit": 10,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

### 6. Get Category Facet Values
```bash
curl -s -X POST http://localhost:3000/api/facets/values \
  -H "Content-Type: application/json" \
  -d '{
    "facet_type": "category",
    "limit": 20
  }' | jq .
```

### 7. Get Keywords Facet Values
```bash
curl -s -X POST http://localhost:3000/api/facets/values \
  -H "Content-Type: application/json" \
  -d '{
    "facet_type": "keyword",
    "query": "machine learning",
    "limit": 15
  }' | jq .
```

### 8. Get Person/Organization/Concept Facets
```bash
# Persons
curl -s -X POST http://localhost:3000/api/facets/values \
  -H "Content-Type: application/json" \
  -d '{
    "facet_type": "person",
    "limit": 20
  }' | jq .

# Organizations
curl -s -X POST http://localhost:3000/api/facets/values \
  -H "Content-Type: application/json" \
  -d '{
    "facet_type": "organization",
    "limit": 20
  }' | jq .

# Concepts
curl -s -X POST http://localhost:3000/api/facets/values \
  -H "Content-Type: application/json" \
  -d '{
    "facet_type": "concept",
    "limit": 20
  }' | jq .
```

### 9. BM25-Heavy Search (80% lexical, 20% semantic)
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "exact phrase matching",
    "limit": 10,
    "bm25_weight": 0.8,
    "vector_weight": 0.2
  }' | jq .
```

### 10. Vector-Heavy Search (30% lexical, 70% semantic)
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "conceptual similarity",
    "limit": 10,
    "bm25_weight": 0.3,
    "vector_weight": 0.7
  }' | jq .
```

### 11. Filter-Only Search (No Text Query)
```bash
# Search by filters only (using "*" as query)
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "*",
    "keywords": ["important", "urgent"],
    "limit": 10,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

### 12. Date Range Filter
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "research",
    "date_from": "2023-01-01T00:00:00Z",
    "date_to": "2024-12-31T23:59:59Z",
    "limit": 10,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

### 13. Entity-Based Search (Persons, Organizations, etc)
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "research",
    "persons": ["Albert Einstein", "Marie Curie"],
    "organizations": ["MIT", "Stanford"],
    "concepts": ["Physics", "Chemistry"],
    "limit": 10,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq .
```

## Response Parsing Examples

### Extract Just the Scores
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":5,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq '.[] | {title, score: (.score * 100 | round | tostring + "%")}'
```

Output:
```json
{
  "title": "Document Title",
  "score": "85%"
}
```

### Extract Titles and IDs
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":5,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq '.[] | {id, title}'
```

### Count Results
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":100,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq 'length'
```

### Get Top Result
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":1,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq '.[0]'
```

### Extract Facet Counts from Faceted Search
```bash
curl -s -X POST http://localhost:3000/api/search/faceted \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":10,"facet_limit":10,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq '.facets | group_by(.facet_name) | map({(.[0].facet_name): .[]})'
```

### Filter Results by Score Threshold
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":20,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq '.[] | select(.score >= 0.7)'
```

## Shell Script Examples

### Save Results to File
```bash
#!/bin/bash
QUERY="machine learning"
OUTPUT_FILE="search_results.json"

curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"$QUERY\",
    \"limit\": 50,
    \"bm25_weight\": 0.5,
    \"vector_weight\": 0.5
  }" > "$OUTPUT_FILE"

echo "Saved $(jq 'length' "$OUTPUT_FILE") results to $OUTPUT_FILE"
```

### Compare BM25 vs Vector Weights
```bash
#!/bin/bash
QUERY="$1"

echo "=== BM25-Heavy (0.8/0.2) ==="
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"$QUERY\",
    \"limit\": 3,
    \"bm25_weight\": 0.8,
    \"vector_weight\": 0.2
  }" | jq '.[] | {title, score}'

echo -e "\n=== Balanced (0.5/0.5) ==="
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"$QUERY\",
    \"limit\": 3,
    \"bm25_weight\": 0.5,
    \"vector_weight\": 0.5
  }" | jq '.[] | {title, score}'

echo -e "\n=== Vector-Heavy (0.2/0.8) ==="
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d "{
    \"query\": \"$QUERY\",
    \"limit\": 3,
    \"bm25_weight\": 0.2,
    \"vector_weight\": 0.8
  }" | jq '.[] | {title, score}'
```

### Export Results to CSV
```bash
#!/bin/bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{
    "query": "test",
    "limit": 100,
    "bm25_weight": 0.5,
    "vector_weight": 0.5
  }' | jq -r '.[] | [.id, .title, .score * 100 | floor] | @csv' > results.csv

echo "Exported results to results.csv"
```

## Debugging Tips

### Enable Verbose Curl Output
```bash
curl -v -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":5,"bm25_weight":0.5,"vector_weight":0.5}'
```

### Pretty-Print with Custom Color
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":5,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq --color-output '.'
```

### Check Response Time
```bash
time curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":5,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq 'length'
```

### View Only Errors
```bash
curl -s -X POST http://localhost:3000/api/search \
  -H "Content-Type: application/json" \
  -d '{"query":"test","limit":5,"bm25_weight":0.5,"vector_weight":0.5}' \
  | jq '.error // "No errors"'
```

## Common Issues & Solutions

### "No results found"
Try:
```bash
# Simplify query
curl ... -d '{"query":"test","limit":10,"bm25_weight":0.5,"vector_weight":0.5}'

# Remove filters
curl ... -d '{"query":"test","limit":10,"bm25_weight":0.5,"vector_weight":0.5}'

# Check database has documents
curl http://localhost:3000/api/aggregation-stats | jq '.categories'
```

### "Scores seem wrong (>100%)"
This was fixed by adding `LEAST(1.0, score)` normalization.

If still seeing invalid scores, recreate database:
```bash
make test-db-reset
```

### "Slow queries"
Reduce limits and use more specific filters:
```bash
curl ... -d '{
  "query":"test",
  "limit":5,
  "facet_limit":5,
  "keywords":["important"],
  "bm25_weight":0.5,
  "vector_weight":0.5
}'
```

### "Empty facets"
Check that:
1. Query returns results
2. Documents have the facet fields (keywords, locations, entities)
3. facet_limit > 0
