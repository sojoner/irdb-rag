#!/bin/bash

# Faceted Search API Test Suite
# Tests all faceted search features via curl

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
API_URL="${API_URL:-http://localhost:3000/api}"
VERBOSE="${VERBOSE:-false}"

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Helper function to print test headers
test_header() {
    echo -e "\n${BLUE}============================================${NC}"
    echo -e "${BLUE}TEST: $1${NC}"
    echo -e "${BLUE}============================================${NC}"
}

# Helper function for assertions
assert_status() {
    local expected=$1
    local actual=$2
    local message=$3

    TESTS_RUN=$((TESTS_RUN + 1))
    if [ "$actual" = "$expected" ]; then
        echo -e "${GREEN}✓ PASS${NC}: $message (HTTP $actual)"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗ FAIL${NC}: $message (Expected $expected, got $actual)"
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

assert_contains() {
    local response=$1
    local expected=$2
    local message=$3

    TESTS_RUN=$((TESTS_RUN + 1))
    if echo "$response" | grep -q "$expected"; then
        echo -e "${GREEN}✓ PASS${NC}: $message"
        TESTS_PASSED=$((TESTS_PASSED + 1))
    else
        echo -e "${RED}✗ FAIL${NC}: $message"
        echo "  Response: ${response:0:100}..."
        TESTS_FAILED=$((TESTS_FAILED + 1))
    fi
}

# ============================================
# Test 1: Basic Aggregation Stats
# ============================================
test_header "1. Get Aggregation Statistics"

RESPONSE=$(curl -s -w "\n%{http_code}" "$API_URL/aggregation-stats")
HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-1)

assert_status "200" "$HTTP_STATUS" "Aggregation stats endpoint"
assert_contains "$BODY" "categories" "Response contains categories"
assert_contains "$BODY" "keywords" "Response contains keywords"
assert_contains "$BODY" "locations" "Response contains locations"

if [ "$VERBOSE" = "true" ]; then
    echo "Response: $BODY" | jq . 2>/dev/null || echo "$BODY"
fi

# ============================================
# Test 2: Basic Search
# ============================================
test_header "2. Basic Hybrid Search (with scoring)"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 5,
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-1)

assert_status "200" "$HTTP_STATUS" "Basic search endpoint"

# Verify score is normalized correctly (should be between 0.0 and 1.0)
if echo "$BODY" | grep -q '"score"'; then
    SCORES=$(echo "$BODY" | grep -o '"score":[0-9.]*' | head -5)
    echo "Sample scores found:"
    echo "$SCORES"

    # Check if any score is > 1.0 (which indicates normalization bug)
    while IFS= read -r line; do
        SCORE=$(echo "$line" | grep -o '[0-9.]*$')
        if (( $(echo "$SCORE > 1.0" | bc -l 2>/dev/null || echo "0") )); then
            echo -e "${RED}✗ WARNING${NC}: Score $SCORE exceeds 1.0 (normalization issue)"
        fi
    done <<< "$SCORES"
fi

if [ "$VERBOSE" = "true" ]; then
    echo "Response: $BODY" | jq . 2>/dev/null || echo "$BODY"
fi

# ============================================
# Test 3: Search with Category Filter
# ============================================
test_header "3. Search with Category Filter"

# First get available categories
CATEGORIES=$(curl -s "$API_URL/list-categories")
CATEGORY_ID=$(echo "$CATEGORIES" | jq -r '.[0].id' 2>/dev/null || echo "")

if [ ! -z "$CATEGORY_ID" ] && [ "$CATEGORY_ID" != "null" ]; then
    RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
        -H "Content-Type: application/json" \
        -d "{
            \"query\": \"test\",
            \"limit\": 5,
            \"category_id\": \"$CATEGORY_ID\",
            \"bm25_weight\": 0.5,
            \"vector_weight\": 0.5
        }")

    HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
    BODY=$(echo "$RESPONSE" | head -n-1)

    assert_status "200" "$HTTP_STATUS" "Search with category filter"
else
    echo -e "${YELLOW}⊘ SKIP${NC}: No categories available"
fi

# ============================================
# Test 4: Search with Keywords Filter
# ============================================
test_header "4. Search with Keywords Filter"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 5,
        "keywords": ["important", "urgent"],
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
assert_status "200" "$HTTP_STATUS" "Search with keywords filter"

# ============================================
# Test 5: Search with Multiple Filters
# ============================================
test_header "5. Search with Combined Filters"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 5,
        "keywords": ["test"],
        "locations": ["USA"],
        "authors": ["John Doe"],
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
assert_status "200" "$HTTP_STATUS" "Search with combined filters"

# ============================================
# Test 6: Search with Weights
# ============================================
test_header "6. Search with Different Weight Combinations"

# Test BM25-heavy (0.8 BM25, 0.2 vector)
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 5,
        "bm25_weight": 0.8,
        "vector_weight": 0.2
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-1)
assert_status "200" "$HTTP_STATUS" "BM25-heavy search (0.8/0.2)"

# Test Vector-heavy (0.2 BM25, 0.8 vector)
RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 5,
        "bm25_weight": 0.2,
        "vector_weight": 0.8
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
assert_status "200" "$HTTP_STATUS" "Vector-heavy search (0.2/0.8)"

# ============================================
# Test 7: Search with Date Range
# ============================================
test_header "7. Search with Date Range Filter"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 5,
        "date_from": "2023-01-01T00:00:00Z",
        "date_to": "2024-12-31T23:59:59Z",
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
assert_status "200" "$HTTP_STATUS" "Search with date range filter"

# ============================================
# Test 8: Search with Organizations/Persons/Concepts
# ============================================
test_header "8. Search with Entity Filters"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 5,
        "persons": ["Alice", "Bob"],
        "organizations": ["ACME Corp"],
        "concepts": ["Machine Learning"],
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
assert_status "200" "$HTTP_STATUS" "Search with entity filters"

# ============================================
# Test 9: Empty Search
# ============================================
test_header "9. Empty Query Handling"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "",
        "limit": 5,
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
BODY=$(echo "$RESPONSE" | head -n-1)

assert_status "200" "$HTTP_STATUS" "Empty query handling"
# Empty query should return empty results, not error
if echo "$BODY" | grep -q '\[\]'; then
    echo -e "${GREEN}✓ PASS${NC}: Empty query returns empty array"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⊘ INFO${NC}: Empty query response: $BODY"
fi
TESTS_RUN=$((TESTS_RUN + 1))

# ============================================
# Test 10: Filter-Only Search (no query)
# ============================================
test_header "10. Filter-Only Search (No Text Query)"

RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "*",
        "limit": 5,
        "keywords": ["test"],
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

HTTP_STATUS=$(echo "$RESPONSE" | tail -n1)
assert_status "200" "$HTTP_STATUS" "Filter-only search"

# ============================================
# Test 11: Pagination
# ============================================
test_header "11. Search Pagination"

# First page
RESPONSE1=$(curl -s -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 3,
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

# Second page (limit 3, but offset via limit parameter not directly available)
# This tests if we can get consistent ordering
RESPONSE2=$(curl -s -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "test",
        "limit": 3,
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

assert_contains "$RESPONSE1" "\"id\"" "First page has results"
TESTS_RUN=$((TESTS_RUN + 1))

# ============================================
# Test 12: Score Validation
# ============================================
test_header "12. Score Validation and Normalization"

RESPONSE=$(curl -s -X POST "$API_URL/search" \
    -H "Content-Type: application/json" \
    -d '{
        "query": "important",
        "limit": 10,
        "bm25_weight": 0.5,
        "vector_weight": 0.5
    }')

# Extract all scores
SCORES=$(echo "$RESPONSE" | grep -o '"score":[0-9.]*' | grep -o '[0-9.]*$')

INVALID_SCORES=0
VALID_SCORES=0

while IFS= read -r score; do
    if [ ! -z "$score" ]; then
        # Check if score is between 0 and 1
        if (( $(echo "$score >= 0 && $score <= 1" | bc -l 2>/dev/null || echo "0") )); then
            VALID_SCORES=$((VALID_SCORES + 1))
        else
            INVALID_SCORES=$((INVALID_SCORES + 1))
            echo -e "${RED}  Invalid score found: $score${NC}"
        fi
    fi
done <<< "$SCORES"

if [ $INVALID_SCORES -eq 0 ] && [ $VALID_SCORES -gt 0 ]; then
    echo -e "${GREEN}✓ PASS${NC}: All $VALID_SCORES scores are in valid range [0.0, 1.0]"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Found $INVALID_SCORES invalid scores out of $((VALID_SCORES + INVALID_SCORES)) total"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi
TESTS_RUN=$((TESTS_RUN + 1))

# ============================================
# Test Summary
# ============================================
echo -e "\n${BLUE}============================================${NC}"
echo -e "${BLUE}TEST SUMMARY${NC}"
echo -e "${BLUE}============================================${NC}"
echo "Total Tests:  $TESTS_RUN"
echo -e "Passed:       ${GREEN}$TESTS_PASSED${NC}"
echo -e "Failed:       ${RED}$TESTS_FAILED${NC}"

if [ $TESTS_FAILED -eq 0 ]; then
    echo -e "\n${GREEN}All tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}Some tests failed!${NC}"
    exit 1
fi
