use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DbStats {
    pub documents: i64,
    pub chunks: i64,
}

#[component]
pub fn StatsBar() -> impl IntoView {
    let (stats, set_stats) = signal(Option::<DbStats>::None);
    let (is_loading, set_is_loading) = signal(true);

    // Fetch stats on component mount
    #[cfg(target_arch = "wasm32")]
    Effect::new(move |_| {
        leptos::logging::log!("StatsBar: Effect triggered, fetching DB stats");
        leptos::task::spawn_local(async move {
            leptos::logging::log!("StatsBar: Async task started");
            if let Some(db_stats) = fetch_db_stats().await {
                leptos::logging::log!("StatsBar: Got stats, updating signal");
                set_stats.set(Some(db_stats));
                set_is_loading.set(false);
            } else {
                leptos::logging::warn!("StatsBar: fetch_db_stats returned None");
                // Set default values on error
                set_stats.set(Some(DbStats {
                    documents: 0,
                    chunks: 0,
                }));
                set_is_loading.set(false);
            }
        });
    });

    // Suppress unused warning for SSR
    #[cfg(not(target_arch = "wasm32"))]
    let _ = (set_stats, set_is_loading);

    view! {
        <div class="bg-white border-b border-gray-200 px-6 py-2 flex gap-6 text-xs text-gray-600">
            <Show
                when=move || !is_loading.get()
                fallback=move || view! {
                    <div class="flex items-center gap-2">
                        <svg class="h-4 w-4 text-blue-500" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M3 4a1 1 0 011-1h12a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1V4z" />
                            <path fill-rule="evenodd" d="M3 10a1 1 0 011-1h12a1 1 0 011 1v6a1 1 0 01-1 1H4a1 1 0 01-1-1v-6zm5-3a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
                        </svg>
                        <span class="h-4 w-20 bg-gray-200 rounded animate-pulse" />
                    </div>
                }
            >
                <div class="flex items-center gap-2">
                    <svg class="h-4 w-4 text-blue-500" fill="currentColor" viewBox="0 0 20 20">
                        <path d="M3 4a1 1 0 011-1h12a1 1 0 011 1v2a1 1 0 01-1 1H4a1 1 0 01-1-1V4z" />
                        <path fill-rule="evenodd" d="M3 10a1 1 0 011-1h12a1 1 0 011 1v6a1 1 0 01-1 1H4a1 1 0 01-1-1v-6zm5-3a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
                    </svg>
                    <span>
                        {move || {
                            stats.get().map(|s| {
                                if s.documents == 1 {
                                    "1 document".to_string()
                                } else {
                                    format!("{} documents", s.documents)
                                }
                            }).unwrap_or_else(|| "0 documents".to_string())
                        }}
                    </span>
                </div>
            </Show>

            <Show
                when=move || !is_loading.get()
                fallback=move || view! {
                    <div class="flex items-center gap-2">
                        <svg class="h-4 w-4 text-green-500" fill="currentColor" viewBox="0 0 20 20">
                            <path d="M2 11a1 1 0 011-1h2a1 1 0 011 1v5a1 1 0 01-1 1H3a1 1 0 01-1-1v-5zM8 7a1 1 0 011-1h2a1 1 0 011 1v9a1 1 0 01-1 1H9a1 1 0 01-1-1V7zM14 4a1 1 0 011-1h2a1 1 0 011 1v12a1 1 0 01-1 1h-2a1 1 0 01-1-1V4z" />
                        </svg>
                        <span class="h-4 w-20 bg-gray-200 rounded animate-pulse" />
                    </div>
                }
            >
                <div class="flex items-center gap-2">
                    <svg class="h-4 w-4 text-green-500" fill="currentColor" viewBox="0 0 20 20">
                        <path d="M2 11a1 1 0 011-1h2a1 1 0 011 1v5a1 1 0 01-1 1H3a1 1 0 01-1-1v-5zM8 7a1 1 0 011-1h2a1 1 0 011 1v9a1 1 0 01-1 1H9a1 1 0 01-1-1V7zM14 4a1 1 0 011-1h2a1 1 0 011 1v12a1 1 0 01-1 1h-2a1 1 0 01-1-1V4z" />
                    </svg>
                    <span>
                        {move || {
                            stats.get().map(|s| {
                                if s.chunks == 1 {
                                    "1 chunk".to_string()
                                } else {
                                    format!("{} chunks", s.chunks)
                                }
                            }).unwrap_or_else(|| "0 chunks".to_string())
                        }}
                    </span>
                </div>
            </Show>

            <div class="flex items-center gap-2 ml-auto text-gray-400">
                <span>"BM25 + Vector Search"</span>
            </div>
        </div>
    }
}

#[cfg(target_arch = "wasm32")]
#[allow(dead_code)]
async fn fetch_db_stats() -> Option<DbStats> {
    use web_sys::window;

    leptos::logging::log!("StatsBar: fetch_db_stats called");

    let window = window()?;
    let origin = window.location().origin().ok()?;
    let url = format!("{}/api/stats/db", origin);

    leptos::logging::log!("StatsBar: Fetching from URL: {}", url);

    let response = gloo_net::http::Request::get(&url)
        .send()
        .await
        .map_err(|e| {
            leptos::logging::error!("StatsBar: Fetch error: {}", e);
        })
        .ok()?;

    leptos::logging::log!("StatsBar: Response status: {}", response.status());

    let json_result = response.json::<DbStats>().await.map_err(|e| {
        leptos::logging::error!("StatsBar: JSON parsing error: {}", e);
    });

    match json_result {
        Ok(stats) => {
            leptos::logging::log!(
                "StatsBar: Successfully fetched stats - docs: {}, chunks: {}",
                stats.documents,
                stats.chunks
            );
            Some(stats)
        }
        Err(_) => None,
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
async fn fetch_db_stats() -> Option<DbStats> {
    // SSR fallback - return None to trigger loading state
    None
}
