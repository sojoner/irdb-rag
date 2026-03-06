use axum::{
    routing::{delete, get, options, post},
    Router,
};
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::api::{handlers, state::AppState};
use crate::shell;
use crate::web_app::app::App;

// Handler for CORS preflight OPTIONS requests
async fn handle_options() -> &'static str {
    ""
}

pub fn create_router(state: AppState) -> Router {
    let leptos_options = state.leptos_options.clone();
    let routes = generate_route_list(App);

    Router::new()
        // CORS preflight handlers for API routes
        .route("/api/search", options(handle_options))
        .route("/api/search/bm25", options(handle_options))
        .route("/api/search/vector", options(handle_options))
        .route("/api/search/faceted", options(handle_options))
        .route("/api/facets/values", options(handle_options))
        .route("/api/chat", options(handle_options))
        .route("/api/chat/stream", options(handle_options))
        .route("/api/chat/conversation", options(handle_options))
        .route("/api/chat/conversation/stream", options(handle_options))
        .route("/api/conversations", options(handle_options))
        .route("/api/conversations/{id}", options(handle_options))
        .route("/api/documents", options(handle_options))
        .route("/api/documents/{id}", options(handle_options))
        .route("/api/documents/{id}/assets", options(handle_options))
        .route("/api/documents/{id}/markdown", options(handle_options))
        .route("/api/documents/batch", options(handle_options))
        .route("/api/categories", options(handle_options))
        .route("/api/aggregation-stats", options(handle_options))
        .route("/api/stats/db", options(handle_options))
        .route("/api/index", options(handle_options))
        .route("/api/health", options(handle_options))
        .route("/api/status", options(handle_options))
        .route("/api/config/model", options(handle_options))
        .route("/api/logs", options(handle_options))
        .route("/api/import", options(handle_options))
        .route("/api/import/{id}", options(handle_options))
        .route("/api/import/{id}/items", options(handle_options))
        .route("/api/import/{id}/resume", options(handle_options))
        .route("/api/knowledge-base/paths", options(handle_options))
        .route("/api/knowledge-base/bookmarks", options(handle_options))
        .route("/api/knowledge-base/scan", options(handle_options))
        // API routes
        .route("/api/search", post(handlers::search))
        .route("/api/search/bm25", post(handlers::search_bm25))
        .route("/api/search/vector", post(handlers::search_vector))
        .route("/api/search/faceted", post(handlers::faceted_search))
        .route("/api/facets/values", post(handlers::get_facet_values))
        .route("/api/chat", post(handlers::chat))
        .route("/api/chat/stream", post(handlers::chat_stream))
        .route("/api/chat/conversation", post(handlers::chat_conversation))
        .route(
            "/api/chat/conversation/stream",
            post(handlers::chat_conversation_stream),
        )
        // Conversation management routes
        .route("/api/conversations", get(handlers::list_conversations))
        .route("/api/conversations", post(handlers::create_conversation))
        .route("/api/conversations/{id}", get(handlers::get_conversation))
        .route("/api/conversations/{id}", delete(handlers::delete_conversation))
        .route("/api/documents", get(handlers::list_documents))
        .route("/api/documents/{id}", get(handlers::get_document))
        .route("/api/documents/{id}", delete(handlers::delete_document))
        .route(
            "/api/documents/{id}/assets",
            get(handlers::get_document_assets),
        )
        .route(
            "/api/documents/{id}/markdown",
            get(handlers::export_markdown),
        )
        .route(
            "/api/documents/batch",
            delete(handlers::delete_documents_batch),
        )
        .route("/api/categories", get(handlers::list_categories))
        .route(
            "/api/aggregation-stats",
            get(handlers::get_aggregation_stats),
        )
        .route("/api/stats/db", get(handlers::get_db_stats))
        .route("/api/index", post(handlers::index_document))
        .route("/api/health", get(handlers::health_check))
        .route("/api/status", get(handlers::get_status))
        .route("/api/config/model", post(handlers::update_model))
        .route("/api/logs", get(handlers::get_logs))
        // Import routes
        .route("/api/import", post(handlers::create_import))
        .route("/api/import", get(handlers::list_imports))
        .route("/api/import/{id}", get(handlers::get_import_status))
        .route("/api/import/{id}", delete(handlers::delete_import))
        .route("/api/import/{id}/items", get(handlers::get_import_items))
        .route("/api/import/{id}/resume", post(handlers::resume_import))
        // Knowledge base routes
        .route(
            "/api/knowledge-base/paths",
            post(handlers::add_knowledge_base_paths),
        )
        .route(
            "/api/knowledge-base/bookmarks",
            post(handlers::import_chrome_bookmarks),
        )
        .route("/api/knowledge-base/scan", post(handlers::trigger_scan))
        // Handle server functions
        .route(
            "/api/{*fn_name}",
            post({
                let state = state.clone();
                move |req| {
                    let state = state.clone();
                    async move {
                        leptos_axum::handle_server_fns_with_context(
                            move || {
                                leptos::prelude::provide_context(state.clone());
                            },
                            req,
                        )
                        .await
                    }
                }
            }),
        )
        // Serve static files from the "site/pkg" directory (WASM/JS)
        .nest_service("/pkg", ServeDir::new("site/pkg"))
        // Serve CSS and other assets from site root
        .route_service(
            "/tailwind.css",
            tower_http::services::ServeFile::new("site/tailwind.css"),
        )
        // Serve other static assets
        .nest_service("/assets", ServeDir::new("assets"))
        // Leptos routes with shell closure
        .leptos_routes_with_context(
            &state,
            routes,
            {
                let state = state.clone();
                move || {
                    use leptos::prelude::provide_context;
                    provide_context(state.clone());
                }
            },
            {
                let leptos_options = leptos_options.clone();
                move || shell(leptos_options.clone())
            },
        )
        .layer(CorsLayer::permissive())
        .with_state(state)
}
