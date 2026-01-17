use leptos::prelude::*;
use leptos::web_sys;
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct ConversationListItem {
    pub id: Uuid,
    pub title: Option<String>,
    pub message_count: i64,
    pub updated_at: String,
}

#[component]
pub fn ConversationList(
    #[prop(optional)] on_conversation_select: Option<Callback<Uuid>>,
    #[prop(optional)] on_new_conversation: Option<Callback<()>>,
    #[prop(optional)] on_delete_conversation: Option<Callback<Uuid>>,
) -> impl IntoView {
    let (conversations, set_conversations) = signal(Vec::<ConversationListItem>::new());
    let (loading, _set_loading) = signal(false);
    let (error_message, _set_error_message) = signal(String::new());

    // Trigger for reloading conversations
    let (reload_trigger, set_reload_trigger) = signal(0u32);

    // Load conversations on mount and when reload_trigger changes
    Effect::new(move |_| {
        // React to reload_trigger changes
        let _trigger = reload_trigger.get();

        #[cfg(target_arch = "wasm32")]
        {
            leptos::task::spawn_local(async move {
                _set_loading.set(true);
                _set_error_message.set(String::new());

                match fetch_conversations().await {
                    Ok(convos) => {
                        leptos::logging::log!("Loaded {} conversations", convos.len());
                        set_conversations.set(convos);
                        _set_loading.set(false);
                    }
                    Err(e) => {
                        leptos::logging::error!("Failed to load conversations: {}", e);
                        _set_error_message.set(format!("Failed to load conversations: {}", e));
                        _set_loading.set(false);
                    }
                }
            });
        }
    });

    let handle_new_conversation = move |_| {
        if let Some(callback) = on_new_conversation {
            callback.run(());
        }
        // Reload conversation list after creating new conversation
        set_reload_trigger.update(|t| *t += 1);
    };

    let handle_conversation_click = move |id: Uuid| {
        move |_| {
            if let Some(callback) = on_conversation_select {
                callback.run(id);
            }
        }
    };

    let handle_delete = move |id: Uuid| {
        move |ev: web_sys::MouseEvent| {
            ev.stop_propagation(); // Prevent triggering the conversation select
            if let Some(callback) = on_delete_conversation {
                callback.run(id);
            }
            // Reload conversation list after delete
            set_reload_trigger.update(|t| *t += 1);
        }
    };

    view! {
        <div class="flex flex-col h-full bg-gray-50 border-r border-gray-200">
            // Header with "New Chat" button
            <div class="px-4 py-3 border-b border-gray-200 bg-white">
                <button
                    class="w-full px-4 py-2 bg-blue-600 text-white rounded-md hover:bg-blue-700 flex items-center justify-center gap-2"
                    on:click=handle_new_conversation
                >
                    <span>"+"</span>
                    <span>"New Chat"</span>
                </button>
            </div>

            // Conversations list
            <div class="flex-1 overflow-y-auto">
                <Show
                    when=move || loading.get()
                    fallback=move || view! {
                        <Show
                            when=move || !error_message.get().is_empty()
                            fallback=move || view! {
                                <Show
                                    when=move || conversations.get().is_empty()
                                    fallback=move || view! {
                                        <div class="divide-y divide-gray-200">
                                            <For
                                                each=move || conversations.get()
                                                key=|convo| convo.id
                                                children=move |convo: ConversationListItem| {
                                                    let convo_id = convo.id;
                                                    let delete_id = convo.id;
                                                    view! {
                                                        <div
                                                            class="px-4 py-3 hover:bg-gray-100 cursor-pointer group relative"
                                                            on:click=handle_conversation_click(convo_id)
                                                        >
                                                            <div class="flex justify-between items-start">
                                                                <div class="flex-1 min-w-0">
                                                                    <h3 class="text-sm font-medium text-gray-900 truncate">
                                                                        {move || convo.title.clone().unwrap_or_else(|| "Untitled".to_string())}
                                                                    </h3>
                                                                    <p class="text-xs text-gray-500 mt-1">
                                                                        {move || format!("{} messages", convo.message_count)}
                                                                    </p>
                                                                </div>
                                                                <button
                                                                    class="ml-2 opacity-0 group-hover:opacity-100 text-red-600 hover:text-red-800"
                                                                    on:click=handle_delete(delete_id)
                                                                    title="Delete conversation"
                                                                >
                                                                    "×"
                                                                </button>
                                                            </div>
                                                        </div>
                                                    }
                                                }
                                            />
                                        </div>
                                    }
                                >
                                    <div class="px-4 py-8 text-center text-gray-500">
                                        <p class="text-sm">"No conversations yet"</p>
                                        <p class="text-xs mt-1">"Click 'New Chat' to start"</p>
                                    </div>
                                </Show>
                            }
                        >
                            <div class="px-4 py-3 bg-red-50 text-red-700 text-sm">
                                {move || error_message.get()}
                            </div>
                        </Show>
                    }
                >
                    <div class="px-4 py-8 text-center text-gray-500">
                        <div class="animate-pulse">"Loading conversations..."</div>
                    </div>
                </Show>
            </div>
        </div>
    }
}

#[cfg(target_arch = "wasm32")]
async fn fetch_conversations() -> Result<Vec<ConversationListItem>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;

    let window = web_sys::window().ok_or("No window")?;

    let resp_promise = window.fetch_with_str("/api/conversations");
    let resp = JsFuture::from(resp_promise)
        .await
        .map_err(|_| "Fetch failed".to_string())?;

    let resp: web_sys::Response = resp.dyn_into().map_err(|_| "Invalid response")?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let json_promise = resp
        .json()
        .map_err(|_| "Failed to parse response")?;
    let json = JsFuture::from(json_promise)
        .await
        .map_err(|_| "Failed to read JSON")?;

    let json_str = js_sys::JSON::stringify(&json)
        .map_err(|_| "Failed to stringify")?
        .as_string()
        .ok_or("Failed to convert to string")?;

    #[derive(serde::Deserialize)]
    struct ConversationsResponse {
        conversations: Vec<ConversationData>,
    }

    #[derive(serde::Deserialize)]
    struct ConversationData {
        id: String,
        title: Option<String>,
        message_count: i64,
        updated_at: String,
    }

    let response: ConversationsResponse = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    Ok(response
        .conversations
        .into_iter()
        .map(|c| ConversationListItem {
            id: Uuid::parse_str(&c.id).unwrap_or_default(),
            title: c.title,
            message_count: c.message_count,
            updated_at: c.updated_at,
        })
        .collect())
}

#[cfg(not(target_arch = "wasm32"))]
async fn fetch_conversations() -> Result<Vec<ConversationListItem>, String> {
    Err("Client-side only".to_string())
}
