use leptos::prelude::*;
use leptos::web_sys;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[component]
pub fn Chat(
    #[prop(optional)] initial_context_docs: Option<Signal<Vec<Uuid>>>,
    #[prop(optional)] external_input_text: Option<Signal<String>>,
) -> impl IntoView {
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (input_text, set_input_text) = signal(String::new());
    let (is_streaming, set_is_streaming) = signal(false);
    let (error_message, set_error_message) = signal(String::new());
    let (_conversation_id, _set_conversation_id) = signal(Option::<Uuid>::None);

    // Track document context (can be set externally)
    let (context_docs, set_context_docs) = signal(Vec::<Uuid>::new());

    // Initialize context docs if provided
    if let Some(initial_docs) = initial_context_docs {
        Effect::new(move |_| {
            set_context_docs.set(initial_docs.get());
        });
    }

    // Sync external input text to internal input_text if provided
    if let Some(ext_input) = external_input_text {
        Effect::new(move |_| {
            let external = ext_input.get();
            if !external.is_empty() {
                set_input_text.set(external);
            }
        });
    }

    let send_message = move |_| {
        let user_message = input_text.get().trim().to_string();

        if user_message.is_empty() {
            return;
        }

        // Add user message to the conversation
        set_messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: "user".to_string(),
                content: user_message.clone(),
            });
        });

        // Clear input
        set_input_text.set(String::new());
        set_is_streaming.set(true);
        set_error_message.set(String::new());

        // Prepare request
        #[cfg(target_arch = "wasm32")]
        {
            use crate::domain::dtos;

            let current_messages: Vec<dtos::ChatMessage> = messages
                .get()
                .into_iter()
                .map(|m| dtos::ChatMessage {
                    role: m.role,
                    content: m.content,
                })
                .collect();

            let docs = context_docs.get();
            let doc_ids = if docs.is_empty() {
                None
            } else {
                Some(docs.clone())
            };

            let request_body = serde_json::json!({
                "messages": current_messages,
                "conversation_id": _conversation_id.get(),
                "document_ids": doc_ids,
                "context_chunks": 5,
            });

            leptos::logging::log!("Chat: Sending request: {}", request_body.to_string());

            // Add placeholder for assistant response
            set_messages.update(|msgs| {
                msgs.push(ChatMessage {
                    role: "assistant".to_string(),
                    content: String::new(),
                });
            });

            leptos::task::spawn_local(async move {
                let result = fetch_stream(
                    "/api/chat/conversation/stream",
                    &request_body.to_string(),
                    move |chunk| {
                        leptos::logging::log!("Chat: Received chunk: {}", chunk);
                        set_messages.update(|msgs| {
                            if let Some(last) = msgs.last_mut() {
                                if last.role == "assistant" {
                                    last.content.push_str(&chunk);
                                }
                            }
                        });
                    },
                )
                .await;

                match result {
                    Ok(_) => {
                        leptos::logging::log!("Chat: Stream completed successfully");
                        set_is_streaming.set(false);
                    }
                    Err(e) => {
                        leptos::logging::error!("Chat: Stream failed: {}", e);
                        set_error_message.set(e);
                        set_is_streaming.set(false);
                        // Remove the empty assistant message
                        set_messages.update(|msgs| {
                            if let Some(last) = msgs.last() {
                                if last.role == "assistant" && last.content.is_empty() {
                                    msgs.pop();
                                }
                            }
                        });
                    }
                }
            });
        }
    };

    // Handle enter key
    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            // Trigger send_message with a dummy event
            #[cfg(target_arch = "wasm32")]
            {
                use wasm_bindgen::JsCast;
                if let Some(target) = ev.target() {
                    if let Ok(mouse_ev) = target.dyn_into::<web_sys::EventTarget>() {
                        let _ = mouse_ev;
                        // Actually just call send without the event parameter
                        let user_message = input_text.get().trim().to_string();

                        if user_message.is_empty() {
                            return;
                        }

                        // Add user message to the conversation
                        set_messages.update(|msgs| {
                            msgs.push(ChatMessage {
                                role: "user".to_string(),
                                content: user_message.clone(),
                            });
                        });

                        // Clear input
                        set_input_text.set(String::new());
                        set_is_streaming.set(true);
                        set_error_message.set(String::new());

                        // Prepare request
                        use crate::domain::dtos;

                        let current_messages: Vec<dtos::ChatMessage> = messages
                            .get()
                            .into_iter()
                            .map(|m| dtos::ChatMessage {
                                role: m.role,
                                content: m.content,
                            })
                            .collect();

                        let docs = context_docs.get();
                        let doc_ids = if docs.is_empty() {
                            None
                        } else {
                            Some(docs.clone())
                        };

                        let request_body = serde_json::json!({
                            "messages": current_messages,
                            "conversation_id": _conversation_id.get(),
                            "document_ids": doc_ids,
                            "context_chunks": 5,
                        });

                        leptos::logging::log!("Chat: Sending request: {}", request_body.to_string());

                        // Add placeholder for assistant response
                        set_messages.update(|msgs| {
                            msgs.push(ChatMessage {
                                role: "assistant".to_string(),
                                content: String::new(),
                            });
                        });

                        leptos::task::spawn_local(async move {
                            let result = fetch_stream(
                                "/api/chat/conversation/stream",
                                &request_body.to_string(),
                                move |chunk| {
                                    leptos::logging::log!("Chat: Received chunk: {}", chunk);
                                    set_messages.update(|msgs| {
                                        if let Some(last) = msgs.last_mut() {
                                            if last.role == "assistant" {
                                                last.content.push_str(&chunk);
                                            }
                                        }
                                    });
                                },
                            )
                            .await;

                            match result {
                                Ok(_) => {
                                    leptos::logging::log!("Chat: Stream completed successfully");
                                    set_is_streaming.set(false);
                                }
                                Err(e) => {
                                    leptos::logging::error!("Chat: Stream failed: {}", e);
                                    set_error_message.set(e);
                                    set_is_streaming.set(false);
                                    // Remove the empty assistant message
                                    set_messages.update(|msgs| {
                                        if let Some(last) = msgs.last() {
                                            if last.role == "assistant" && last.content.is_empty() {
                                                msgs.pop();
                                            }
                                        }
                                    });
                                }
                            }
                        });
                    }
                }
            }
        }
    };

    view! {
        <div class="flex flex-col h-full bg-white">
            // Header
            <div class="px-4 py-3 border-b border-gray-200 bg-gray-50 flex justify-between items-center">
                <h2 class="text-sm font-bold text-gray-700">"Chat"</h2>
                <div class="flex items-center gap-2">
                    <Show when=move || context_docs.get().is_empty()>
                        <span class="text-xs text-gray-500">"No context"</span>
                    </Show>
                    <Show when=move || !context_docs.get().is_empty()>
                        <span class="text-xs text-green-600">
                            {move || format!("{} docs in context", context_docs.get().len())}
                        </span>
                    </Show>
                </div>
            </div>

            // Messages area
            <div class="flex-1 overflow-y-auto p-4 space-y-4">
                <Show
                    when=move || messages.get().is_empty()
                    fallback=move || {
                        view! {
                            <div class="space-y-4">
                                <For
                                    each=move || messages.get()
                                    key=|msg| format!("{}:{}", msg.role, msg.content.len())
                                    children=move |msg: ChatMessage| {
                                        let is_user = msg.role == "user";
                                        view! {
                                            <div class=move || {
                                                if is_user {
                                                    "flex justify-end"
                                                } else {
                                                    "flex justify-start"
                                                }
                                            }>
                                                <div class=move || {
                                                    if is_user {
                                                        "max-w-[80%] px-4 py-2 rounded-lg bg-blue-600 text-white"
                                                    } else {
                                                        "max-w-[80%] px-4 py-2 rounded-lg bg-gray-100 text-gray-900"
                                                    }
                                                }>
                                                    <div class="text-sm whitespace-pre-wrap">
                                                        {msg.content}
                                                    </div>
                                                </div>
                                            </div>
                                        }
                                    }
                                />
                            </div>
                        }
                    }
                >
                    <div class="flex items-center justify-center h-full text-gray-400">
                        <p class="text-center text-sm">"Start a conversation..."</p>
                    </div>
                </Show>

                <Show when=move || !error_message.get().is_empty()>
                    <div class="bg-red-50 border border-red-200 rounded-md p-3">
                        <p class="text-sm text-red-700">{move || error_message.get()}</p>
                    </div>
                </Show>
            </div>

            // Input area
            <div class="border-t border-gray-200 p-4">
                <div class="flex gap-2">
                    <textarea
                        class="flex-1 px-3 py-2 border border-gray-300 rounded-md focus:outline-none focus:ring-2 focus:ring-blue-500 resize-none"
                        rows="2"
                        placeholder="Type your message... (Enter to send, Shift+Enter for new line)"
                        prop:value=move || input_text.get()
                        on:input=move |ev| set_input_text.set(event_target_value(&ev))
                        on:keydown=on_keydown
                        prop:disabled=move || is_streaming.get()
                    />
                    <button
                        class="px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 disabled:opacity-50 disabled:cursor-not-allowed"
                        on:click=send_message
                        disabled=move || is_streaming.get() || input_text.get().trim().is_empty()
                    >
                        <Show
                            when=move || is_streaming.get()
                            fallback=move || view! { <span>"Send"</span> }
                        >
                            <span class="animate-pulse">"..."</span>
                        </Show>
                    </button>
                </div>
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

    leptos::logging::log!("fetch_stream: Starting request to {}", url);
    leptos::logging::log!("fetch_stream: Body: {}", body);

    let window = web_sys::window().ok_or("No window")?;

    // Create request options
    let mut init = web_sys::RequestInit::new();
    init.method("POST");
    init.body(Some(&JsValue::from_str(body)));

    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| {
        leptos::logging::error!("Failed to create request: {:?}", e);
        "Failed to create request".to_string()
    })?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|_| "Failed to set header".to_string())?;

    // Fetch and convert Promise to Future
    let promise = window.fetch_with_request(&request);
    let resp_promise: JsFuture = promise.into();
    let resp = resp_promise.await.map_err(|_| "Fetch failed".to_string())?;

    let resp = web_sys::Response::from(resp);

    leptos::logging::log!("fetch_stream: Response status: {}", resp.status());

    if !resp.ok() {
        let error_msg = format!("HTTP {}", resp.status());
        leptos::logging::error!("fetch_stream: {}", error_msg);
        return Err(error_msg);
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
                leptos::logging::log!("fetch_stream: Buffer after push: {}", buffer);
                // Process buffer for SSE lines
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].to_string();
                    buffer.drain(..=pos); // Remove line and newline

                    leptos::logging::log!("fetch_stream: Processing line: {}", line);

                    if let Some(data) = line.strip_prefix("data: ") {
                        leptos::logging::log!("fetch_stream: Found data line: {}", data);
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            leptos::logging::log!("fetch_stream: Parsed JSON: {}", json);
                            if let Some("chunk") = json.get("type").and_then(|t| t.as_str()) {
                                if let Some(content) = json.get("content").and_then(|c| c.as_str())
                                {
                                    leptos::logging::log!(
                                        "fetch_stream: Got chunk content: {}",
                                        content
                                    );
                                    on_chunk(content.to_string());
                                }
                            } else if let Some("error") = json.get("type").and_then(|t| t.as_str())
                            {
                                if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
                                    let error_msg = format!("Server error: {}", msg);
                                    leptos::logging::error!("fetch_stream: {}", error_msg);
                                    return Err(error_msg);
                                }
                            }
                        } else {
                            leptos::logging::warn!("fetch_stream: Failed to parse JSON: {}", data);
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
