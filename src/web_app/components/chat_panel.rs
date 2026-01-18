use crate::domain::models::SearchResult;
use leptos::prelude::*;
use leptos::*;
use uuid::Uuid;

#[component]
pub fn ChatPanel(
    results: Signal<Vec<SearchResult>>,
    search_query: Signal<String>,
    #[prop(optional)] selected_context: Option<Signal<Vec<Uuid>>>,
    #[prop(optional)] ai_mode_enabled: Option<Signal<bool>>,
) -> impl IntoView {
    // Suppress unused warning for SSR
    #[cfg(not(target_arch = "wasm32"))]
    let _ = &selected_context;

    let (response_text, set_response_text) = signal(String::new());
    let (is_streaming, set_is_streaming) = signal(false);
    let (error_message, set_error_message) = signal(String::new());

    // Trigger chat manually
    let trigger_synthesis = move |_: web_sys::MouseEvent| {
        let query = search_query.get();
        let current_results = results.get();

        if query.trim().is_empty() || current_results.is_empty() {
            return;
        }

        set_is_streaming.set(true);
        set_response_text.set(String::from("Generating synthesis..."));
        set_error_message.set(String::new());

        // Create a simple fetch request using standard browser APIs
        #[cfg(target_arch = "wasm32")]
        {
            let query_clone = query.clone();

            // Extract document IDs from results or selection
            let document_ids: Vec<Uuid> = if let Some(selected) = selected_context {
                let ids = selected.get();
                if !ids.is_empty() {
                    ids
                } else {
                    current_results.iter().take(5).map(|r| r.id).collect()
                }
            } else {
                current_results.iter().take(5).map(|r| r.id).collect()
            };

            let chat_message = format!("Please provide a markdown summary of these search results for the query '{}'. Be concise and well-structured.", query_clone);

            // Build the request body
            let request_body = serde_json::json!({
                "message": chat_message,
                "document_ids": document_ids,
                "context_chunks": 5,
            });

            leptos::task::spawn_local(async move {
                let result = fetch_stream(
                    "/api/chat/stream",
                    &request_body.to_string(),
                    move |chunk| {
                        set_response_text.update(|text| text.push_str(&chunk));
                    },
                )
                .await;

                match result {
                    Ok(_) => {
                        set_is_streaming.set(false);
                    }
                    Err(e) => {
                        set_error_message.set(e);
                        set_is_streaming.set(false);
                    }
                }
            });
        }
    };

    // Auto-trigger synthesis if AI mode is enabled and results change
    Effect::new(move |_| {
        if let Some(ai_enabled) = ai_mode_enabled {
            if ai_enabled.get() {
                let current_results = results.get();
                let query = search_query.get();

                if !query.is_empty()
                    && !current_results.is_empty()
                    && !is_streaming.get()
                    && response_text.get().is_empty()
                {
                    // Manually trigger synthesis without needing MouseEvent
                    #[cfg(target_arch = "wasm32")]
                    {
                        set_is_streaming.set(true);
                        set_response_text.set(String::from("Generating synthesis..."));
                        set_error_message.set(String::new());

                        let document_ids: Vec<Uuid> = if let Some(selected) = selected_context {
                            let ids = selected.get();
                            if !ids.is_empty() {
                                ids
                            } else {
                                current_results.iter().take(5).map(|r| r.id).collect()
                            }
                        } else {
                            current_results.iter().take(5).map(|r| r.id).collect()
                        };

                        let chat_message = format!("Please provide a markdown summary of these search results for the query '{}'. Be concise and well-structured.", query.clone());

                        let request_body = serde_json::json!({
                            "message": chat_message,
                            "document_ids": document_ids,
                            "context_chunks": 5,
                        });

                        leptos::task::spawn_local(async move {
                            let result = fetch_stream(
                                "/api/chat/stream",
                                &request_body.to_string(),
                                move |chunk| {
                                    set_response_text.update(|text| text.push_str(&chunk));
                                },
                            )
                            .await;

                            match result {
                                Ok(_) => {
                                    set_is_streaming.set(false);
                                }
                                Err(e) => {
                                    set_error_message.set(e);
                                    set_is_streaming.set(false);
                                }
                            }
                        });
                    }
                }
            }
        }
    });

    view! {
        <div class="flex flex-col h-full overflow-hidden bg-white">
            <div class="px-4 py-3 border-b border-gray-200 bg-gray-50 flex justify-between items-center">
                <h2 class="text-sm font-bold text-gray-700">"Synthesis & Chat"</h2>
                <div class="flex items-center gap-2">
                    <Show
                        when=move || is_streaming.get()
                        fallback=move || {
                            view! {
                                <button
                                    on:click=trigger_synthesis
                                    class="px-2 py-1 text-xs bg-blue-600 text-white rounded hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
                                    disabled=move || results.get().is_empty()
                                >
                                    "Generate Synthesis"
                                </button>
                            }
                        }
                    >
                        <span class="text-xs text-blue-500 animate-pulse">"Generating..."</span>
                    </Show>
                </div>
            </div>

            // Content area
            <div class="flex-1 overflow-y-auto p-4 space-y-4">
                <Show
                    when=move || !error_message.get().is_empty()
                    fallback=move || {
                        view! {
                            <Show
                                when=move || {
                                    let text = response_text.get();
                                    let streaming = is_streaming.get();
                                    text.is_empty() && !streaming
                                }
                                fallback=move || view! {
                                    <div class="prose prose-sm max-w-none text-gray-700 whitespace-pre-wrap leading-relaxed">
                                        {move || response_text.get()}
                                    </div>
                                }
                            >
                                <div class="flex items-center justify-center h-full text-gray-400">
                                    <p class="text-center text-sm">"Run a search to generate synthesis..."</p>
                                </div>
                            </Show>
                        }
                    }
                >
                    <div class="bg-red-50 border border-red-200 rounded-md p-3">
                        <p class="text-sm text-red-700">{move || error_message.get()}</p>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_stream(
    url: &str,
    body: &str,
    on_chunk: impl Fn(String) + 'static,
) -> Result<(), String> {
    use futures::StreamExt;
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use wasm_streams::ReadableStream;

    let window = web_sys::window().ok_or("No window")?;

    // Create request options
    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(body));

    let request = web_sys::Request::new_with_str_and_init(url, &init)
        .map_err(|_| "Failed to create request".to_string())?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|_| "Failed to set header".to_string())?;

    // Fetch and convert Promise to Future
    let promise = window.fetch_with_request(&request);
    let resp_promise: JsFuture = promise.into();
    let resp = resp_promise.await.map_err(|_| "Fetch failed".to_string())?;

    let resp = web_sys::Response::from(resp);

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let body = resp.body().ok_or("No response body")?;
    let stream = ReadableStream::from_raw(body.unchecked_into()).into_stream();
    let mut stream = stream.map(|chunk| {
        let chunk = chunk.map_err(|_| "Stream error")?;
        let chunk = chunk.unchecked_into::<js_sys::Uint8Array>();
        let vec = chunk.to_vec();
        String::from_utf8(vec).map_err(|_| "Invalid UTF-8")
    });

    let mut buffer = String::new();

    while let Some(result) = stream.next().await {
        match result {
            Ok(text) => {
                buffer.push_str(&text);
                // Process buffer for SSE lines
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].to_string();
                    buffer.drain(..=pos); // Remove line and newline

                    if let Some(data) = line.strip_prefix("data: ") {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some("chunk") = json.get("type").and_then(|t| t.as_str()) {
                                if let Some(content) = json.get("content").and_then(|c| c.as_str())
                                {
                                    on_chunk(content.to_string());
                                }
                            } else if let Some("error") = json.get("type").and_then(|t| t.as_str())
                            {
                                if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
                                    return Err(msg.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
async fn fetch_stream(
    _url: &str,
    _body: &str,
    _on_chunk: impl Fn(String) + 'static,
) -> Result<(), String> {
    Err("Client-side only".to_string())
}
