//! RAG Chat Library
//!
//! Library modules for the RAG Chat application

pub mod db;
pub mod indexer;
pub mod enricher;
pub mod api;
pub mod llm;
pub mod types;
pub mod web_app;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use leptos::*;
    console_error_panic_hook::set_once();
    mount_to_body(web_app::app::App);
}
