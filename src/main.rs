//! RAG Chat - Hybrid Search RAG System
//! 
//! A Rust-based RAG system using ParadeDB (BM25 + pgvector) for hybrid search,
//! Leptos for the UI, and FastEmbed for local embeddings.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use leptos::prelude::get_configuration;

use rag_chat::api::{self, state::AppState};
use rag_chat::infra::{db, embedder::Embedder};
use rag_chat::services::indexing;
use rag_chat::logging;

// SSR features not currently used - serving static files instead

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
            let embedder = Embedder::new()?;
            embedder.init().await?;
            
            if let Some(path) = path {
                indexing::index_path(&pool, &embedder, &path).await?;
            }
            if let Some(url) = url {
                indexing::index_url(&pool, &embedder, &url).await?;
            }
        }
        Some(Commands::Watch { folders }) => {
            let pool = db::create_pool().await?;
            let embedder = Embedder::new()?;
            embedder.init().await?;
            indexing::watch_folders(&pool, &embedder, folders).await?;
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

    // Load Leptos configuration
    let conf = get_configuration(Some("Cargo.toml"))
        .map_err(|e| anyhow::anyhow!("Failed to load configuration: {}", e))?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;

    let pool = db::create_pool().await?;
    let embedder = Embedder::new()?;
    embedder.init().await?;

    let state = AppState::new(pool, embedder, log_buffer, leptos_options.clone());

    // Create the Axum router with API routes and Leptos integration
    let app = api::routes::create_router(state);

    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
