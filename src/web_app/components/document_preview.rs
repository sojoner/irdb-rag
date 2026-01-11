use leptos::*;
use leptos::prelude::*;
use uuid::Uuid;
use crate::domain::models::Document;

#[server(GetDocument, "/api")]
pub async fn get_document(id: Uuid) -> Result<Option<Document>, ServerFnError> {
    use crate::infra::db;
    use crate::api::state::AppState;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found"))?;

    let doc = db::get_document(&state.pool, id).await
        .map_err(|e| ServerFnError::new(e.to_string()))?;

    Ok(doc)
}

#[component]
pub fn DocumentPreview(
    #[prop(into)] document_id: Signal<Option<Uuid>>,
    #[prop(into)] on_close: Callback<()>,
) -> impl IntoView {
    let document = Resource::new(
        move || document_id.get(),
        |id| async move {
            match id {
                Some(id) => get_document(id).await,
                None => Ok(None),
            }
        }
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
                        {move || document.get().map(|res| match res {
                            Ok(Some(doc)) => view! {
                                <DocumentDetails doc=doc />
                            }.into_any(),
                            Ok(None) => view! { <p>"Document not found"</p> }.into_any(),
                            Err(e) => view! { <p class="text-red-500">{e.to_string()}</p> }.into_any(),
                        })}
                    </Suspense>
                </div>
            </div>
        </Show>
    }
}

#[component]
fn DocumentDetails(doc: Document) -> impl IntoView {
    let summary_check = doc.summary.clone();
    let summary_view = doc.summary.clone();
    
    let keywords_check = doc.keywords.clone();
    let keywords_view = doc.keywords.clone();
    
    let locations_check = doc.locations.clone();
    let locations_view = doc.locations.clone();

    view! {
        <article class="prose max-w-none">
            <h1 class="text-2xl font-bold mb-4 text-gray-900">{doc.title}</h1>

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
