use leptos::*;
use leptos::prelude::*;
use leptos::html;
use crate::types::SourceReference;

#[derive(Clone, Debug)]
pub struct Message {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub sources: Option<Vec<SourceReference>>,
}

#[component]
pub fn ChatPanel(
    messages: Signal<Vec<Message>>,
    #[allow(unused_variables)]
    set_messages: WriteSignal<Vec<Message>>,
    loading: Signal<bool>,
    on_send: Callback<(String, bool)>, // (message, with_search)
    on_clear: Callback<()>,
    context_count: Signal<usize>,
    auto_context_enabled: Signal<bool>,
    set_auto_context_enabled: WriteSignal<bool>,
) -> impl IntoView {
    let (input, set_input) = signal(String::new());
    let chat_container_ref: NodeRef<html::Div> = NodeRef::new();

    // Auto-scroll to bottom when messages change
    Effect::new(move |_| {
        messages.get(); // dependency
        if let Some(div) = chat_container_ref.get() {
            div.set_scroll_top(div.scroll_height());
        }
    });

    view! {
        // Header
        <header class="px-6 py-4 border-b border-gray-200 bg-white flex justify-between items-center shadow-sm z-10">
            <div class="flex items-center gap-3">
                <div class="p-2 bg-purple-100 rounded-lg text-purple-600">
                    <svg class="h-5 w-5" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M8 10h.01M12 10h.01M16 10h.01M9 16H5a2 2 0 01-2-2V6a2 2 0 012-2h14a2 2 0 012 2v8a2 2 0 01-2 2h-5l-5 5v-5z" />
                    </svg>
                </div>
                <div>
                    <h2 class="text-lg font-bold text-gray-900">"RAG Assistant"</h2>
                    <div class="flex items-center gap-2 text-xs">
                        <span class="flex items-center gap-1"
                              class=("text-green-600", move || context_count.get() > 0)
                              class=("text-gray-400", move || context_count.get() == 0)>
                            <span class="w-1.5 h-1.5 rounded-full"
                                  class=("bg-green-500", move || context_count.get() > 0)
                                  class=("bg-gray-300", move || context_count.get() == 0)></span>
                            <span>{move || context_count.get()} " docs in context"</span>
                        </span>
                        <span class="text-gray-300">"|"</span>
                        <label class="flex items-center gap-1 cursor-pointer hover:text-purple-600 transition-colors">
                            <input type="checkbox"
                                   prop:checked=auto_context_enabled
                                   on:change=move |ev| set_auto_context_enabled.set(event_target_checked(&ev))
                                   class="rounded text-purple-600 focus:ring-purple-500 w-3 h-3" />
                            <span>"Auto-select top 5"</span>
                        </label>
                    </div>
                </div>
            </div>
            <button on:click=move |_| on_clear.run(())
                    class="text-xs text-gray-500 hover:text-red-600 px-3 py-1.5 rounded hover:bg-red-50 transition-colors">
                "Clear Chat"
            </button>
        </header>

        // Chat Area
        <div class="flex-1 overflow-y-auto p-6 space-y-6 bg-white" node_ref=chat_container_ref>
            <Show when=move || messages.get().is_empty()>
                <div class="h-full flex flex-col items-center justify-center text-center opacity-60">
                    <div class="w-16 h-16 bg-purple-50 rounded-2xl flex items-center justify-center mb-4 text-purple-500">
                        <svg class="h-8 w-8" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                        </svg>
                    </div>
                    <h3 class="text-lg font-medium text-gray-900">"Ready to help"</h3>
                    <p class="text-sm text-gray-500 max-w-xs mt-1">"Select documents from the left or just ask a question to get started."</p>
                </div>
            </Show>

            <For
                each=move || messages.get()
                key=|msg| msg.id
                children=move |msg| {
                    let is_user = msg.role == "user";
                    let (sources_sig, _) = signal(msg.sources.clone());
                    
                    view! {
                        <div class=if is_user { "flex justify-end" } else { "flex justify-start" }>
                            <div class="max-w-[85%]">
                                <div class=if is_user {
                                    "bg-purple-600 text-white rounded-2xl rounded-tr-sm px-5 py-3.5 shadow-sm"
                                } else {
                                    "bg-gray-100 text-gray-800 rounded-2xl rounded-tl-sm px-5 py-3.5"
                                }>
                                    <p class="text-sm leading-relaxed whitespace-pre-wrap">{msg.content}</p>
                                </div>
                                
                                <Show when=move || sources_sig.get().is_some()>
                                    <div class="mt-2 ml-2">
                                        <p class="text-[10px] font-semibold text-gray-400 uppercase tracking-wider mb-1">"Sources"</p>
                                        <div class="flex flex-wrap gap-2">
                                            <For
                                                each=move || sources_sig.get().unwrap()
                                                key=|src| src.document_id
                                                children=move |src| {
                                                    view! {
                                                        <div class="flex items-center gap-1 text-[11px] text-blue-600 bg-blue-50 px-2 py-1 rounded border border-blue-100 hover:bg-blue-100 cursor-pointer transition-colors">
                                                            <svg class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 011.414.586l5.414 5.414a1 1 0 01.586 1.414V19a2 2 0 01-2 2z" />
                                                            </svg>
                                                            <span class="truncate max-w-[150px]">{src.title}</span>
                                                        </div>
                                                    }
                                                }
                                            />
                                        </div>
                                    </div>
                                </Show>
                            </div>
                        </div>
                    }
                }
            />

            <Show when=move || loading.get()>
                <div class="flex justify-start">
                    <div class="bg-gray-100 rounded-2xl rounded-tl-sm px-5 py-3.5 flex items-center gap-2">
                        <div class="flex space-x-1">
                            <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style="animation-delay: 0ms"></div>
                            <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style="animation-delay: 150ms"></div>
                            <div class="w-2 h-2 bg-gray-400 rounded-full animate-bounce" style="animation-delay: 300ms"></div>
                        </div>
                    </div>
                </div>
            </Show>
        </div>

        // Input Area
        <div class="p-4 bg-white border-t border-gray-200">
            <div class="relative max-w-4xl mx-auto">
                <textarea
                    prop:value=input
                    on:input=move |ev| set_input.set(event_target_value(&ev))
                    on:keydown=move |ev: web_sys::KeyboardEvent| {
                        if ev.key() == "Enter" && !ev.shift_key() {
                            ev.prevent_default();
                            if !input.get().trim().is_empty() {
                                on_send.run((input.get(), false));
                                set_input.set(String::new());
                            }
                        }
                    }
                    placeholder="Ask a question about the selected documents..."
                    rows="1"
                    class="w-full pl-4 pr-24 py-3 bg-gray-50 border border-gray-200 rounded-xl text-sm focus:ring-2 focus:ring-purple-500 focus:border-purple-500 resize-none shadow-sm"
                    style="min-height: 48px; max-height: 120px;"
                ></textarea>
                
                <div class="absolute right-2 bottom-2 flex gap-1">
                    <button on:click=move |_| {
                                if !input.get().trim().is_empty() {
                                    on_send.run((input.get(), true));
                                    set_input.set(String::new());
                                }
                            }
                            disabled=move || input.get().trim().is_empty() || loading.get()
                            class="p-2 text-gray-400 hover:text-blue-600 hover:bg-blue-50 rounded-lg disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                            title="Search & Ask (Updates results)">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
                        </svg>
                    </button>
                    <button on:click=move |_| {
                                if !input.get().trim().is_empty() {
                                    on_send.run((input.get(), false));
                                    set_input.set(String::new());
                                }
                            }
                            disabled=move || input.get().trim().is_empty() || loading.get()
                            class="p-2 bg-purple-600 text-white rounded-lg hover:bg-purple-700 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
                            title="Ask (Keep current results)">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 19l9 2-9-18-9 18 9-2zm0 0v-8" />
                        </svg>
                    </button>
                </div>
            </div>
            <p class="text-center text-[10px] text-gray-400 mt-2">
                "AI can make mistakes. Verify important information."
            </p>
        </div>
    }
}
