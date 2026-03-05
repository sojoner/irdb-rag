use crate::domain::models::SearchResult;
use crate::web_app::utils::format_document_for_chat;
use leptos::prelude::*;
use leptos::*;
use uuid::Uuid;

#[component]
pub fn ResultsList(
    results: Signal<Vec<SearchResult>>,
    loading: Signal<bool>,
    selected_context: Signal<Vec<Uuid>>,
    set_selected_context: WriteSignal<Vec<Uuid>>,
    #[prop(optional)] on_preview: Option<Callback<Uuid>>,
    #[prop(optional)] on_delete: Option<Callback<Uuid>>,
    #[prop(optional)] set_chat_input: Option<leptos::prelude::WriteSignal<String>>,
) -> impl IntoView {
    let toggle_selection = move |id: Uuid| {
        set_selected_context.update(|ids: &mut Vec<Uuid>| {
            if let Some(pos) = ids.iter().position(|x| *x == id) {
                ids.remove(pos);
            } else {
                ids.push(id);
            }
        });
    };

    let handle_preview_click = move |id: Uuid| {
        if let Some(callback) = on_preview {
            callback.run(id);
        }
    };

    let handle_delete_click = move |id: Uuid| {
        if let Some(callback) = on_delete {
            if web_sys::window()
                .and_then(|w| {
                    w.confirm_with_message("Delete this document permanently?")
                        .ok()
                })
                .unwrap_or(false)
            {
                callback.run(id);
            }
        }
    };

    let handle_copy_to_clipboard = move |id: Uuid, content: String| {
        #[cfg(target_arch = "wasm32")]
        {
            let formatted = format_document_for_chat(id, &content);
            leptos::task::spawn_local(async move {
                if let Some(window) = web_sys::window() {
                    let clipboard = window.navigator().clipboard();
                    let promise = clipboard.write_text(&formatted);
                    match wasm_bindgen_futures::JsFuture::from(promise).await {
                        Ok(_) => {
                            leptos::logging::log!("Copied document {} to clipboard", id);
                        }
                        Err(e) => {
                            leptos::logging::error!("Failed to copy to clipboard: {:?}", e);
                        }
                    }
                }
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = (id, content);
        }
    };

    let handle_send_to_chat = move |id: Uuid, content: String| {
        if let Some(setter) = set_chat_input {
            let formatted = format_document_for_chat(id, &content);
            // Append to existing input, or replace if empty
            setter.update(|current| {
                if current.is_empty() {
                    *current = formatted;
                } else {
                    current.push_str(&formatted);
                }
            });
        }
    };

    view! {
        <div class="flex-1 overflow-y-auto bg-gray-100 p-4">
            <Show when=move || loading.get()>
                <div class="flex flex-col items-center justify-center h-32 text-gray-500">
                    <svg class="animate-spin h-6 w-6 mb-2" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
                        <circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
                        <path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
                    </svg>
                    <span class="text-xs">"Searching..."</span>
                </div>
            </Show>

            <Show when=move || !loading.get() && results.get().is_empty()>
                <div class="flex flex-col items-center justify-center h-64 text-gray-400">
                    <svg class="h-12 w-12 mb-2 opacity-20" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                    </svg>
                    <p class="text-sm">"No results found"</p>
                </div>
            </Show>

            <div class="space-y-3">
                <For
                    each=move || results.get()
                    key=|res| res.id
                    children=move |res| {
                        let id = res.id;
                        let is_selected = move || selected_context.get().contains(&id);
                        let category_name = res.category_name.clone();
                        let snippet = RwSignal::new(res.snippet.clone());
                        let snippet_get = move || snippet.get();

                        let category_name_clone = category_name.clone();
                        let res_content = res.content.clone();
                        let res_content_send = res.content.clone();
                        let combined_score = res.combined_score;
                        let raw_bm25_score = res.raw_bm25_score;

                        view! {
                            <div class="group bg-white rounded-lg border border-gray-200 shadow-sm hover:shadow-md transition-all overflow-hidden"
                                 class:ring-2=is_selected
                                 class:ring-blue-500=is_selected
                                 class:border-transparent=is_selected>
                                <div class="p-3">
                                    <div class="flex items-start gap-3">
                                        <div class="pt-0.5" on:click:stop_propagation=|_: web_sys::MouseEvent| {}>
                                            <input type="checkbox"
                                                   prop:checked=is_selected
                                                   on:change=move |_| toggle_selection(id)
                                                   class="rounded text-blue-600 focus:ring-blue-500 cursor-pointer" />
                                        </div>

                                        <div class="flex-1 min-w-0">
                                            <div class="flex justify-between items-start gap-2">
                                                <button
                                                    class="text-sm font-semibold text-blue-700 group-hover:text-blue-800 leading-tight truncate hover:underline text-left"
                                                    on:click=move |_| handle_preview_click(id)
                                                >
                                                    {res.title.clone()}
                                                </button>
                                                <div class="flex gap-1 flex-shrink-0">
                                                    // Show normalized percentage (0 = no BM25 search)
                                                    <Show when=move || { combined_score > 0.0 }>
                                                        <span class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-green-100 text-green-800"
                                                              title=format!("Normalized relevance score (raw RSV: {:.2})", raw_bm25_score)>
                                                            {format!("{:.0}%", combined_score * 100.0)}
                                                        </span>
                                                    </Show>
                                                    // Show raw RSV for BM25 searches
                                                    <Show when=move || { raw_bm25_score > 0.0 }>
                                                        <span class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-yellow-100 text-yellow-800"
                                                              title="BM25 Retrieval Status Value (raw score)">
                                                            {format!("RSV: {:.2}", raw_bm25_score)}
                                                        </span>
                                                    </Show>
                                                    {res.reranker_score.map(|reranker_score| {
                                                        view! {
                                                            <span class="inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-100 text-blue-800"
                                                                  title="Re-ranker relevance score">
                                                                {format!("RR: {:.0}%", reranker_score * 100.0)}
                                                            </span>
                                                        }
                                                    })}
                                                </div>
                                            </div>

                                            <p class="text-xs text-gray-600 mt-1 line-clamp-2 cursor-pointer hover:text-gray-700"
                                               on:click=move |_| handle_preview_click(id)>
                                                <Show when=move || snippet_get().is_some()>
                                                    {move || {
                                                        // Strip HTML tags from snippet for display
                                                        let snippet = snippet_get().unwrap_or_default();
                                                        let cleaned = snippet
                                                            .replace("<mark>", "")
                                                            .replace("</mark>", "");
                                                        cleaned
                                                    }}
                                                </Show>
                                                <Show when=move || snippet_get().is_none()>
                                                    {res.content.chars().take(150).collect::<String>()}
                                                    {if res.content.len() > 150 { "..." } else { "" }}
                                                </Show>
                                            </p>

                                            <div class="flex items-center gap-2 mt-2 text-[10px] text-gray-400">
                                                <Show when=move || category_name.is_some()>
                                                    <span class="flex items-center gap-1">
                                                        <svg class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
                                                        </svg>
                                                        {category_name_clone.clone().unwrap()}
                                                    </span>
                                                    <span>"•"</span>
                                                </Show>
                                                <div class="ml-auto flex items-center gap-1">
                                                    <button
                                                        class="text-gray-400 hover:text-gray-600 transition-colors"
                                                        on:click=move |_| handle_preview_click(id)
                                                        title="View full details"
                                                    >
                                                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                                                        </svg>
                                                    </button>
                                                    <Show when=move || on_delete.is_some()>
                                                        <button
                                                            class="text-gray-400 hover:text-red-600 transition-colors"
                                                            on:click=move |e: web_sys::MouseEvent| {
                                                                e.stop_propagation();
                                                                handle_delete_click(id);
                                                            }
                                                            title="Delete document"
                                                        >
                                                            <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                                                      d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                                                            </svg>
                                                        </button>
                                                    </Show>
                                                    <button
                                                        class="text-gray-400 hover:text-blue-600 transition-colors"
                                                        on:click=move |e: web_sys::MouseEvent| {
                                                            e.stop_propagation();
                                                            handle_copy_to_clipboard(id, res_content.clone());
                                                        }
                                                        title="Copy content to clipboard"
                                                    >
                                                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                                                  d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
                                                        </svg>
                                                    </button>
                                                    <button
                                                        class="text-gray-400 hover:text-green-600 transition-colors"
                                                        on:click=move |e: web_sys::MouseEvent| {
                                                            e.stop_propagation();
                                                            handle_send_to_chat(id, res_content_send.clone());
                                                        }
                                                        title="Paste to chat input"
                                                    >
                                                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                                                  d="M13 7l5 5m0 0l-5 5m5-5H6" />
                                                        </svg>
                                                    </button>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            </div>
                        }
                    }
                />
            </div>
        </div>
    }
}
