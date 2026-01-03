//! RAG Chat - Hybrid Search RAG System
//! 
//! A Rust-based RAG system using ParadeDB (BM25 + pgvector) for hybrid search,
//! Leptos for the UI, and FastEmbed for local embeddings.

mod db;
mod indexer;
mod api;
mod llm;
mod logging;

use anyhow::Result;
use axum::{
    routing::{get, post},
    Router,
};
use clap::{Parser, Subcommand};
use std::net::SocketAddr;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser)]
#[command(name = "rag-chat")]
#[command(about = "RAG Chat - Hybrid Search Document Chat System")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the web server
    Serve {
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Index documents
    Index {
        #[arg(short, long)]
        path: Option<String>,
        #[arg(short, long)]
        url: Option<String>,
    },
    /// Watch folders for changes and auto-index
    Watch {
        #[arg(short, long)]
        folders: Vec<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    // Create log buffer
    let log_buffer = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let buffer_layer = logging::BufferLayer::new(log_buffer.clone(), 100);

    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(buffer_layer)
        .init();

    // Load environment
    dotenvy::dotenv().ok();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { port }) => {
            serve(port, log_buffer).await?;
        }
        Some(Commands::Index { path, url }) => {
            let pool = db::create_pool().await?;
            let embedder = indexer::Embedder::new()?;
            embedder.init().await?;
            
            if let Some(path) = path {
                indexer::index_path(&pool, &embedder, &path).await?;
            }
            if let Some(url) = url {
                indexer::index_url(&pool, &embedder, &url).await?;
            }
        }
        Some(Commands::Watch { folders }) => {
            let pool = db::create_pool().await?;
            let embedder = indexer::Embedder::new()?;
            embedder.init().await?;
            indexer::watch_folders(&pool, &embedder, folders).await?;
        }
        None => {
            // Default: serve on port 3000
            serve(3000, log_buffer).await?;
        }
    }

    Ok(())
}

async fn serve(port: u16, log_buffer: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Result<()> {
    tracing::info!("Starting RAG Chat server on port {}", port);

    let pool = db::create_pool().await?;
    let embedder = indexer::Embedder::new()?;
    embedder.init().await?;
    
    let state = api::AppState::new(pool, embedder, log_buffer);

    let app = Router::new()
        // API routes
        .route("/api/search", post(api::search))
        .route("/api/chat", post(api::chat))
        .route("/api/documents", get(api::list_documents))
        .route("/api/documents/{id}", get(api::get_document))
        .route("/api/documents/{id}/assets", get(api::get_document_assets))
        .route("/api/documents/{id}/markdown", get(api::export_markdown))
        .route("/api/categories", get(api::list_categories))
        .route("/api/index", post(api::index_document))
        .route("/api/health", get(api::health_check))
        .route("/api/status", get(api::get_status))
        .route("/api/config/model", post(api::update_model))
        .route("/api/logs", get(api::get_logs))
        // Static files and UI
        .fallback(api::serve_ui)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Listening on http://{}", addr);
    
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
