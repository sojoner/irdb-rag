#![recursion_limit = "256"]

//! RAG Chat Library
//!
//! Library modules for the RAG Chat application

#[cfg(feature = "ssr")]
pub mod api;
pub mod config;
pub mod domain;
#[cfg(feature = "ssr")]
pub mod infra;
#[cfg(feature = "ssr")]
pub mod logging;
#[cfg(feature = "ssr")]
pub mod services;
pub mod web_app;

// SSR entry point (server-side)
#[cfg(feature = "ssr")]
pub fn shell(options: leptos::prelude::LeptosOptions) -> impl leptos::IntoView {
    use leptos::prelude::*;
    use leptos_meta::MetaTags;

    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <meta name="description" content="AI-enhanced document search with hybrid BM25 and vector similarity"/>
                <title>"RAG Chat - Document Search & Chat"</title>
                <AutoReload options=options.clone() />
                <HydrationScripts options=options />
                <MetaTags/>
            </head>
            <body>
                <web_app::app::App/>
            </body>
        </html>
    }
}

// Hydration entry point (client-side WASM)
#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();

    leptos::mount::hydrate_body(web_app::app::App);
}
