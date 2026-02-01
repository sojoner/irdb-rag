use crate::domain::models::SortOrder;
use crate::web_app::services::search::SearchRequest;
use crate::web_app::components::advanced_query_builder::{
    AdvancedQueryBuilder, FilterValue, QueryFilter,
};
use crate::web_app::components::results_list::ResultsList;
use crate::web_app::services::search::search_documents;
use leptos::prelude::*;
use uuid::Uuid;

/// Convert QueryFilter objects to SearchRequest
pub fn build_search_request(
    base_query: String,
    filters: Vec<QueryFilter>,
    sort: SortOrder,
) -> SearchRequest {
    let mut request = SearchRequest {
        query: base_query,
        limit: 20,
        sort,
        search_fields: vec![
            "content".to_string(),
            "title".to_string(),
            "summary".to_string(),
        ],
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
        offset: 0,
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
    let (sort, set_sort) = signal(SortOrder::Relevance);
    let (selected_context, set_selected_context) = signal(Vec::<Uuid>::new());

    // Create a resource that fetches results when dependencies change
    let search_resource = Resource::new(
        move || (query.get(), filters.get(), sort.get()),
        move |(q, f, s)| async move {
            let request = build_search_request(q, f, s);
            search_documents(request).await
        },
    );

    let handle_filter_change = move |new_filters: Vec<QueryFilter>| {
        set_filters.set(new_filters);
    };

    // Derived signals for the ResultsList
    let results = Signal::derive(move || {
        search_resource
            .get()
            .and_then(|res| res.ok())
            .map(|r| r.results)
            .unwrap_or_default()
    });

    // In Leptos Resource, get() returns Option<T>. If None, it's loading.
    let loading = Signal::derive(move || search_resource.get().is_none());

    let stats = move || {
        search_resource.get().and_then(|res| res.ok()).map(|r| {
            format!(
                "Found {} results in {}ms",
                r.total_count, r.duration_ms
            )
        })
    };

    view! {
        <div class="max-w-7xl mx-auto p-6 space-y-6">
            <div class="flex flex-col gap-6">
                // Top Bar: Search Input & Sort
                <div class="bg-white rounded-lg shadow p-4 flex gap-4 items-center">
                    <div class="flex-1">
                        <input
                            type="text"
                            value=move || query.get()
                            on:input=move |ev| set_query.set(event_target_value(&ev))
                            placeholder="Search documents..."
                            class="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent text-lg"
                        />
                    </div>
                    <div class="w-48">
                        <select
                            on:change=move |ev| {
                                let val = event_target_value(&ev);
                                let new_sort = match val.as_str() {
                                    "DateDesc" => SortOrder::DateDesc,
                                    "DateAsc" => SortOrder::DateAsc,
                                    "TitleAsc" => SortOrder::TitleAsc,
                                    "TitleDesc" => SortOrder::TitleDesc,
                                    _ => SortOrder::Relevance,
                                };
                                set_sort.set(new_sort);
                            }
                            class="w-full px-4 py-2 border border-gray-300 rounded-lg focus:ring-2 focus:ring-blue-500 focus:border-transparent"
                        >
                            <option value="Relevance">"Relevance"</option>
                            <option value="DateDesc">"Newest First"</option>
                            <option value="DateAsc">"Oldest First"</option>
                            <option value="TitleAsc">"Title A-Z"</option>
                            <option value="TitleDesc">"Title Z-A"</option>
                        </select>
                    </div>
                </div>

                <div class="grid grid-cols-1 lg:grid-cols-4 gap-6">
                    // Sidebar: Filters
                    <div class="lg:col-span-1 space-y-4">
                        <AdvancedQueryBuilder on_filter_change=Callback::new(handle_filter_change) />
                        
                        <div class="bg-blue-50 rounded-lg border border-blue-200 p-4 text-sm text-blue-800">
                             <p class="font-semibold mb-2">"Search Tips:"</p>
                             <ul class="list-disc list-inside space-y-1">
                                 <li>"Use filters to refine results"</li>
                                 <li>"Combine multiple criteria"</li>
                                 <li>"Date ranges for time-based search"</li>
                             </ul>
                        </div>
                    </div>

                    // Main Content: Stats & Results
                    <div class="lg:col-span-3 space-y-4">
                        // Stats Bar
                        <div class="bg-white rounded-lg shadow-sm px-4 py-2 border border-gray-200 flex justify-between items-center h-12">
                             <span class="text-sm text-gray-600 font-medium">
                                {move || stats().unwrap_or_else(|| "Ready to search".to_string())}
                             </span>
                        </div>

                        // Results List
                        <div class="bg-white rounded-lg shadow min-h-[500px] flex flex-col">
                            <ResultsList
                                results=results
                                loading=loading
                                selected_context=selected_context.into()
                                set_selected_context=set_selected_context
                            />
                        </div>
                    </div>
                </div>
            </div>
        </div>
    }
}
