use leptos::*;
use leptos::prelude::*;
use uuid::Uuid;
use crate::web_app::components::{
    unified_search::UnifiedSearch,
    results_list::ResultsList,
    search::SearchDocuments,
    chat_panel::ChatPanel,
    document_preview::DocumentPreview,
    faceted_filters::{FacetedFilters, get_categories},
};

#[component]
pub fn SearchPage() -> impl IntoView {
    // ============ STATE ============
    let (search_query, set_search_query) = signal(String::new());

    // Search settings
    let (bm25_weight, set_bm25_weight) = signal(0.5);
    let (vector_weight, set_vector_weight) = signal(0.5);
    let (selected_context, set_selected_context) = signal(Vec::<Uuid>::new());
    let (ai_mode_enabled, set_ai_mode_enabled) = signal(false); // Disabled for Phase 1
    let (selected_document_id, set_selected_document_id) = signal(None::<Uuid>);

    // Filter state - Load categories on mount
    let categories_resource = Resource::new_blocking(|| (), |_| async { get_categories().await });
    let categories = Signal::derive(move || {
        categories_resource.get()
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

    // Server Action for Search
    let search_action = ServerAction::<SearchDocuments>::new();
    
    // Derived signals from action
    let search_results = move || {
        let val = search_action.value().get();
        leptos::logging::log!("SearchPage: search_action value changed: {:?}", val);
        match val {
            Some(Ok(res)) => {
                leptos::logging::log!("SearchPage: received {} results", res.len());
                res
            },
            Some(Err(e)) => {
                leptos::logging::error!("SearchPage: Search failed: {:?}", e);
                vec![]
            }
            None => vec![],
        }
    };
    
    let is_loading = search_action.pending();

    // Dummy signals for read-only props
    let (_, set_dummy_results) = signal(Vec::new());
    let (_, set_dummy_loading) = signal(false);

    // ============ SEARCH FUNCTION ============
    Effect::new(move |_| {
        leptos::logging::log!("SearchPage mounted/hydrated");
    });

    let execute_search = move |_| {
        let query = search_query.get();
        leptos::logging::log!("SearchPage: executing search for '{}'", query);
        if query.trim().is_empty() {
            return;
        }

        // Collect filter values
        let keywords = selected_keywords.get();
        let concepts = selected_concepts.get();
        let locations = selected_locations.get();
        let persons = selected_persons.get();
        let organizations = selected_organizations.get();
        let authors = selected_authors.get();

        use crate::web_app::components::search::SearchRequest;

        search_action.dispatch(SearchDocuments {
            request: SearchRequest {
                query,
                limit: 20,
                bm25_weight: bm25_weight.get(),
                vector_weight: vector_weight.get(),
                category_id: selected_category.get(),
                keywords: if keywords.is_empty() { None } else { Some(keywords) },
                concepts: if concepts.is_empty() { None } else { Some(concepts) },
                locations: if locations.is_empty() { None } else { Some(locations) },
                persons: if persons.is_empty() { None } else { Some(persons) },
                organizations: if organizations.is_empty() { None } else { Some(organizations) },
                authors: if authors.is_empty() { None } else { Some(authors) },
            }
        });
    };

    // ============ RENDER ============
    let search_error = move || {
        search_action.value().get().and_then(|res| res.err())
    };

    view! {
        <div class="flex flex-col h-screen bg-white">
            // HEADER: Unified Search with macOS-style toolbar
            <header class="bg-white shadow-sm z-10">
                <div class="px-6 py-3 border-b border-gray-200 flex justify-between items-center bg-gradient-to-b from-gray-50 to-white">
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
                            title="Settings"
                        >
                            <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                                <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                            </svg>
                        </button>

                        <button
                            class="px-3 py-1.5 text-xs font-medium text-gray-700 bg-white rounded-md hover:bg-gray-50 transition-all flex items-center gap-1.5 shadow-sm border border-gray-200/40 hover:shadow"
                            title="Help"
                        >
                            <svg class="h-3.5 w-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" stroke-width="2.5">
                                <path stroke-linecap="round" stroke-linejoin="round" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                            </svg>
                        </button>
                    </div>
                </div>
                <UnifiedSearch
                    query=search_query.into()
                    set_query=set_search_query
                    results=Signal::derive(search_results)
                    set_results=set_dummy_results
                    loading=is_loading.into()
                    set_loading=set_dummy_loading
                    bm25_weight=bm25_weight.into()
                    set_bm25_weight=set_bm25_weight
                    vector_weight=vector_weight.into()
                    set_vector_weight=set_vector_weight
                    ai_mode_enabled=ai_mode_enabled.into()
                    set_ai_mode_enabled=set_ai_mode_enabled
                    on_search=Callback::new(move |_| execute_search(()))
                    on_ai_search=Callback::new(move |_| execute_search(())) // Treat AI search as normal search for Phase 1
                />
                <Show when=move || search_error().is_some()>
                    <div class="bg-red-50 border-l-4 border-red-500 p-4 mx-4 mt-2">
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

            // MAIN CONTENT: 3 Column Layout (Filters + Results + Chat)
            <div class="flex-1 overflow-hidden flex gap-4 p-4 bg-gray-50">
                // Column 1: Filters Sidebar (20%)
                <div class="w-[20%] flex flex-col bg-white rounded-lg border border-gray-200 overflow-y-auto shadow-sm">
                    <div class="px-4 py-3 border-b border-gray-200 bg-gray-50">
                        <h2 class="text-sm font-bold text-gray-700">"Filters"</h2>
                    </div>
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

                // Column 2: Search Results (35%)
                <div class="w-[35%] flex flex-col bg-white rounded-lg border border-gray-200 overflow-hidden shadow-sm">
                    <div class="px-4 py-3 border-b border-gray-200 bg-gray-50 flex justify-between items-center">
                        <h2 class="text-sm font-bold text-gray-700">"Discovery & Context"</h2>
                        <span class="text-xs text-gray-500">{move || format!("{} found", search_results().len())}</span>
                    </div>
                    <ResultsList
                        results=Signal::derive(search_results)
                        loading=is_loading.into()
                        selected_context=selected_context.into()
                        set_selected_context=set_selected_context
                        on_preview=Callback::new(move |id| set_selected_document_id.set(Some(id)))
                    />
                </div>

                // Column 3: Synthesis & Chat (45%)
                <div class="md:flex w-[45%] flex-col bg-white rounded-lg border border-gray-200 overflow-hidden shadow-sm">
                    <ChatPanel
                        results=Signal::derive(search_results)
                        search_query=search_query.into()
                        selected_context=selected_context.into()
                    />
                </div>
            </div>

            <DocumentPreview
                document_id=selected_document_id
                on_close=Callback::new(move |_| set_selected_document_id.set(None))
            />
        </div>
    }
}
