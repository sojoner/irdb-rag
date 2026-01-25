# Dynamic Query Builder Refactoring Plan

## Goal
Transform the static, hardcoded filter UI into a Kibana-like dynamic search query builder that:
- Searches the metadata keyspace (dynamic fields like Persons, Categories, Keywords, Locations, etc.)
- Provides autocomplete for both field names AND field values
- Allows complex AND/OR/NOT boolean query composition
- Leverages full pg_search (ParadeDB BM25) capabilities
- Maintains simplicity as primary goal

## Current State Analysis

### Existing Architecture
- **Static filters**: Hardcoded 7 facet types (categories, keywords, concepts, locations, persons, organizations, authors)
- **UI**: `FacetedFilters` component with fixed signal props and toggle handlers
- **API**: `SearchRequest` DTO with hardcoded filter fields
- **Database**: `SearchFilters` struct with hardcoded fields, dedicated columns for each facet
- **Aggregation**: `get_aggregation_stats()` server fn loads all facets at once

### Problems to Solve
1. **No metadata discovery**: Can't search across unknown field names
2. **No field-level autocomplete**: User must scroll pre-loaded lists
3. **No value filtering**: Autocomplete for values hardcoded per field type
4. **No query visualization**: User builds query but doesn't see the SQL equivalent
5. **Tight coupling**: Frontend signal structure mirrors backend filter struct
6. **Poor scalability**: Adding new facet requires changes in 4+ files

## Three-Phase Implementation Plan

### Phase 1: Metadata Discovery & API (Foundation)
**Goal**: Enable backend to expose schema metadata dynamically

#### 1.1 Create Metadata Schema
**File**: `src/domain/models.rs`
```rust
// Add to existing file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldMetadata {
    pub name: String,                    // "persons", "locations", etc.
    pub display_name: String,            // "People", "Places"
    pub field_type: FieldType,           // Text, Number, Date, etc.
    pub total_unique_values: i64,        // For UI: "234 unique values"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    Text,
    Number { min: Option<f64>, max: Option<f64> },
    Date { min: Option<String>, max: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldValueAutocomplete {
    pub field: String,
    pub query: String,                   // Partial text for matching
    pub values: Vec<(String, i64)>,      // (value, doc_count)
    pub total_matching: i64,
}
```

#### 1.2 Create Metadata Service
**File**: `src/infra/metadata.rs` (new)
```rust
// Functions:
pub async fn discover_fields(pool: &PgPool) -> Result<Vec<FieldMetadata>>;
pub async fn discover_field_values(
    pool: &PgPool,
    field: &str,
    query: &str,     // Partial text for autocomplete
    limit: usize,
) -> Result<Vec<(String, i64)>>;
```

SQL queries:
```sql
-- Discover fields from JSONB metadata
SELECT
    jsonb_object_keys(metadata) AS field_name,
    COUNT(DISTINCT metadata->field_name) AS unique_count
FROM documents
WHERE metadata IS NOT NULL
GROUP BY field_name
ORDER BY field_name;

-- Autocomplete for field values (BM25 on JSONB values)
SELECT
    metadata->>'field_name' AS value,
    COUNT(*) AS doc_count
FROM documents
WHERE metadata->>'field_name' @@@ $1  -- BM25 search
GROUP BY value
ORDER BY doc_count DESC
LIMIT $2;
```

#### 1.3 API Endpoints
**File**: `src/api/handlers.rs`
```rust
pub async fn get_field_metadata(State(state): State<AppState>) -> Result<Json<Vec<FieldMetadata>>>;

pub async fn get_field_values(
    State(state): State<AppState>,
    Json(req): Json<FieldValueRequest>,
) -> Result<Json<FieldValueAutocomplete>>;
```

**DTO**: Add to `src/domain/dtos.rs`
```rust
#[derive(Debug, Deserialize)]
pub struct FieldValueRequest {
    pub field: String,
    pub query: String,
    pub limit: usize,
}
```

#### 1.4 Routes
**File**: `src/api/routes.rs`
```rust
router
    .route("/api/metadata/fields", get(get_field_metadata))
    .route("/api/metadata/field-values", post(get_field_values))
```

#### 1.5 Tests
**File**: `tests/metadata_discovery_test.rs` (new)
- Test field discovery returns all metadata keys
- Test value autocomplete filters correctly
- Test value autocomplete limits results
- Test counts are accurate

### Phase 2: Dynamic Query Builder Component (UI)
**Goal**: Frontend component that can build complex queries interactively

#### 2.1 Query Builder Types
**File**: `src/web_app/types/query_builder.rs` (new)
```rust
// In Leptos/Rust FFI layer
#[derive(Debug, Clone)]
pub enum FilterCondition {
    And(Vec<FilterCondition>),
    Or(Vec<FilterCondition>),
    Not(Box<FilterCondition>),
    Equals { field: String, value: String },
    Contains { field: String, value: String },  // Text search
    Range { field: String, min: Option<f64>, max: Option<f64> },
    DateRange { field: String, min: Option<String>, max: Option<String> },
}

#[derive(Debug, Clone)]
pub struct QueryBuilderState {
    pub conditions: Vec<FilterCondition>,
    pub operator: LogicalOperator,  // AND or OR at root level
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogicalOperator {
    And,
    Or,
}
```

#### 2.2 Query Builder Component
**File**: `src/web_app/components/query_builder.rs` (new)

Features:
- **Field selector dropdown**: Autocomplete from metadata API
- **Operator selector**: =, contains, >, <, >=, <=, between
- **Value input with autocomplete**: Fetches values for selected field
- **Add/Remove conditions**: Manage multiple rules
- **Group nesting**: AND/OR groups with parentheses visualization
- **Query preview**: Show SQL preview (non-interactive)

Structure:
```rust
#[component]
pub fn QueryBuilder(
    #[prop(into)] on_query_change: Callback<FilterCondition>,
) -> impl IntoView { ... }

#[component]
fn FilterRule(
    index: usize,
    #[prop(into)] on_change: Callback<FilterCondition>,
    #[prop(into)] on_remove: Callback<()>,
) -> impl IntoView { ... }

#[component]
fn FilterGroup(
    rules: Signal<Vec<FilterCondition>>,
    operator: Signal<LogicalOperator>,
    #[prop(into)] on_change: Callback<FilterCondition>,
) -> impl IntoView { ... }

#[component]
fn FieldAutocomplete(
    #[prop(into)] on_select: Callback<FieldMetadata>,
) -> impl IntoView { ... }

#[component]
fn ValueAutocomplete(
    field: FieldMetadata,
    #[prop(into)] on_select: Callback<String>,
) -> impl IntoView { ... }

#[component]
fn QueryPreview(
    condition: Signal<Option<FilterCondition>>,
) -> impl IntoView { ... }
```

#### 2.3 Leptos Server Functions
**File**: `src/web_app/components/query_builder.rs`

```rust
#[server(GetFieldMetadata, "/api")]
pub async fn get_field_metadata_client() -> Result<Vec<FieldMetadata>, ServerFnError> { ... }

#[server(GetFieldValues, "/api")]
pub async fn get_field_values_client(
    field: String,
    query: String,
    limit: usize,
) -> Result<Vec<(String, i64)>, ServerFnError> { ... }
```

#### 2.4 Tests
**File**: `tests/query_builder_test.rs` (new)
- Test component renders without errors
- Test field autocomplete filters options
- Test value autocomplete shows correct values for field
- Test adding/removing conditions updates state
- Test logical operator toggle (AND/OR) works

### Phase 3: Backend Query Execution (SQL Generation & Search)
**Goal**: Convert FilterCondition to SQL and execute against pg_search

#### 3.1 Query Compiler
**File**: `src/infra/query_compiler.rs` (new)

```rust
pub struct QueryCompiler;

impl QueryCompiler {
    pub fn compile_where_clause(condition: &FilterCondition) -> String {
        // Converts FilterCondition tree to SQL WHERE clause
        // Uses BM25 operators (|||, &&&, ~~) for text search
    }

    pub fn compile_with_metadata(condition: &FilterCondition) -> String {
        // Generates SQL for JSONB metadata searches
        // Example: metadata->>'persons' @@@ 'john'
    }
}
```

SQL pattern examples:
```sql
-- Text field contains (BM25)
WHERE metadata->>'persons' @@@ 'john'

-- AND operation
WHERE (metadata->>'persons' @@@ 'john') AND (metadata->>'locations' @@@ 'london')

-- OR operation
WHERE (metadata->>'persons' @@@ 'john') OR (metadata->>'persons' @@@ 'jane')

-- Negation
WHERE NOT (metadata->>'persons' @@@ 'john')

-- Number range (if stored as number)
WHERE (metadata->>'age')::numeric BETWEEN 25 AND 65

-- Nested grouping (parentheses)
WHERE (
    (metadata->>'persons' @@@ 'john' OR metadata->>'persons' @@@ 'jane')
    AND (metadata->>'locations' @@@ 'london')
)
```

#### 3.2 Update Search API
**File**: `src/api/handlers.rs`

Modify `search()` to accept either:
1. Legacy `SearchRequest` (for backward compat)
2. New `QueryBuilderSearch` with `FilterCondition`

```rust
#[derive(Debug, Deserialize)]
pub struct QueryBuilderSearch {
    pub query: String,                      // Free text query
    pub filter_condition: Option<FilterCondition>,  // Optional structured filters
    pub limit: usize,
    pub bm25_weight: f32,
    pub vector_weight: f32,
}

pub async fn search_with_query_builder(
    State(state): State<AppState>,
    Json(req): Json<QueryBuilderSearch>,
) -> Result<Json<Vec<SearchResult>>>;
```

#### 3.3 Tests
**File**: `tests/query_compilation_test.rs` (new)
- Test FilterCondition compiles to valid SQL
- Test BM25 operators used correctly
- Test AND/OR logic produces correct parentheses
- Test NOT negation works
- Test complex nested conditions
- Test actual queries execute without error

---

## Implementation Order (Recommended)

### Sprint 1: Foundation
1. **Domain models** (Phase 1.1)
   - Add FieldMetadata, FieldType, FieldValueAutocomplete
   - Add tests for models

2. **Metadata discovery SQL** (Phase 1.2)
   - Implement discover_fields query
   - Implement discover_field_values query
   - Add to db module
   - Test against actual database

3. **API endpoints** (Phase 1.3, 1.4, 1.5)
   - Add handlers to handlers.rs
   - Add routes
   - Manual curl testing

4. **Server functions** (Phase 2.3)
   - Leptos wrappers for API
   - Test in browser

### Sprint 2: UI
1. **Query builder types** (Phase 2.1)
   - Define FilterCondition, QueryBuilderState
   - No tests needed yet

2. **Basic component** (Phase 2.2)
   - Start with single rule (field + operator + value)
   - Field autocomplete working
   - Value autocomplete working
   - Add rule button

3. **Component iteration**
   - Add remove rule
   - Add AND/OR toggle
   - Add rule grouping (optional for MVP)
   - Query preview

### Sprint 3: Backend Execution
1. **Query compiler** (Phase 3.1)
   - FilterCondition → SQL WHERE
   - Comprehensive tests

2. **API integration** (Phase 3.2)
   - New search endpoint or extend existing
   - Backward compatibility

---

## Migration Strategy

### Keep existing code working:
- Leave `FacetedFilters` component untouched
- Add `QueryBuilder` as alternative
- Both can coexist on search page
- User selects mode via UI toggle

### When to migrate:
1. **Phase 1-2 complete**: New UI available, old UI still works
2. **Gather feedback**: See if users prefer new interface
3. **Phase 3 complete**: New UI has full search capability
4. **After validation**: Replace old UI, keep backward compat in API

---

## Key Design Decisions

### Why separate `metadata.rs`?
- **Modularity**: Metadata discovery is independent service
- **Testability**: Can test without full search setup
- **Reusability**: Query builder, faceted view, stats all use same API

### Why FilterCondition enum?
- **Type safety**: Prevents invalid condition combinations
- **Composability**: Easy to build complex queries recursively
- **Testability**: Can unit test condition tree

### Why compile to SQL string?
- **Flexibility**: Reuse PostgreSQL's own query optimizer
- **Performance**: Leverages pg_search indexes natively
- **Debuggability**: Users can inspect SQL

### Why keep FacetedFilters?
- **Backward compat**: Existing users not disrupted
- **Simpler UX**: For basic use cases, faceted view is faster
- **Progressive enhancement**: Advanced users use query builder

---

## Performance Considerations

### Metadata Discovery (once on page load)
- Field list: ~50-200ms (single query, cached in Leptos Resource)
- Field value autocomplete: Query on input (debounced 300ms)

### Field Value Autocomplete (per keystroke)
- Uses BM25 full-text index on metadata values
- Limit to 20 results to keep network fast
- Frontend debounces to 300ms input delay

### Search Execution
- Query compiler produces efficient SQL
- Reuses existing BM25/vector indexes
- No additional indexes needed (leverage JSONB operators)

### Future Optimization
- Cache metadata (revalidate daily)
- Index JSONB fields if large volume
- Add field-level statistics API

---

## Success Criteria

Phase 1 complete when:
- [ ] Metadata APIs return all available fields
- [ ] Field value autocomplete works
- [ ] No errors in logs

Phase 2 complete when:
- [ ] Single condition builder works
- [ ] Autocomplete on field and value both work
- [ ] Add/remove conditions work
- [ ] AND/OR toggle works

Phase 3 complete when:
- [ ] FilterCondition compiles to SQL
- [ ] Search queries using new format return results
- [ ] Results are correct (match hand-written SQL)
- [ ] Performance acceptable (< 1s for typical query)

---

## Future Enhancements (Not in MVP)

1. **Saved queries**: Save/load filter combinations
2. **Query suggestions**: "People who also searched for..."
3. **Field statistics widget**: Distribution, top values
4. **Visual query editor**: Drag-drop conditions
5. **Advanced operators**: fuzzy match, regex, levenshtein
6. **Query syntax**: Allow hand-written SQL-like syntax
7. **Reranking preview**: Show how reranker orders results

---

## Files to Create/Modify Summary

### New Files
- `src/domain/models.rs` - Add FieldMetadata, FieldType
- `src/infra/metadata.rs` - Field discovery service
- `src/infra/query_compiler.rs` - SQL generation
- `src/web_app/components/query_builder.rs` - UI component
- `src/web_app/types/query_builder.rs` - Types
- `tests/metadata_discovery_test.rs`
- `tests/query_compilation_test.rs`
- `tests/query_builder_test.rs`

### Modify Files
- `src/api/handlers.rs` - Add new endpoints
- `src/api/routes.rs` - Add new routes
- `src/domain/dtos.rs` - Add new DTOs
- `src/web_app/pages/search.rs` - Integrate new component
- `src/web_app/components/mod.rs` - Export new component

---

## Notes

- **CLAUDE.md compliance**: Follows SOLID principles, proper error handling, test-first approach
- **No breaking changes**: Existing code untouched, new functionality additive
- **Kibana inspiration**: Similar metadata keyspace search, but simpler UI
- **SQL focus**: Leverages pg_search/pgvector native capabilities
