/// Example integration of Advanced Query Builder
/// This shows how to convert QueryFilter objects to SearchRequest
/// and integrate with the search functionality

use crate::domain::dtos::SearchRequest;
use crate::web_app::components::advanced_query_builder::{
    AdvancedQueryBuilder, FilterValue, QueryFilter,
};
use leptos::prelude::*;

/// Convert QueryFilter objects to SearchRequest
pub fn build_search_request(
    base_query: String,
    filters: Vec<QueryFilter>,
) -> SearchRequest {
    let mut request = SearchRequest {
        query: base_query,
        limit: 20,
        search_fields: vec!["content".to_string(), "title".to_string(), "summary".to_string()],
        bm25_weight: 0.5,
        vector_weight: 0.5,
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
    };

    for filter in filters {
        match filter.value {
            FilterValue::DateRange { from, to } => {
                request.date_from = from;
                request.date_to = to;
            }
            FilterValue::Text { field, value } => {
                // For text filters, append to query using field syntax
                if !value.is_empty() {
                    request.query.push(' ');
                    request.query.push_str(&format!("{}:\"{}\"", field, value));
                }
            }
            FilterValue::Array { field, values } => {
                if !values.is_empty() {
                    match field.as_str() {
                        "keywords" => request.keywords = Some(values),
                        "locations" => request.locations = Some(values),
                        "persons" => request.persons = Some(values),
                        "organizations" => request.organizations = Some(values),
                        "products" => request.products = Some(values),
                        "concepts" => request.concepts = Some(values),
                        _ => {}
                    }
                }
            }
        }
    }

    request
}

#[component]
pub fn QueryBuilderExample() -> impl IntoView {
    let (query, set_query) = signal(String::new());
    let (filters, set_filters) = signal(Vec::<QueryFilter>::new());
    let (debug_request, set_debug_request) = signal(String::new());

    let handle_filter_change = move |new_filters: Vec<QueryFilter>| {
        set_filters.set(new_filters.clone());

        // Build the search request
        let search_req = build_search_request(query.get(), new_filters);

        // Show debug info
        let debug = format!("{:#?}", search_req);
        set_debug_request.set(debug);
    };

    view! {
        <div class="max-w-4xl mx-auto p-6 space-y-6">
            <div class="bg-white rounded-lg shadow p-6">
                <h1 class="text-2xl font-bold text-gray-900 mb-4">"Advanced Query Builder Example"</h1>

                <div class="mb-6">
                    <label class="block text-sm font-medium text-gray-700 mb-2">
                        "Search Query"
                    </label>
                    <input
                        type="text"
                        value=move || query.get()
                        on:input=move |ev| set_query.set(event_target_value(&ev))
                        placeholder="Enter your search query..."
                        class="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                    />
                </div>

                // Advanced Query Builder Component
                <AdvancedQueryBuilder on_filter_change=Callback::new(handle_filter_change) />
            </div>

            // Debug panel showing generated SearchRequest
            <div class="bg-gray-50 rounded-lg border border-gray-200 p-6">
                <h2 class="text-lg font-semibold text-gray-900 mb-4">"Generated SearchRequest (Debug)"</h2>
                <pre class="bg-white p-4 rounded border border-gray-300 text-xs font-mono overflow-auto max-h-96">
                    {move || {
                        if debug_request.get().is_empty() {
                            "No filters applied yet. Add filters above to see the SearchRequest.".to_string()
                        } else {
                            debug_request.get()
                        }
                    }}
                </pre>
            </div>

            // Instructions
            <div class="bg-blue-50 rounded-lg border border-blue-200 p-6">
                <h3 class="text-sm font-semibold text-blue-900 mb-3">"Integration Instructions"</h3>
                <ul class="text-sm text-blue-800 space-y-2">
                    <li>"1. Use the Advanced Query Builder to add filters"</li>
                    <li>"2. The filters are automatically converted to SearchRequest format"</li>
                    <li>"3. Text fields can be searched (title, content, summary, author)"</li>
                    <li>"4. Array fields support multiple selections (keywords, locations, etc.)"</li>
                    <li>"5. Date filters support quick shortcuts and custom ranges"</li>
                    <li>"6. See debug panel above for the generated SearchRequest"</li>
                </ul>
            </div>
        </div>
    }
}
