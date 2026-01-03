use leptos::*;
use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::*;
use leptos_router::components::{Router, Routes, Route};
use crate::web_app::pages::search::SearchPage;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Title text="RAG Chat - Document Search & Chat" />
        <Meta name="description" content="AI-enhanced document search with hybrid BM25 and vector similarity" />
        <Meta name="viewport" content="width=device-width, initial-scale=1" />
        
        // Tailwind CSS (via CDN for simplicity in this refactor, or local if set up)
        // In a real production app, you'd bundle this. For now, we keep the CDN link 
        // to ensure it works like the original static HTML.
        <Script src="https://cdn.tailwindcss.com" />
        
        // If using trunk/cargo-leptos, this would be the compiled CSS
        // <Stylesheet id="leptos" href="/pkg/rag-chat.css" />

        <Router>
            <main class="h-screen w-screen overflow-hidden bg-gray-100 text-gray-800">
                <Routes fallback=|| view! { <div class="p-4">Not Found</div> }>
                    <Route path=path!("/") view=SearchPage />
                </Routes>
            </main>
        </Router>
    }
}
