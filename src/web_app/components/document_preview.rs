use crate::domain::models::Document;
use leptos::prelude::*;
use uuid::Uuid;
use leptos::web_sys;

#[server(GetDocument, "/api")]
pub async fn get_document(id: Uuid) -> Result<Option<Document>, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;

    let state =
        use_context::<AppState>().ok_or_else(|| ServerFnError::new("AppState not found"))?;

    let doc = db::get_document(&state.pool, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(doc)
}

#[component]
pub fn DocumentPreview(
    #[prop(into)] document_id: Signal<Option<Uuid>>,
    #[prop(into)] on_close: Callback<()>,
    #[prop(optional)] set_chat_input: Option<leptos::prelude::WriteSignal<String>>,
) -> impl IntoView {
    let document = Resource::new(
        move || document_id.get(),
        |id| async move {
            match id {
                Some(id) => get_document(id).await,
                None => Ok(None),
            }
        },
    );

    view! {
        <Show when=move || document_id.get().is_some()>
            <div class="fixed inset-0 bg-black/50 flex items-center justify-center z-50"
                 on:click=move |_| on_close.run(())>
                <div class="bg-white rounded-lg w-3/4 h-5/6 p-6 overflow-auto shadow-xl relative"
                     on:click:stop_propagation=|_: web_sys::MouseEvent| {}>
                    <button
                        class="absolute top-4 right-4 text-gray-500 hover:text-gray-700"
                        on:click=move |_| on_close.run(())
                    >
                        <svg class="h-6 w-6" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                        </svg>
                    </button>

                    <Suspense fallback=|| view! {
                        <div class="flex justify-center items-center h-full">
                            <div class="animate-spin rounded-full h-12 w-12 border-b-2 border-blue-500"></div>
                        </div>
                    }>
                        {move || {
                            document.get().map(|res| match res {
                                Ok(Some(doc)) => {
                                    match set_chat_input {
                                        Some(setter) => view! {
                                            <DocumentDetailsInline doc=doc.clone() set_chat_input=setter />
                                        }.into_any(),
                                        None => view! {
                                            <DocumentDetailsInline doc=doc.clone() />
                                        }.into_any(),
                                    }
                                },
                                Ok(None) => view! { <p>"Document not found"</p> }.into_any(),
                                Err(e) => view! { <p class="text-red-500">{e.to_string()}</p> }.into_any(),
                            })
                        }}
                    </Suspense>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn DocumentDetailsInline(
    doc: Document,
    #[prop(optional)] set_chat_input: Option<leptos::prelude::WriteSignal<String>>,
) -> impl IntoView {
    let summary_check = doc.summary.clone();
    let summary_view = doc.summary.clone();

    let keywords_check = doc.keywords.clone();
    let keywords_view = doc.keywords.clone();

    let locations_check = doc.locations.clone();
    let locations_view = doc.locations.clone();

    let handle_copy_to_clipboard = move |content: String| {
        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Some(navigator) = window.navigator().clipboard() {
                    let promise = navigator.write_text(&content);
                    let _ = wasm_bindgen_futures::JsFuture::from(promise);
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let _ = content;
        }
    };

    let handle_send_to_chat = move |content: String| {
        if let Some(setter) = set_chat_input {
            setter.set(content);
        }
    };

    let doc_content = doc.content.clone();
    let doc_content_copy = doc.content.clone();

    view! {
        <article class="prose max-w-none">
            <div class="flex items-center justify-between mb-4">
                <h1 class="text-2xl font-bold text-gray-900">{doc.title}</h1>
                <div class="flex gap-2">
                    <button
                        class="px-3 py-2 text-sm text-gray-600 hover:text-blue-600 hover:bg-blue-50 rounded-md transition-colors border border-gray-200 hover:border-blue-200 flex items-center gap-2"
                        on:click=move |_| handle_copy_to_clipboard(doc_content.clone())
                        title="Copy content to clipboard"
                    >
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                  d="M9 5H7a2 2 0 00-2 2v12a2 2 0 002 2h10a2 2 0 002-2V7a2 2 0 00-2-2h-2M9 5a2 2 0 002 2h2a2 2 0 002-2M9 5a2 2 0 012-2h2a2 2 0 012 2" />
                        </svg>
                        <span>"Copy"</span>
                    </button>
                    <button
                        class="px-3 py-2 text-sm text-gray-600 hover:text-green-600 hover:bg-green-50 rounded-md transition-colors border border-gray-200 hover:border-green-200 flex items-center gap-2"
                        on:click=move |_| handle_send_to_chat(doc_content_copy.clone())
                        title="Paste to chat input"
                    >
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2"
                                  d="M13 7l5 5m0 0l-5 5m5-5H6" />
                        </svg>
                        <span>"Send to Chat"</span>
                    </button>
                </div>
            </div>

            <div class="grid grid-cols-2 md:grid-cols-4 gap-4 mb-6 text-sm bg-gray-50 p-4 rounded-lg border border-gray-200">
                <div>
                    <span class="block font-semibold text-gray-500">"Type"</span>
                    <span>{doc.source_type}</span>
                </div>
                <div>
                    <span class="block font-semibold text-gray-500">"Created"</span>
                    <span>{doc.created_at.format("%Y-%m-%d").to_string()}</span>
                </div>
                <div>
                    <span class="block font-semibold text-gray-500">"Author"</span>
                    <span>{doc.author.unwrap_or_else(|| "Unknown".to_string())}</span>
                </div>
            </div>

            <Show when=move || summary_check.is_some()>
                <div class="mb-6 p-4 bg-blue-50 border-l-4 border-blue-500 rounded-r">
                    <h3 class="font-semibold text-blue-900 mb-2">"Summary"</h3>
                    <p class="text-blue-800 m-0">{summary_view.clone()}</p>
                </div>
            </Show>

            // Keywords
            <Show when=move || keywords_check.is_some()>
                <div class="flex flex-wrap gap-2 mb-4">
                    {keywords_view.clone().unwrap_or_default().into_iter().map(|kw| view! {
                        <span class="px-2 py-1 bg-gray-100 text-gray-700 rounded-full text-xs border border-gray-200">
                            {kw}
                        </span>
                    }).collect_view()}
                </div>
            </Show>

            // Locations
            <Show when=move || locations_check.is_some()>
                <div class="flex flex-wrap gap-2 mb-6">
                    {locations_view.clone().unwrap_or_default().into_iter().map(|loc| view! {
                        <span class="px-2 py-1 bg-green-50 text-green-700 rounded-full text-xs border border-green-200 flex items-center gap-1">
                            <span>"📍"</span>
                            {loc}
                        </span>
                    }).collect_view()}
                </div>
            </Show>

            <div class="border-t border-gray-200 pt-6 mt-6">
                <h3 class="text-lg font-semibold mb-4">"Content"</h3>
                <div class="whitespace-pre-wrap font-mono text-sm bg-gray-50 p-4 rounded border border-gray-200 overflow-x-auto">
                    {doc.content}
                </div>
            </div>
        </article>
    }
}
