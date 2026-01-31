# Advanced Query Builder Integration Guide

## Overview

The Advanced Query Builder provides a flexible UI for building complex search queries with multiple filters. It supports:

- **Date Range Filters**: With quick shortcuts (1 Day, Last Week, Last Month, Last Year) and custom date ranges
- **Text Field Filters**: For title, content, summary, and author fields with typeahead search
- **Array Field Filters**: For keywords, locations, persons, organizations, products, and concepts with dropdown selection

## Component Location

`src/web_app/components/advanced_query_builder.rs`

## Basic Usage

```rust
use crate::web_app::components::advanced_query_builder::AdvancedQueryBuilder;

#[component]
fn MySearchPage() -> impl IntoView {
    let (filters, set_filters) = signal(Vec::new());

    let handle_filter_change = move |new_filters: Vec<QueryFilter>| {
        set_filters.set(new_filters);
        // Now convert filters to SearchRequest and execute search
    };

    view! {
        <AdvancedQueryBuilder on_filter_change=handle_filter_change />
    }
}
```

## Converting Filters to SearchRequest

The `QueryFilter` objects need to be converted to the `SearchRequest` DTO which the API expects. Here's a conversion helper:

```rust
use crate::domain::dtos::SearchRequest;
use crate::web_app::components::advanced_query_builder::{QueryFilter, FilterType, FilterValue};

fn filters_to_search_request(
    query: String,
    filters: Vec<QueryFilter>,
) -> SearchRequest {
    let mut req = SearchRequest::default();
    req.query = query;

    for filter in filters {
        match filter.value {
            FilterValue::DateRange { from, to } => {
                req.date_from = from;
                req.date_to = to;
            }
            FilterValue::Text { field, value } => {
                // For text filters, we could either:
                // 1. Add to query as field:value syntax
                // 2. Store separately for server-side filtering
                // For now, append to query
                if !value.is_empty() {
                    req.query.push_str(&format!(" {}: {}", field, value));
                }
            }
            FilterValue::Array { field, values } => {
                match field.as_str() {
                    "keywords" => req.keywords = Some(values),
                    "locations" => req.locations = Some(values),
                    "persons" => req.persons = Some(values),
                    "organizations" => req.organizations = Some(values),
                    "products" => req.products = Some(values),
                    "concepts" => req.concepts = Some(values),
                    _ => {}
                }
            }
        }
    }

    req
}
```

## Integration with SearchPage

Add the Advanced Query Builder to the search page:

```rust
// In src/web_app/pages/search.rs

use crate::web_app::components::advanced_query_builder::{
    AdvancedQueryBuilder, QueryFilter, FilterType, FilterValue
};

#[component]
pub fn SearchPage() -> impl IntoView {
    // ... existing state ...
    let (query_filters, set_query_filters) = signal(Vec::<QueryFilter>::new());

    // ... existing search function ...

    let handle_filters_change = move |filters: Vec<QueryFilter>| {
        set_query_filters.set(filters);
        execute_search(());
    };

    view! {
        <div class="search-layout">
            <UnifiedSearch ... />

            <div class="filter-panel">
                <FacetedFilters ... />

                // New Advanced Query Builder
                <AdvancedQueryBuilder on_filter_change=handle_filters_change />
            </div>

            <div class="results">
                <ResultsList ... />
            </div>
        </div>
    }
}
```

## Field Type Reference

### Text Fields (with typeahead)
- `title` - Document title
- `content` - Full document content
- `summary` - Document summary/abstract
- `author` - Document author

### Array Fields (with dropdown selection)
- `keywords` - Document keywords
- `locations` - Geographic locations mentioned in document
- `persons` - Person names extracted from document
- `organizations` - Organization names extracted from document
- `products` - Product names mentioned in document
- `concepts` - Concepts/topics extracted from document

### Date Range
- Quick presets: 1 Day, Last Week, Last Month, Last Year
- Custom range: YYYY-MM-DD format

## Type Definitions

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub filter_type: FilterType,
    pub value: FilterValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterType {
    DateRange,
    TextField(String),       // field name
    ArrayField(String),      // field name
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FilterValue {
    DateRange {
        from: Option<String>,
        to: Option<String>,
    },
    Text {
        field: String,
        value: String,
    },
    Array {
        field: String,
        values: Vec<String>,
    },
}
```

## Server-Side Implementation

The API already supports these filters in SearchRequest:

```rust
pub struct SearchRequest {
    pub query: String,
    pub date_from: Option<String>,    // YYYY-MM-DD format
    pub date_to: Option<String>,      // YYYY-MM-DD format
    pub locations: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub organizations: Option<Vec<String>>,
    pub persons: Option<Vec<String>>,
    pub products: Option<Vec<String>>,
    pub word_count_min: Option<i32>,
    pub word_count_max: Option<i32>,
    // ... other fields
}
```

The `hybrid_search` function in `src/infra/db.rs` already supports these filters with efficient PostgreSQL queries.

## Future Enhancements

1. **Real Typeahead Suggestions**
   - Connect to server endpoint to fetch actual suggestions from document data
   - Current: Mock suggestions (keywords, locations, etc.)

2. **Custom Field Extensions**
   - Support for metadata fields and custom entity types
   - Dynamic field discovery from database schema

3. **Saved Queries**
   - Save filter combinations for later use
   - Query templates for common searches

4. **Filter Logic UI**
   - AND/OR operator selection between filters
   - Parentheses for complex boolean logic

5. **Performance**
   - Debounce typeahead requests
   - Batch API calls for filter suggestions
   - Cache frequently used suggestions
