use leptos::*;
use leptos::prelude::*;
use leptos::task::spawn_local;
use uuid::Uuid;
use crate::web_app::components::{
    search_bar::SearchBar,
    filters::Filters,
    results_list::ResultsList,
    chat_panel::{ChatPanel, Message},
};
use crate::types::*;

#[component]
pub fn SearchPage() -> impl IntoView {
    // State
    let (search_query, set_search_query) = signal(String::new());
    let (results, set_results) = signal(Vec::<SearchResult>::new());
    let (loading, set_loading) = signal(false);
    let (categories, set_categories) = signal(Vec::<Category>::new());
    let (selected_category, set_selected_category) = signal(None::<Uuid>);

    let (bm25_weight, set_bm25_weight) = signal(0.5);
    let (vector_weight, set_vector_weight) = signal(0.5);

    let (selected_context, set_selected_context) = signal(Vec::<Uuid>::new());
    let (auto_context_enabled, set_auto_context_enabled) = signal(true);
    let (context_count, set_context_count) = signal(0);

    let (messages, set_messages) = signal(Vec::<Message>::new());
    let (chat_loading, set_chat_loading) = signal(false);
    let (conversation_id, set_conversation_id) = signal(None::<Uuid>);

    // Derived state for context count
    Effect::new(move |_| {
        let count = if auto_context_enabled.get() && selected_context.get().is_empty() {
            results.get().len().min(5)
        } else {
            selected_context.get().len()
        };
        set_context_count.set(count);
    });

    // Load categories on mount
    Effect::new(move |_| {
        spawn_local(async move {
            if let Ok(res) = reqwest::get("/api/categories").await {
                if let Ok(cats) = res.json::<Vec<Category>>().await {
                    set_categories.set(cats);
                }
            }
        });
    });

    let search = move || {
        let query = search_query.get();
        if query.trim().is_empty() {
            set_results.set(vec![]);
            return;
        }

        set_loading.set(true);
        spawn_local(async move {
            let req = SearchRequest {
                query,
                limit: 50,
                bm25_weight: bm25_weight.get(),
                vector_weight: vector_weight.get(),
                category_id: selected_category.get(),
                date_from: None,
                date_to: None,
                locations: None,
                keywords: None,
            };

            let client = reqwest::Client::new();
            if let Ok(res) = client.post("/api/search").json(&req).send().await {
                if let Ok(data) = res.json::<Vec<SearchResult>>().await {
                    set_results.set(data);
                    
                    // If auto-context is enabled, we clear manual selection so it defaults to top N
                    if auto_context_enabled.get() {
                        set_selected_context.set(vec![]); 
                    }
                }
            }
            set_loading.set(false);
        });
    };

    let send_chat = move |(msg, with_search): (String, bool)| {
        let user_msg = Message {
            id: chrono::Utc::now().timestamp_millis(),
            role: "user".to_string(),
            content: msg.clone(),
            sources: None,
        };
        
        set_messages.update(|m| m.push(user_msg));
        set_chat_loading.set(true);

        if with_search {
            set_search_query.set(msg.clone());
            search();
        }

        spawn_local(async move {
            // Determine context IDs
            let context_ids = if auto_context_enabled.get() && selected_context.get().is_empty() {
                results.get().iter().take(5).map(|r| r.id).collect()
            } else {
                selected_context.get()
            };

            let req = ChatRequest {
                message: msg,
                conversation_id: conversation_id.get(),
                context_chunks: if context_ids.is_empty() { 5 } else { context_ids.len() as i32 * 2 },
                document_ids: if context_ids.is_empty() { None } else { Some(context_ids) },
            };

            let client = reqwest::Client::new();
            if let Ok(res) = client.post("/api/chat").json(&req).send().await {
                if let Ok(data) = res.json::<ChatResponse>().await {
                    set_conversation_id.set(Some(data.conversation_id));
                    let bot_msg = Message {
                        id: chrono::Utc::now().timestamp_millis() + 1,
                        role: "assistant".to_string(),
                        content: data.message,
                        sources: Some(data.sources),
                    };
                    set_messages.update(|m| m.push(bot_msg));
                } else {
                     let error_msg = Message {
                        id: chrono::Utc::now().timestamp_millis() + 1,
                        role: "assistant".to_string(),
                        content: "Sorry, I encountered an error.".to_string(),
                        sources: None,
                    };
                    set_messages.update(|m| m.push(error_msg));
                }
            }
            set_chat_loading.set(false);
        });
    };

    let clear_chat = move || {
        set_messages.set(vec![]);
        set_conversation_id.set(None);
    };

    view! {
        <div class="flex h-full">
            // Left Column
            <aside class="w-[45%] bg-gray-50 border-r border-gray-200 flex flex-col shadow-lg z-10 h-screen">
                // Header
                <div class="px-6 py-4 bg-white border-b border-gray-200 flex justify-between items-center">
                    <div>
                        <h1 class="text-xl font-bold text-gray-900 tracking-tight">"📚 RAG Search"</h1>
                        <p class="text-xs text-gray-500">"Hybrid Document Search & Discovery"</p>
                    </div>
                    <div class="text-xs text-gray-400">
                        <span>{move || results.get().len()}</span> " results"
                    </div>
                </div>

                <SearchBar
                    query=search_query.into()
                    set_query=set_search_query
                    on_search=search
                    bm25_weight=bm25_weight.into()
                    set_bm25_weight=set_bm25_weight
                    vector_weight=vector_weight.into()
                    set_vector_weight=set_vector_weight
                />
                
                <Filters
                    categories=categories.into()
                    selected_category=selected_category.into()
                    set_selected_category=set_selected_category
                    on_change=search
                />
                
                <ResultsList
                    results=results.into()
                    loading=loading.into()
                    selected_context=selected_context.into()
                    set_selected_context=set_selected_context
                />
            </aside>

            // Right Column
            <main class="w-[55%] flex flex-col bg-white h-full relative h-screen">
                <ChatPanel
                    messages=messages.into()
                    set_messages=set_messages
                    loading=chat_loading.into()
                    on_send=Callback::new(send_chat)
                    on_clear=Callback::new(move |_| clear_chat())
                    context_count=context_count.into()
                    auto_context_enabled=auto_context_enabled.into()
                    set_auto_context_enabled=set_auto_context_enabled
                />
            </main>
        </div>
    }
}
