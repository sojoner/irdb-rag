use leptos::*;
use leptos::prelude::*;

#[component]
pub fn SearchBar(
    query: Signal<String>,
    set_query: WriteSignal<String>,
    #[prop(into)] on_search: Callback<()>,
    bm25_weight: Signal<f64>,
    set_bm25_weight: WriteSignal<f64>,
    vector_weight: Signal<f64>,
    set_vector_weight: WriteSignal<f64>,
) -> impl IntoView {
    let (show_help, set_show_help) = signal(false);
    let (show_settings, set_show_settings) = signal(false);

    view! {
        <div class="p-4 bg-white border-b border-gray-200 space-y-3">
            <div class="relative">
                <div class="absolute inset-y-0 left-0 pl-3 flex items-center pointer-events-none">
                    <svg class="h-4 w-4 text-gray-400" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                    </svg>
                </div>
                <input type="text"
                       prop:value=query
                       on:input=move |ev| set_query.set(event_target_value(&ev))
                       on:keyup=move |ev| if ev.key() == "Enter" { on_search.run(()); }
                       placeholder="Search documents (e.g., 'machine learning' or 'title:rust')"
                       class="w-full pl-10 pr-10 py-2.5 bg-gray-50 border border-gray-300 rounded-lg text-sm focus:ring-2 focus:ring-blue-500 focus:border-blue-500 transition-shadow" />
                
                <div class="absolute inset-y-0 right-0 flex items-center pr-2 gap-1">
                    <button on:click=move |_| set_show_help.update(|v| *v = !*v)
                            class="p-1 text-gray-400 hover:text-blue-600 rounded-full hover:bg-blue-50 transition-colors"
                            title="Search Syntax">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8.228 9c.549-1.165 2.03-2 3.772-2 2.21 0 4 1.343 4 3 0 1.4-1.278 2.575-3.006 2.907-.542.104-.994.54-.994 1.093m0 3h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
                        </svg>
                    </button>
                    <button on:click=move |_| set_show_settings.update(|v| *v = !*v)
                            class="p-1 text-gray-400 hover:text-blue-600 rounded-full hover:bg-blue-50 transition-colors"
                            title="Search Settings">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.065 2.572c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.572 1.065c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.065-2.572c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                        </svg>
                    </button>
                </div>
            </div>

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

            <Show when=move || show_settings.get()>
                <div class="text-xs bg-gray-50 p-3 rounded-lg border border-gray-200">
                    <p class="font-semibold mb-2 text-gray-700">"Hybrid Search Weights:"</p>
                    <div class="space-y-3">
                        <div>
                            <div class="flex justify-between mb-1">
                                <label class="text-gray-600">"BM25 (Keyword)"</label>
                                <span class="font-mono">{move || bm25_weight.get()}</span>
                            </div>
                            <input type="range" min="0" max="1" step="0.1"
                                   prop:value=move || bm25_weight.get()
                                   on:input=move |ev| {
                                       if let Ok(val) = event_target_value(&ev).parse::<f64>() {
                                           set_bm25_weight.set(val);
                                           on_search.run(());
                                       }
                                   }
                                   class="w-full h-1.5 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-blue-600" />
                        </div>
                        <div>
                            <div class="flex justify-between mb-1">
                                <label class="text-gray-600">"Vector (Semantic)"</label>
                                <span class="font-mono">{move || vector_weight.get()}</span>
                            </div>
                            <input type="range" min="0" max="1" step="0.1"
                                   prop:value=move || vector_weight.get()
                                   on:input=move |ev| {
                                       if let Ok(val) = event_target_value(&ev).parse::<f64>() {
                                           set_vector_weight.set(val);
                                           on_search.run(());
                                       }
                                   }
                                   class="w-full h-1.5 bg-gray-200 rounded-lg appearance-none cursor-pointer accent-purple-600" />
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}
