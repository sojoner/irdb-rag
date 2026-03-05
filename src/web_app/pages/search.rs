use crate::domain::models::SearchMetadata;
use crate::web_app::components::{
    advanced_query_builder::{AdvancedQueryBuilder, QueryFilter, FilterValue},
    chat::Chat,
    conversation_list::ConversationList,
    document_preview::DocumentPreview,
    faceted_filters::{get_categories, FacetedFilters},
    results_list::ResultsList,
    stats_bar::StatsBar,
    unified_search::UnifiedSearch,
};
use crate::web_app::services::search::{DeleteDocument, SearchDocuments, SortOrder};
use leptos::prelude::*;
use uuid::Uuid;

#[component]
pub fn SearchPage() -> impl IntoView {
    // ============ STATE ============
    let (search_query, set_search_query) = signal(String::new());

    // Modal states
    let (show_help, set_show_help) = signal(false);

    // Search settings - BM25 only for main search
    let (selected_context, set_selected_context) = signal(Vec::<Uuid>::new());
    let (selected_document_id, set_selected_document_id) = signal(None::<Uuid>);
    let (search_fields, set_search_fields) = signal(vec![
        "content".to_string(),
        "title".to_string(),
        "summary".to_string(),
    ]);

    // Pagination state
    let (current_page, set_current_page) = signal(0i32);
    let (page_size, _set_page_size) = signal(20i32);
    let (total_count, set_total_count) = signal(0i64);

    // Sort state
    let (sort_order, set_sort_order) = signal(SortOrder::Relevance);

    // Filter state - Load categories on mount
    let categories_resource = Resource::new_blocking(|| (), |_| async { get_categories().await });
    let categories = Signal::derive(move || {
        categories_resource
            .get()
            .and_then(|res: Result<_, _>| res.ok())
            .unwrap_or_default()
    });

    let (selected_category, set_selected_category) = signal(None::<Uuid>);
    let (selected_keywords, set_selected_keywords) = signal(Vec::<String>::new());
    let (selected_concepts, set_selected_concepts) = signal(Vec::<String>::new());
    let (selected_locations, set_selected_locations) = signal(Vec::<String>::new());
    let (selected_persons, set_selected_persons) = signal(Vec::<String>::new());
    let (selected_organizations, set_selected_organizations) = signal(Vec::<String>::new());
    let (selected_authors, set_selected_authors) = signal(Vec::<String>::new());

    // Advanced Query Builder filters
    let (query_builder_filters, set_query_builder_filters) = signal(Vec::<QueryFilter>::new());

    // Server Action for Search
    let search_action = ServerAction::<SearchDocuments>::new();

    // Server Action for Delete
    let delete_action = ServerAction::<DeleteDocument>::new();

    // Derived signals from action
    let (results, set_results) = signal(Vec::new());
    let (_search_metadata, _set_search_metadata) = signal(None::<SearchMetadata>);

    // ============ SEARCH FUNCTION ============
    let execute_search_with_page = move |page: i32| {
        let query = search_query.get();
        let query_trimmed = query.trim();
        leptos::logging::log!("SearchPage: executing search for '{}', page {}", query, page);

        // Collect filter values from faceted filters
        let mut keywords = selected_keywords.get();
        let mut concepts = selected_concepts.get();
        let mut locations = selected_locations.get();
        let mut persons = selected_persons.get();
        let mut organizations = selected_organizations.get();
        let mut authors = selected_authors.get();
        let category = selected_category.get();
        let mut date_from: Option<String> = None;
        let mut date_to: Option<String> = None;

        // Collect text field filters to append to query
        let mut text_field_queries: Vec<String> = Vec::new();

        // Merge query builder filters with faceted filters
        for filter in query_builder_filters.get() {
            match filter.value {
                FilterValue::DateRange { from, to } => {
                    date_from = from;
                    date_to = to;
                }
                FilterValue::Text { field, value } => {
                    // Text filters are appended to the query using BM25 field syntax
                    if !value.is_empty() {
                        leptos::logging::log!("Text filter: {} = {}", field, value);
                        // Use BM25 field-qualified syntax: field:(value)
                        text_field_queries.push(format!("{}:({})", field, value));
                    }
                }
                FilterValue::Array { field, values } => {
                    // Merge array field values with existing selections
                    if !values.is_empty() {
                        match field.as_str() {
                            "keywords" => {
                                for v in values {
                                    if !keywords.contains(&v) {
                                        keywords.push(v);
                                    }
                                }
                            }
                            "locations" => {
                                for v in values {
                                    if !locations.contains(&v) {
                                        locations.push(v);
                                    }
                                }
                            }
                            "persons" => {
                                for v in values {
                                    if !persons.contains(&v) {
                                        persons.push(v);
                                    }
                                }
                            }
                            "organizations" => {
                                for v in values {
                                    if !organizations.contains(&v) {
                                        organizations.push(v);
                                    }
                                }
                            }
                            "concepts" => {
                                for v in values {
                                    if !concepts.contains(&v) {
                                        concepts.push(v);
                                    }
                                }
                            }
                            "authors" => {
                                for v in values {
                                    if !authors.contains(&v) {
                                        authors.push(v);
                                    }
                                }
                            }
                            "products" => {
                                // Products field - currently not stored in dedicated signal
                                // Can be handled via query enhancement or API parameter if needed
                                leptos::logging::log!("Products filter: {:?}", values);
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Check if we have any search criteria
        let has_query = !query_trimmed.is_empty();
        let has_text_field_filters = !text_field_queries.is_empty();
        let has_filters = category.is_some()
            || !keywords.is_empty()
            || !concepts.is_empty()
            || !locations.is_empty()
            || !persons.is_empty()
            || !organizations.is_empty()
            || !authors.is_empty()
            || date_from.is_some()
            || date_to.is_some()
            || has_text_field_filters;

        // Build final query: combine user query with text field filters
        // Allow '*' to browse all documents
        let final_query = if query_trimmed == "*" {
            // Explicit browse all
            "*".to_string()
        } else if !has_query && has_text_field_filters {
            // No main query but have text field filters - use the field filters as the query
            leptos::logging::log!(
                "SearchPage: using text field filters as query: {:?}",
                text_field_queries
            );
            text_field_queries.join(" AND ")
        } else if !has_query && has_filters {
            // No query but have other filters - use wildcard
            leptos::logging::log!(
                "SearchPage: using wildcard search with filters (filter-only mode)"
            );
            "*".to_string()
        } else if !has_query && !has_filters {
            // No query and no filters - skip search
            leptos::logging::log!("SearchPage: skipping search - no query and no filters");
            return;
        } else if has_text_field_filters {
            // Have both main query AND text field filters - combine them
            leptos::logging::log!(
                "SearchPage: combining query '{}' with text field filters {:?}",
                query,
                text_field_queries
            );
            format!("{} AND {}", query, text_field_queries.join(" AND "))
        } else {
            // User has typed something - use their exact query
            query.to_string()
        };

        use crate::web_app::services::search::SearchRequest;

        let limit = page_size.get();
        let offset = page * limit;
        let sort = sort_order.get();

        search_action.dispatch(SearchDocuments {
            request: SearchRequest {
                query: final_query,
                limit,
                offset,
                sort,
                search_fields: search_fields.get(),
                bm25_weight: 1.0,
                vector_weight: 0.0,
                category_id: category,
                date_from,
                date_to,
                keywords: if keywords.is_empty() {
                    None
                } else {
                    Some(keywords)
                },
                concepts: if concepts.is_empty() {
                    None
                } else {
                    Some(concepts)
                },
                locations: if locations.is_empty() {
                    None
                } else {
                    Some(locations)
                },
                persons: if persons.is_empty() {
                    None
                } else {
                    Some(persons)
                },
                organizations: if organizations.is_empty() {
                    None
                } else {
                    Some(organizations)
                },
                authors: if authors.is_empty() {
                    None
                } else {
                    Some(authors)
                },
            },
        });
    };

    // Wrapper for new searches (resets to page 0)
    let execute_search = move |_| {
        set_current_page.set(0);
        execute_search_with_page(0);
    };

    let on_delete = Callback::new(move |id: Uuid| {
        use crate::web_app::services::search::DeleteDocument;
        delete_action.dispatch(DeleteDocument { doc_id: id });
    });

    // Effect to update results when search_action completes
    Effect::new(move |_| {
        if let Some(Ok(response)) = search_action.value().get() {
            leptos::logging::log!(
                "SearchPage: Effect received {} of {} total results (page {}, {}ms)",
                response.result_count,
                response.total_count,
                response.page,
                response.duration_ms
            );

            set_results.set(response.results);
            set_total_count.set(response.total_count);
        } else if let Some(Err(e)) = search_action.value().get() {
            leptos::logging::error!("SearchPage: Search failed: {:?}", e);
            set_results.set(vec![]);
            set_total_count.set(0);
        }
    });

    let is_loading = search_action.pending();

    // Effect to refresh results after delete
    Effect::new(move |_| {
        if let Some(Ok(_)) = delete_action.value().get() {
            // Re-run the search to update results
            execute_search(());
        }
    });

    // Dummy signals for read-only props (UnifiedSearch component expects WriteSignal)
    let (_, set_dummy_results) = signal(Vec::new());
    let (_, set_dummy_loading) = signal(false);

    // Signal for chat input text - will be set by copy/send buttons
    let (chat_input_text, set_chat_input_text) = signal(String::new());

    // Conversation management callbacks
    let (current_conversation_id, set_current_conversation_id) = signal(Option::<Uuid>::None);

    // Chat reset trigger - increment to clear chat messages
    let (chat_reset_trigger, set_chat_reset_trigger) = signal(0u32);

    let on_conversation_select = Callback::new(move |id: Uuid| {
        leptos::logging::log!("Selected conversation: {}", id);
        set_current_conversation_id.set(Some(id));
    });

    let on_new_conversation = Callback::new(move |_: ()| {
        leptos::logging::log!("Creating new conversation");
        set_current_conversation_id.set(None);
        // Increment reset trigger to clear chat messages
        set_chat_reset_trigger.update(|t| *t += 1);
    });

    let on_delete_conversation = Callback::new(move |id: Uuid| {
        leptos::logging::log!("Deleting conversation: {}", id);

        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                let window = match web_sys::window() {
                    Some(w) => w,
                    None => {
                        leptos::logging::error!("No window available");
                        return;
                    }
                };

                let url = format!("/api/conversations/{}", id);

                let opts = web_sys::RequestInit::new();
                opts.set_method("DELETE");

                let request = match web_sys::Request::new_with_str_and_init(&url, &opts) {
                    Ok(r) => r,
                    Err(e) => {
                        leptos::logging::error!("Failed to create delete request: {:?}", e);
                        return;
                    }
                };

                match wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request)).await {
                    Ok(resp_value) => {
                        let resp: web_sys::Response = resp_value.into();
                        if resp.ok() {
                            leptos::logging::log!("Conversation deleted successfully");
                            // Reload page to refresh conversation list
                            let _ = window.location().reload();
                        } else {
                            leptos::logging::error!("Delete failed with status: {}", resp.status());
                        }
                    }
                    Err(e) => {
                        leptos::logging::error!("Delete request failed: {:?}", e);
                    }
                }
            });
        }
    });

    // Effect for page mount
    Effect::new(move |_| {
        leptos::logging::log!("SearchPage mounted/hydrated");
    });

    // ============ RESIZE STATE ============
    // Track left column width percentage (0-100), default 50%
    // Note: Resize functionality can be re-enabled with proper closure handling
    let (left_width_percent, _set_left_width_percent) = signal(50.0);

    // ============ RENDER ============
    let search_error = move || search_action.value().get().and_then(|res| res.err());

    view! {
        <div class="flex flex-col h-screen bg-white">
            // HEADER: Title bar with toolbar buttons
            <header class="bg-white shadow-sm z-10 border-b border-gray-200">
                <div class="px-6 py-4 flex justify-between items-center">
                    <h1 class="text-2xl font-bold text-gray-900">"RAG Search"</h1>

                    // macOS-style toolbar buttons on the right
                    <div class="flex items-center gap-1 bg-gray-100/50 rounded-lg p-1 border border-gray-200/60 shadow-sm">
                        <a
                            href="/import"
                            class="px-3 py-1.5 text-xs font-medium text-gray-700 bg-white rounded-md hover:bg-gray-50 transition-all flex items-center gap-1.5 shadow-sm border border-gray-200/40 hover:shadow"
                            title="Import Documents"
                        >
                            <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                            </svg>
                            <span>"Import"</span>
                        </a>

                        <button
                            class="px-3 py-1.5 text-xs font-medium text-gray-700 bg-white rounded-md hover:bg-gray-50 transition-all flex items-center gap-1.5 shadow-sm border border-gray-200/40 hover:shadow"
                            title="Help"
                            on:click=move |_| set_show_help.set(!show_help.get())
                        >
                            <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                        </button>
                    </div>
                </div>
                <Show when=move || search_error().is_some()>
                    <div class="bg-red-50 border-l-4 border-red-500 p-4 mx-6 mb-2">
                        <div class="flex">
                            <div class="flex-shrink-0">
                                <svg class="h-5 w-5 text-red-400" viewBox="0 0 20 20" fill="currentColor">
                                    <path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd" />
                                </svg>
                            </div>
                            <div class="ml-3">
                                <p class="text-sm text-red-700">
                                    {move || search_error().unwrap().to_string()}
                                </p>
                            </div>
                        </div>
                    </div>
                </Show>
            </header>

            // STATS BAR: Database and index information
            <StatsBar />

            // Remove modal states from render

            // MAIN CONTENT: 2 Column Layout (Search + Filters + Results | Chat)
            <div id="content-container" class="flex-1 overflow-hidden flex bg-gray-50 select-none p-4 gap-1">
                // Column 1: Search + Filters + Results (resizable)
                <div class="flex flex-col bg-white rounded-lg border border-gray-200 overflow-hidden shadow-sm" style=move || {
                    format!("width: calc({}% - 2px); transition: width 0.1s ease;",
                        left_width_percent.get()
                    )
                }>
                    <div class="px-4 py-3 border-b border-gray-200 bg-white flex-shrink-0">
                        <UnifiedSearch
                            query=search_query.into()
                            set_query=set_search_query
                            results=results.into()
                            set_results=set_dummy_results
                            loading=is_loading.into()
                            set_loading=set_dummy_loading
                            search_fields=search_fields.into()
                            set_search_fields=set_search_fields
                            on_search=Callback::new(move |_| execute_search(()))
                        />
                        // Advanced Query Builder - directly under search
                        <div class="mt-3 pt-3 border-t border-gray-100">
                            <AdvancedQueryBuilder
                                on_filter_change=Callback::new(move |filters: Vec<QueryFilter>| {
                                    set_query_builder_filters.set(filters);
                                    execute_search(());
                                })
                            />
                        </div>
                    </div>

                    // Filters and Results
                    <div class="flex-1 flex gap-3 overflow-hidden p-3">
                        // Filters (collapsible)
                        <div class="w-80 flex flex-col bg-white rounded border border-gray-200 overflow-y-auto flex-shrink-0 z-0">
                            <div class="px-3 py-2 border-b border-gray-200 bg-white sticky top-0">
                                <h3 class="text-xs font-bold text-gray-700">"Filters"</h3>
                            </div>
                            <div class="divide-y">
                                // Faceted Filters Section
                                <div class="bg-gray-50">
                                    <FacetedFilters
                                        categories=categories
                                        selected_category=selected_category.into()
                                        set_selected_category=set_selected_category
                                        selected_keywords=selected_keywords.into()
                                        set_selected_keywords=set_selected_keywords
                                        selected_concepts=selected_concepts.into()
                                        set_selected_concepts=set_selected_concepts
                                        selected_locations=selected_locations.into()
                                        set_selected_locations=set_selected_locations
                                        selected_persons=selected_persons.into()
                                        set_selected_persons=set_selected_persons
                                        selected_organizations=selected_organizations.into()
                                        set_selected_organizations=set_selected_organizations
                                        selected_authors=selected_authors.into()
                                        set_selected_authors=set_selected_authors
                                        on_change=Callback::new(move |_| execute_search(()))
                                    />
                                </div>

                            </div>
                        </div>

                        // Results
                        <div class="flex-1 flex flex-col bg-gray-50 rounded border border-gray-200 overflow-hidden">
                            // Header with count and sort
                            <div class="px-3 py-2 border-b border-gray-200 bg-white flex justify-between items-center flex-shrink-0 gap-2">
                                <h3 class="text-xs font-bold text-gray-700">"Results"</h3>
                                <div class="flex items-center gap-3">
                                    // Sort dropdown
                                    <div class="flex items-center gap-1.5">
                                        <label class="text-xs text-gray-500">"Sort:"</label>
                                        <select
                                            class="text-xs border border-gray-200 rounded px-1.5 py-0.5 bg-white focus:ring-1 focus:ring-blue-500"
                                            on:change=move |ev| {
                                                let value = event_target_value(&ev);
                                                let new_sort = match value.as_str() {
                                                    "relevance" => SortOrder::Relevance,
                                                    "date_desc" => SortOrder::DateDesc,
                                                    "date_asc" => SortOrder::DateAsc,
                                                    "title_asc" => SortOrder::TitleAsc,
                                                    "title_desc" => SortOrder::TitleDesc,
                                                    _ => SortOrder::Relevance,
                                                };
                                                set_sort_order.set(new_sort);
                                                execute_search(());
                                            }
                                        >
                                            <option value="relevance" selected=move || matches!(sort_order.get(), SortOrder::Relevance)>"Relevance"</option>
                                            <option value="date_desc" selected=move || matches!(sort_order.get(), SortOrder::DateDesc)>"Date (newest)"</option>
                                            <option value="date_asc" selected=move || matches!(sort_order.get(), SortOrder::DateAsc)>"Date (oldest)"</option>
                                            <option value="title_asc" selected=move || matches!(sort_order.get(), SortOrder::TitleAsc)>"Title (A-Z)"</option>
                                            <option value="title_desc" selected=move || matches!(sort_order.get(), SortOrder::TitleDesc)>"Title (Z-A)"</option>
                                        </select>
                                    </div>
                                    // Result count
                                    <span class="text-xs text-gray-500">
                                        {move || {
                                            let count = total_count.get();
                                            let page = current_page.get();
                                            let size = page_size.get();
                                            let start = page * size + 1;
                                            let end = std::cmp::min((page + 1) * size, count as i32);
                                            if count > 0 {
                                                format!("{}-{} of {}", start, end, count)
                                            } else {
                                                "0 found".to_string()
                                            }
                                        }}
                                    </span>
                                </div>
                            </div>

                            <ResultsList
                                results=results.into()
                                loading=is_loading.into()
                                selected_context=selected_context.into()
                                set_selected_context=set_selected_context
                                on_preview=Callback::new(move |id| set_selected_document_id.set(Some(id)))
                                on_delete=on_delete
                                set_chat_input=set_chat_input_text
                            />

                            // Pagination controls
                            <Show when=move || { total_count.get() > page_size.get() as i64 }>
                                <div class="px-3 py-2 border-t border-gray-200 bg-white flex justify-center items-center gap-2 flex-shrink-0">
                                    <button
                                        class="px-2 py-1 text-xs font-medium text-gray-600 bg-gray-100 rounded hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed"
                                        disabled=move || current_page.get() == 0
                                        on:click=move |_| {
                                            let new_page = current_page.get() - 1;
                                            set_current_page.set(new_page);
                                            execute_search_with_page(new_page);
                                        }
                                    >
                                        "← Prev"
                                    </button>

                                    <span class="text-xs text-gray-600">
                                        {move || {
                                            let page = current_page.get();
                                            let total_pages = ((total_count.get() as f64) / (page_size.get() as f64)).ceil() as i32;
                                            format!("Page {} of {}", page + 1, total_pages)
                                        }}
                                    </span>

                                    <button
                                        class="px-2 py-1 text-xs font-medium text-gray-600 bg-gray-100 rounded hover:bg-gray-200 disabled:opacity-50 disabled:cursor-not-allowed"
                                        disabled=move || {
                                            let total_pages = ((total_count.get() as f64) / (page_size.get() as f64)).ceil() as i32;
                                            current_page.get() >= total_pages - 1
                                        }
                                        on:click=move |_| {
                                            let new_page = current_page.get() + 1;
                                            set_current_page.set(new_page);
                                            execute_search_with_page(new_page);
                                        }
                                    >
                                        "Next →"
                                    </button>
                                </div>
                            </Show>
                        </div>
                    </div>
                </div>

                // Draggable Divider (visual only, resizing via CSS media queries)
                <div
                    class="w-1 bg-gradient-to-b from-gray-200 via-gray-300 to-gray-200 hover:from-blue-300 hover:via-blue-400 hover:to-blue-300 hover:bg-gradient-to-b flex-shrink-0 cursor-col-resize transition-colors"
                    style="height: calc(100% + 8px); margin: -4px 0;"
                />

                // Column 2: Chat Interface with Conversation List
                <div class="flex flex-col bg-white rounded-lg border border-gray-200 overflow-hidden shadow-sm flex-1" style="min-width: 400px;">
                    // Conversation List (top section, collapsible)
                    <div class="border-b border-gray-200" style="max-height: 35%; min-height: 200px;">
                        <ConversationList
                            on_conversation_select=on_conversation_select
                            on_new_conversation=on_new_conversation
                            on_delete_conversation=on_delete_conversation
                        />
                    </div>

                    // Chat Interface (bottom section)
                    <div class="flex-1 overflow-hidden">
                        <Chat
                            external_input_text=chat_input_text.into()
                            reset_trigger=chat_reset_trigger.into()
                            selected_conversation_id=current_conversation_id.into()
                            on_search_results=Callback::new(move |docs: Vec<crate::domain::models::SearchResult>| {
                                leptos::logging::log!("SearchPage: Received {} search results from chat", docs.len());
                                set_results.set(docs);
                            })
                        />
                    </div>
                </div>
            </div>

            <DocumentPreview
                document_id=selected_document_id
                on_close=Callback::new(move |_| set_selected_document_id.set(None))
                set_chat_input=set_chat_input_text
            />

            // Help Modal
            <Show when=move || show_help.get()>
                <div class="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center z-50">
                    <div class="bg-white rounded-lg shadow-xl max-w-md w-full mx-4">
                        <div class="px-6 py-4 border-b border-gray-200 flex justify-between items-center">
                            <h2 class="text-lg font-bold text-gray-900">"Help"</h2>
                            <button
                                class="text-gray-500 hover:text-gray-700"
                                on:click=move |_| set_show_help.set(false)
                            >
                                <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                                </svg>
                            </button>
                        </div>
                        <div class="px-6 py-4 space-y-4 max-h-96 overflow-y-auto">
                            <div>
                                <h3 class="font-semibold text-gray-900 text-sm">"How to Search"</h3>
                                <p class="text-sm text-gray-600 mt-1">"Type keywords or phrases to search across your documents. Use filters to narrow results by category, location, person, or other metadata."</p>
                            </div>
                            <div>
                                <h3 class="font-semibold text-gray-900 text-sm">"Search Weights"</h3>
                                <p class="text-sm text-gray-600 mt-1">"BM25 (keyword) search excels at finding exact matches. Vector (semantic) search is better for conceptual queries. Adjust the balance in Settings."</p>
                            </div>
                            <div>
                                <h3 class="font-semibold text-gray-900 text-sm">"Chat Context"</h3>
                                <p class="text-sm text-gray-600 mt-1">"Click on search results to add them as context to your chat. The chat will use these documents to provide more relevant responses."</p>
                            </div>
                            <div>
                                <h3 class="font-semibold text-gray-900 text-sm">"Importing Documents"</h3>
                                <p class="text-sm text-gray-600 mt-1">"Use the Import button in the toolbar to add new documents to your knowledge base. Supports PDF, text, markdown, and HTML files."</p>
                            </div>
                        </div>
                        <div class="px-6 py-4 border-t border-gray-200 flex justify-end gap-2">
                            <button
                                class="px-4 py-2 text-sm font-medium text-gray-700 hover:bg-gray-100 rounded-md"
                                on:click=move |_| set_show_help.set(false)
                            >
                                "Close"
                            </button>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}
