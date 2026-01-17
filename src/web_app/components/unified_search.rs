use crate::domain::models::SearchResult;
use leptos::prelude::*;

#[component]
pub fn UnifiedSearch(
    query: Signal<String>,
    set_query: WriteSignal<String>,
    results: Signal<Vec<SearchResult>>,
    set_results: WriteSignal<Vec<SearchResult>>,
    loading: Signal<bool>,
    set_loading: WriteSignal<bool>,
    search_fields: Signal<Vec<String>>,
    set_search_fields: WriteSignal<Vec<String>>,
    #[prop(into)] on_search: Callback<()>,
) -> impl IntoView {
    let (show_help, set_show_help) = signal(false);

    Effect::new(move |_| {
        leptos::logging::log!("UnifiedSearch component mounted/hydrated");
    });

    let trigger_search = move |_| {
        let q = query.get();
        leptos::logging::log!("Triggering BM25 search. Query: '{}'", q);

        if q.trim().is_empty() {
            set_results.set(vec![]);
            return;
        }

        set_loading.set(true);
        on_search.run(());
    };

    // Handle input - simpler without debounce for now
    let on_input = move |ev| {
        let new_query = event_target_value(&ev);
        set_query.set(new_query);
    };

    view! {
        <div class="p-4 bg-white border-b border-gray-200 space-y-3">
            // Main search input with AI mode toggle and search button
            <div class="flex gap-2">
                <div class="relative flex-1">
                    <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                        <svg class="h-4 w-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                        </svg>
                    </div>
                    <input
                        type="text"
                        value=move || query.get()
                        on:input=on_input
                        on:keydown=move |ev| if ev.key() == "Enter" { trigger_search(()); }
                        placeholder="Search documents..."
                        class="w-full pl-10 pr-4 py-2.5 bg-gray-50 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500 transition-shadow"
                    />
                </div>

                // Search button
                <button
                    on:click=move |_| trigger_search(())
                    disabled=move || query.get().trim().is_empty()
                    class="px-4 py-2.5 bg-blue-600 text-white rounded-lg text-sm font-medium hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                    title="Search (BM25)"
                >
                    <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                    </svg>
                </button>

                // Right side controls (Help)
                <div class="flex items-center gap-1">
                    // Help button
                    <button
                        on:click=move |_| set_show_help.update(|v| *v = !*v)
                        class="p-1 text-gray-400 hover:text-blue-600 rounded-full hover:bg-blue-50 transition-colors"
                        title="Search Syntax"
                    >
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                        </svg>
                    </button>
                </div>
            </div>

            // Field Selector Checkboxes
            <div class="flex items-center gap-3 text-xs">
                <span class="text-gray-600 font-medium">"Search in:"</span>
                <label class="flex items-center gap-1.5 cursor-pointer hover:text-blue-600 transition-colors">
                    <input
                        type="checkbox"
                        checked=move || search_fields.get().contains(&"content".to_string())
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_search_fields.update(|fields| {
                                if checked && !fields.contains(&"content".to_string()) {
                                    fields.push("content".to_string());
                                } else if !checked {
                                    fields.retain(|f| f != "content");
                                }
                            });
                            trigger_search(());
                        }
                        class="w-3.5 h-3.5 text-blue-600 rounded border-gray-300 focus:ring-blue-500 focus:ring-2"
                    />
                    <span>"Content"</span>
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer hover:text-blue-600 transition-colors">
                    <input
                        type="checkbox"
                        checked=move || search_fields.get().contains(&"title".to_string())
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_search_fields.update(|fields| {
                                if checked && !fields.contains(&"title".to_string()) {
                                    fields.push("title".to_string());
                                } else if !checked {
                                    fields.retain(|f| f != "title");
                                }
                            });
                            trigger_search(());
                        }
                        class="w-3.5 h-3.5 text-blue-600 rounded border-gray-300 focus:ring-blue-500 focus:ring-2"
                    />
                    <span>"Title"</span>
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer hover:text-blue-600 transition-colors">
                    <input
                        type="checkbox"
                        checked=move || search_fields.get().contains(&"summary".to_string())
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_search_fields.update(|fields| {
                                if checked && !fields.contains(&"summary".to_string()) {
                                    fields.push("summary".to_string());
                                } else if !checked {
                                    fields.retain(|f| f != "summary");
                                }
                            });
                            trigger_search(());
                        }
                        class="w-3.5 h-3.5 text-blue-600 rounded border-gray-300 focus:ring-blue-500 focus:ring-2"
                    />
                    <span>"Summary"</span>
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer hover:text-blue-600 transition-colors">
                    <input
                        type="checkbox"
                        checked=move || search_fields.get().contains(&"author".to_string())
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_search_fields.update(|fields| {
                                if checked && !fields.contains(&"author".to_string()) {
                                    fields.push("author".to_string());
                                } else if !checked {
                                    fields.retain(|f| f != "author");
                                }
                            });
                            trigger_search(());
                        }
                        class="w-3.5 h-3.5 text-blue-600 rounded border-gray-300 focus:ring-blue-500 focus:ring-2"
                    />
                    <span>"Author"</span>
                </label>
                <label class="flex items-center gap-1.5 cursor-pointer hover:text-blue-600 transition-colors">
                    <input
                        type="checkbox"
                        checked=move || search_fields.get().contains(&"keywords".to_string())
                        on:change=move |ev| {
                            let checked = event_target_checked(&ev);
                            set_search_fields.update(|fields| {
                                if checked && !fields.contains(&"keywords".to_string()) {
                                    fields.push("keywords".to_string());
                                } else if !checked {
                                    fields.retain(|f| f != "keywords");
                                }
                            });
                            trigger_search(());
                        }
                        class="w-3.5 h-3.5 text-blue-600 rounded border-gray-300 focus:ring-blue-500 focus:ring-2"
                    />
                    <span>"Keywords"</span>
                </label>
            </div>

            // Help panel
            <Show when=move || show_help.get()>
                <div class="text-xs bg-blue-50 p-3 rounded-lg border border-blue-100 text-blue-800">
                    <p class="font-semibold mb-1">"Search Syntax:"</p>
                    <div class="grid grid-cols-2 gap-x-4 gap-y-1">
                        <span><code class="bg-white px-1 rounded">"~fuzzy"</code> " Fuzzy match"</span>
                        <span><code class="bg-white px-1 rounded">"term*"</code> " Prefix search"</span>
                        <span><code class="bg-white px-1 rounded">"\"phrase\""</code> " Exact phrase"</span>
                        <span><code class="bg-white px-1 rounded">"AND/OR"</code> " Boolean"</span>
                    </div>
                </div>
            </Show>


            // Status/Results indicator
            <Show when=move || !results.get().is_empty() || loading.get()>
                <div class="flex items-center justify-between text-xs text-gray-500 px-1">
                    <div class="flex items-center gap-2">
                        <Show when=move || loading.get()>
                            <div class="h-2 w-2 bg-blue-500 rounded-full animate-pulse"></div>
                            "Searching..."
                        </Show>
                        <Show when=move || !loading.get() && !results.get().is_empty()>
                            <span>{move || results.get().len()} " results found"</span>
                        </Show>
                    </div>
                </div>
            </Show>
        </div>
    }
}
