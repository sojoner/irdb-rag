# Advanced Query Builder - Complete Implementation

## Overview

A comprehensive, user-friendly Advanced Query Builder UI component has been implemented for the IRDB-RAG search interface. It enables flexible filtering with three main filter types:

1. **Date Range Filters** - Quick presets + custom date ranges
2. **Text Field Filters** - Typeahead search on document fields
3. **Array/Facet Field Filters** - Multi-select dropdowns for entity filters

## Files Created

### Core Component
- `src/web_app/components/advanced_query_builder.rs` - Main component with all filter types
- `src/web_app/components/query_builder_example.rs` - Example implementation and integration helper

### Documentation
- `.claude/specs/advanced_query_builder_integration.md` - Detailed integration guide
- `.claude/specs/advanced_query_builder_README.md` - This file

### Updated Files
- `src/web_app/components/mod.rs` - Exports for new components

## Component Architecture

### Main Component: `AdvancedQueryBuilder`

```rust
#[component]
pub fn AdvancedQueryBuilder(
    on_filter_change: Callback<Vec<QueryFilter>>,
) -> impl IntoView
```

**Props:**
- `on_filter_change` - Callback that receives all active filters when they change

**Features:**
- Add/remove filter buttons (Date, Text Field, Array Field)
- Dynamic filter management with unique indices
- Immediate feedback on filter changes
- Clear visual separation between filter types

### Filter Type 1: Date Range Filter

**UI Components:**
- Quick preset buttons: "1 Day", "Last Week", "Last Month", "Last Year"
- Custom date range inputs: From (YYYY-MM-DD) and To (YYYY-MM-DD)
- Remove button

**Features:**
- One-click date range selection
- Auto-calculates date ranges from today backwards
- Manual override with date picker inputs
- Preset buttons highlight when selected

**Data Returned:**
```rust
FilterValue::DateRange {
    from: Option<String>,  // YYYY-MM-DD format
    to: Option<String>,    // YYYY-MM-DD format
}
```

### Filter Type 2: Text Field Filter

**UI Components:**
- Field selector dropdown: title, content, summary, author
- Search input with typeahead suggestions
- Remove button

**Features:**
- Field selection (4 core searchable fields)
- Typeahead dropdown shows matching suggestions
- Suggestions appear as user types
- Supports empty values

**Data Returned:**
```rust
FilterValue::Text {
    field: String,   // "title" | "content" | "summary" | "author"
    value: String,   // Search term
}
```

### Filter Type 3: Array Field Filter

**UI Components:**
- Field selector dropdown: keywords, locations, persons, organizations, products, concepts
- Search & select input with autocomplete dropdown
- Selected values displayed as removal-enabled badges
- Remove button for entire filter

**Features:**
- Field selection (6 array/facet fields)
- Autocomplete suggestions based on field type
- Multi-select with visual badges
- Individual value removal
- Prevents duplicate selections

**Data Returned:**
```rust
FilterValue::Array {
    field: String,         // Field name
    values: Vec<String>,   // Selected values
}
```

## Data Structure

### QueryFilter
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryFilter {
    pub filter_type: FilterType,
    pub value: FilterValue,
}
```

### FilterType Enum
```rust
pub enum FilterType {
    DateRange,              // Date range filter
    TextField(String),      // Text field: title, content, summary, author
    ArrayField(String),     // Array field: keywords, locations, etc.
}
```

### FilterValue Enum
```rust
pub enum FilterValue {
    DateRange { from: Option<String>, to: Option<String> },
    Text { field: String, value: String },
    Array { field: String, values: Vec<String> },
}
```

## Integration Guide

### Basic Integration

```rust
use crate::web_app::components::advanced_query_builder::AdvancedQueryBuilder;

#[component]
fn SearchPage() -> impl IntoView {
    let (filters, set_filters) = signal(Vec::new());

    let handle_filters = move |new_filters: Vec<QueryFilter>| {
        set_filters.set(new_filters);
        // Execute search with new filters
    };

    view! {
        <AdvancedQueryBuilder on_filter_change=handle_filters />
        // ... rest of search page
    }
}
```

### Converting Filters to SearchRequest

Use the helper function from `query_builder_example.rs`:

```rust
use crate::web_app::components::query_builder_example::build_search_request;

let search_request = build_search_request(
    query_string,  // User's search query
    filters,       // Vec<QueryFilter> from component
);

// search_request is now ready to send to API
```

### Example SearchRequest Output

For filters: "Last Week" date + "python" text in content + "keywords: ['AI', 'ML']"

```json
{
  "query": "content:\"python\"",
  "date_from": "2025-01-24",
  "date_to": "2025-01-31",
  "keywords": ["AI", "ML"],
  "limit": 20,
  "search_fields": ["content", "title", "summary"],
  "bm25_weight": 0.5,
  "vector_weight": 0.5
}
```

## Available Fields

### Text Fields (with typeahead search)
| Field | Purpose |
|-------|---------|
| `title` | Document title |
| `content` | Full document content |
| `summary` | Document summary/abstract |
| `author` | Document author |

### Array/Facet Fields (multi-select)
| Field | Purpose |
|-------|---------|
| `keywords` | Keywords extracted from document |
| `locations` | Geographic locations mentioned |
| `persons` | Person names mentioned |
| `organizations` | Organization names mentioned |
| `products` | Product names mentioned |
| `concepts` | High-level concepts/topics |

### Date Range
- From: YYYY-MM-DD format
- To: YYYY-MM-DD format
- Quick presets: 1 Day, Last Week, Last Month, Last Year

## Styling

All styles are inline Tailwind CSS classes. The component uses:
- Blue color scheme for buttons and highlights (#blue-600, #blue-500)
- Gray backgrounds for filters (#gray-50)
- Red for remove buttons (#red-600)
- Standard padding and spacing from Tailwind

### Key CSS Classes Used
- `bg-white` - Main background
- `border border-gray-200` - Borders
- `rounded` - Border radius
- `shadow-sm` - Subtle shadow
- `px-3 py-2` - Padding
- `text-xs font-medium` - Typography

## Example Component

See `src/web_app/components/query_builder_example.rs` for a complete example page that:

1. Shows the Advanced Query Builder component
2. Demonstrates filter conversion to SearchRequest
3. Includes debug output showing generated SearchRequest
4. Provides integration instructions

## Server-Side Support

The API already supports all these filters in `SearchRequest`:

```rust
pub struct SearchRequest {
    pub query: String,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub locations: Option<Vec<String>>,
    pub keywords: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
    pub concepts: Option<Vec<String>>,
    pub organizations: Option<Vec<String>>,
    pub persons: Option<Vec<String>>,
    pub products: Option<Vec<String>>,
    pub word_count_min: Option<i32>,
    pub word_count_max: Option<i32>,
}
```

The `hybrid_search` function in the database layer efficiently filters using PostgreSQL with proper indexes.

## Future Enhancements

1. **Live Typeahead Suggestions**
   - Currently shows mock suggestions
   - Can be connected to server endpoint for real data

2. **Query History**
   - Save frequently used filter combinations
   - Quick-load previous searches

3. **Advanced Boolean Logic**
   - AND/OR operators between filters
   - Parentheses for complex conditions

4. **Saved Filters**
   - Save filter sets as templates
   - Share with other users

5. **Custom Fields**
   - Support metadata fields
   - User-defined filters

6. **Performance**
   - Debounce typeahead requests
   - Cache suggestions
   - Lazy-load large result sets

## Component States

### Empty State
- "Advanced Query Builder" heading
- Add buttons for all three filter types
- No active filters

### Single Filter
- One filter row visible
- Add buttons below
- Remove button on filter row

### Multiple Filters
- All active filters visible
- Add buttons remain accessible
- Each filter independently removable
- Scroll if needed (max-height with overflow)

## Accessibility Features

- Clear labels for all inputs
- Semantic HTML (buttons, inputs, selects)
- Hover states on interactive elements
- Tab-navigable controls
- Clear visual feedback on actions

## Testing Recommendations

1. Add/remove individual filters
2. Change preset dates and verify calculations
3. Select text fields and verify typeahead
4. Select array fields and verify multi-select
5. Mix filter types and verify output
6. Convert filters to SearchRequest and verify API call

## Notes

- All dates are formatted as YYYY-MM-DD strings (ISO format)
- Date calculations use chrono library
- Mock suggestions are included; can be replaced with server calls
- Component is fully reactive using Leptos signals
- No external dependencies beyond what's already in project
