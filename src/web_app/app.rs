use leptos::prelude::*;
use leptos_meta::*;
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};
use crate::web_app::pages::search::SearchPage;

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();

    view! {
        <Stylesheet id="tailwind" href="/tailwind.css" />

        <Router>
            <main class="h-screen w-screen overflow-hidden bg-gray-100 text-gray-800">
                <Routes fallback=|| "Not Found">
                    <Route path=StaticSegment("") view=SearchPage />
                </Routes>
            </main>
        </Router>
    }
}
