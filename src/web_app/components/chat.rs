use leptos::prelude::*;
use leptos::web_sys;
use uuid::Uuid;
use crate::domain::dtos::ChatMessage;
#[cfg(target_arch = "wasm32")]
use crate::web_app::services::chat::{fetch_conversation_messages, fetch_stream};

#[component]
pub fn Chat(
    #[prop(optional)] initial_context_docs: Option<Signal<Vec<Uuid>>>,
    #[prop(optional)] external_input_text: Option<Signal<String>>,
    #[prop(optional)] on_search_results: Option<Callback<Vec<crate::domain::models::SearchResult>>>,
    #[prop(optional)] reset_trigger: Option<Signal<u32>>,
    #[prop(optional)] selected_conversation_id: Option<Signal<Option<Uuid>>>,
) -> impl IntoView {
    #[allow(unused_variables)]
    let on_search_results = on_search_results;
    let (messages, set_messages) = signal(Vec::<ChatMessage>::new());
    let (input_text, set_input_text) = signal(String::new());
    let (is_streaming, set_is_streaming) = signal(false);
    let (error_message, set_error_message) = signal(String::new());

    // Track document context (can be set externally)
    let (context_docs, set_context_docs) = signal(Vec::<Uuid>::new());

    // Clear messages when reset_trigger changes
    if let Some(trigger) = reset_trigger {
        Effect::new(move |_| {
            let _trigger = trigger.get();
            leptos::logging::log!("Chat: Clearing messages due to reset trigger");
            set_messages.set(Vec::new());
            set_error_message.set(String::new());
            set_context_docs.set(Vec::new());
        });
    }

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

    // Load conversation messages when selected_conversation_id changes
    if let Some(selected_conv_id) = selected_conversation_id {
        Effect::new(move |_| {
            let _conv_id_opt = selected_conv_id.get();

            #[cfg(target_arch = "wasm32")]
            {
                if let Some(conv_id) = _conv_id_opt {
                    leptos::logging::log!("Chat: Loading conversation {}", conv_id);

                    leptos::task::spawn_local(async move {
                        match fetch_conversation_messages(conv_id).await {
                            Ok(conv_messages) => {
                                leptos::logging::log!("Chat: Loaded {} messages", conv_messages.len());
                                set_messages.set(conv_messages);
                                set_error_message.set(String::new());
                            }
                            Err(e) => {
                                leptos::logging::error!("Chat: Failed to load conversation: {}", e);
                                set_error_message.set(format!("Failed to load conversation: {}", e));
                            }
                        }
                    });
                }
            }
        });
    }

    let do_send = move || {
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

            // First, do vector search to find relevant documents
            let search_query = user_message.clone();

            leptos::task::spawn_local(async move {
                // Perform vector search first
                let search_request = serde_json::json!({
                    "query": search_query,
                    "limit": 10,
                    "bm25_weight": 0.0,
                    "vector_weight": 1.0,
                });

                let window = match web_sys::window() {
                    Some(w) => w,
                    None => {
                        set_error_message.set("No window available".to_string());
                        set_is_streaming.set(false);
                        return;
                    }
                };

                let search_opts = web_sys::RequestInit::new();
                search_opts.set_method("POST");
                search_opts.set_body(&wasm_bindgen::JsValue::from_str(&search_request.to_string()));

                let search_req = match web_sys::Request::new_with_str_and_init("/api/search", &search_opts) {
                    Ok(r) => r,
                    Err(_) => {
                        set_error_message.set("Failed to create search request".to_string());
                        set_is_streaming.set(false);
                        return;
                    }
                };

                search_req.headers().set("Content-Type", "application/json").ok();

                // Execute search
                let search_docs = match wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&search_req)).await {
                    Ok(resp_value) => {
                        let resp: web_sys::Response = resp_value.into();
                        if resp.ok() {
                            match wasm_bindgen_futures::JsFuture::from(resp.text().unwrap()).await {
                                Ok(text_value) => {
                                    // Parse search results from text
                                    let text = text_value.as_string().unwrap_or_default();
                                    let results: Result<Vec<crate::domain::models::SearchResult>, serde_json::Error> =
                                        serde_json::from_str(&text);
                                    match results {
                                        Ok(docs) => {
                                            leptos::logging::log!("Chat: Found {} docs via vector search", docs.len());

                                            // Send results to parent component for display
                                            if let Some(cb) = on_search_results {
                                                cb.run(docs.clone());
                                            }

                                            let doc_ids: Vec<uuid::Uuid> = docs.iter().map(|d| d.id).collect();
                                            doc_ids
                                        }
                                        Err(e) => {
                                            leptos::logging::error!("Chat: Failed to parse search results: {:?}", e);
                                            Vec::new()
                                        }
                                    }
                                }
                                Err(_) => Vec::new(),
                            }
                        } else {
                            Vec::new()
                        }
                    }
                    Err(_) => Vec::new(),
                };

                // Update context docs with search results
                set_context_docs.set(search_docs.clone());

                // Capture signal values before entering async context
                let current_messages_raw = messages.get();
                let current_conv_id = selected_conversation_id.and_then(|s| s.get());

                let current_messages: Vec<dtos::ChatMessage> = current_messages_raw
                    .into_iter()
                    .map(|m| dtos::ChatMessage {
                        role: m.role,
                        content: m.content,
                    })
                    .collect();

                let doc_ids = if search_docs.is_empty() {
                    None
                } else {
                    Some(search_docs.clone())
                };

                let request_body = serde_json::json!({
                    "messages": current_messages,
                    "conversation_id": current_conv_id,
                    "document_ids": doc_ids,
                    "context_chunks": 5,
                    "dual_agents": true,
                });

                leptos::logging::log!("Chat: Sending dual-agent chat request with {} docs", doc_ids.as_ref().map(|d: &Vec<Uuid>| d.len()).unwrap_or(0));

                set_messages.update(|msgs| {
                    msgs.push(ChatMessage {
                        role: "moderator".to_string(),
                        content: String::new(),
                    });
                    msgs.push(ChatMessage {
                        role: "rag_reporter".to_string(),
                        content: String::new(),
                    });
                });

                let result = fetch_stream(
                    "/api/chat/conversation/stream",
                    &request_body.to_string(),
                    move |chunk, agent| {
                        let agent = agent.unwrap_or_else(|| "moderator".to_string());
                        leptos::logging::log!("Chat: Received chunk from {}: {}", agent, chunk);
                        set_messages.update(|msgs| {
                            if let Some(msg) = msgs.iter_mut().rev().find(|m| m.role == agent) {
                                msg.content.push_str(&chunk);
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

    // Mouse click handler
    let send_message = move |_ev| {
        do_send();
    };

    // Handle enter key
    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            do_send();
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
                                        let agent_label = match msg.role.as_str() {
                                            "moderator" => "Moderator",
                                            "rag_reporter" => "RAG Reporter",
                                            _ => ""
                                        };
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
                                                    } else if msg.role == "rag_reporter" {
                                                        "max-w-[80%] px-4 py-2 rounded-lg bg-green-50 border border-green-200 text-gray-900"
                                                    } else {
                                                        "max-w-[80%] px-4 py-2 rounded-lg bg-gray-100 text-gray-900"
                                                    }
                                                }>
                                                    <Show when=move || !is_user && !agent_label.is_empty()>
                                                        <div class="text-xs font-semibold text-gray-600 mb-1">
                                                            {agent_label}
                                                        </div>
                                                    </Show>
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
