use axum::{
    routing::{delete, get, post},
    Router,
};
use leptos_axum::{generate_route_list, LeptosRoutes};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::api::{handlers, state::AppState};
use crate::shell;
use crate::web_app::app::App;

pub fn create_router(state: AppState) -> Router {
    let leptos_options = state.leptos_options.clone();
    let routes = generate_route_list(App);

    Router::new()
        // API routes
        .route("/api/search", post(handlers::search))
        .route("/api/search/faceted", post(handlers::faceted_search))
        .route("/api/facets/values", post(handlers::get_facet_values))
        .route("/api/chat", post(handlers::chat))
        .route("/api/chat/stream", post(handlers::chat_stream))
        .route("/api/chat/conversation", post(handlers::chat_conversation))
        .route(
            "/api/chat/conversation/stream",
            post(handlers::chat_conversation_stream),
        )
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
        // Serve static files from the "target/site/pkg" directory (WASM/JS)
        .nest_service("/pkg", ServeDir::new("target/site/pkg"))
        // Serve CSS and other assets from target/site root
        .route_service(
            "/tailwind.css",
            tower_http::services::ServeFile::new("target/site/tailwind.css"),
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
