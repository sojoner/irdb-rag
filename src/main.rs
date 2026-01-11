//! RAG Chat - Hybrid Search RAG System
//!
//! A Rust-based RAG system using ParadeDB (BM25 + pgvector) for hybrid search,
//! Leptos for the UI, and FastEmbed for local embeddings.

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use leptos::prelude::get_configuration;
use std::sync::Arc;

use rag_chat::config::Settings;
use rag_chat::api::{self, state::AppState};
use rag_chat::infra::{db, embedder::Embedder, reranker::Reranker};
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
    /// Import Chrome bookmarks
    ImportBookmarks {
        #[arg(short, long)]
        path: String,
    },
    /// Run knowledge base scan manually
    Scan {
        #[arg(short, long)]
        paths: Vec<String>,
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

    // Load configuration
    let settings = Settings::new()
        .map_err(|e| anyhow::anyhow!("Failed to load configuration: {}", e))?;

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { port: cli_port }) => {
            // CLI port overrides config if provided
            let port = if cli_port != 3000 { cli_port } else { settings.server.port };
            serve(port, log_buffer, settings).await?;
        }
        Some(Commands::Index { path, url }) => {
            let pool = db::create_pool(&settings.database).await?;
            let embedder = Embedder::new(&settings.embedding)?;
            embedder.init().await?;

            if let Some(path) = path {
                indexing::index_path_with_config(&pool, &embedder, &path, Some(&settings)).await?;
            }
            if let Some(url) = url {
                indexing::index_url(&pool, &embedder, &url).await?;
            }
        }
        Some(Commands::Watch { folders }) => {
            let pool = db::create_pool(&settings.database).await?;
            let embedder = Embedder::new(&settings.embedding)?;
            embedder.init().await?;
            indexing::watch_folders(&pool, &embedder, folders).await?;
        }
        Some(Commands::ImportBookmarks { path }) => {
            let pool = db::create_pool(&settings.database).await?;

            use rag_chat::services::bookmark_parser;
            use rag_chat::services::import::ImportJobRunner;

            tracing::info!("Importing Chrome bookmarks from: {}", path);

            let urls = bookmark_parser::parse_chrome_bookmarks(&path)?;
            tracing::info!("Parsed {} URLs from bookmarks", urls.len());

            if urls.is_empty() {
                println!("No URLs found in bookmarks file");
                return Ok(());
            }

            // Create import job
            let runner = ImportJobRunner::new(settings.import.clone());
            let job_id = runner
                .create_job(&pool, "chrome_bookmarks", Some(&path))
                .await?;

            // Create import items
            let url_refs: Vec<&str> = urls.iter().map(|u| u.as_str()).collect();
            rag_chat::services::import::ImportItemManager
                .create_items(&pool, job_id, url_refs)
                .await?;

            tracing::info!("Created import job {} with {} bookmarks", job_id, urls.len());
            println!("Import job created: {}", job_id);
            println!("Queued {} bookmarks for import", urls.len());
        }
        Some(Commands::Scan { paths }) => {
            let pool = db::create_pool(&settings.database).await?;
            let embedder = Embedder::new(&settings.embedding)?;
            embedder.init().await?;

            // Spawn import workers (minimal setup for CLI)
            let import_queue = rag_chat::services::import::spawn_import_workers(
                pool.clone(),
                std::sync::Arc::new(embedder),
                settings.import.workers,
            );

            // Create config with provided paths
            let mut kb_config = settings.knowledge_base.clone();
            kb_config.local_paths = paths;

            let scanner = rag_chat::services::startup_scan::StartupScanner::new(
                pool.clone(),
                kb_config,
                import_queue,
            );

            scanner.run().await?;
            println!("Scan completed");
        }
        None => {
            // Default: serve on configured port
            serve(settings.server.port, log_buffer, settings).await?;
        }
    }

    Ok(())
}

async fn serve(port: u16, log_buffer: std::sync::Arc<std::sync::Mutex<Vec<String>>>, settings: Settings) -> Result<()> {
    tracing::info!("Starting RAG Chat server on port {}", port);

    // Load Leptos configuration
    let conf = get_configuration(Some("Cargo.toml"))
        .map_err(|e| anyhow::anyhow!("Failed to load Leptos configuration: {}", e))?;
    let leptos_options = conf.leptos_options;
    let addr = leptos_options.site_addr;

    let pool = db::create_pool(&settings.database).await?;
    let embedder = Embedder::new(&settings.embedding)?;
    embedder.init().await?;

    // Initialize reranker if enabled
    let reranker = if settings.reranking.enabled {
        match Reranker::new(&settings.reranking) {
            Ok(r) => {
                if let Err(e) = r.init().await {
                    tracing::warn!("Reranker init failed, disabling: {}", e);
                    None
                } else {
                    tracing::info!("Reranker initialized: model={}", r.get_model_name());
                    Some(Arc::new(r))
                }
            }
            Err(e) => {
                tracing::warn!("Failed to create reranker, disabling: {}", e);
                None
            }
        }
    } else {
        None
    };

    // Spawn import job workers
    let import_job_queue = rag_chat::services::import::spawn_import_workers(
        pool.clone(),
        std::sync::Arc::new(embedder.clone()),
        settings.import.workers,
    );
    tracing::info!("Spawned {} import job workers", settings.import.workers);

    // Spawn job cleanup background task
    rag_chat::services::job_cleanup::spawn_cleanup_task(
        pool.clone(),
        &settings.import.cleanup,
    );

    // Recover stuck jobs (jobs that were interrupted during previous run)
    {
        let pool_clone = pool.clone();
        let queue_clone = import_job_queue.clone();
        tokio::spawn(async move {
            if let Err(e) = recover_stuck_jobs(&pool_clone, &queue_clone).await {
                tracing::error!("Failed to recover stuck jobs: {}", e);
            }
        });
    }

    let state = AppState::new(
        pool,
        embedder,
        log_buffer,
        leptos_options.clone(),
        import_job_queue,
        Arc::new(settings),
        reranker,
    );

    // Create the Axum router with API routes and Leptos integration
    let app = api::routes::create_router(state);

    tracing::info!("Listening on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

/// Recover jobs that were stuck in 'pending' or 'running' state from previous runs
async fn recover_stuck_jobs(
    pool: &sqlx::PgPool,
    import_queue: &tokio::sync::mpsc::Sender<uuid::Uuid>,
) -> Result<()> {
    use uuid::Uuid;

    tracing::info!("Checking for stuck import jobs to recover...");

    // Find jobs that are in pending or running status with pending items
    let stuck_jobs: Vec<(Uuid,)> = sqlx::query_as(
        r#"
        SELECT ij.id
        FROM import_jobs ij
        WHERE ij.status IN ('pending', 'running')
        AND EXISTS (
            SELECT 1 FROM import_items ii
            WHERE ii.job_id = ij.id
            AND ii.status = 'pending'
        )
        ORDER BY ij.created_at ASC
        "#
    )
    .fetch_all(pool)
    .await?;

    if stuck_jobs.is_empty() {
        tracing::info!("No stuck jobs found");
        return Ok(());
    }

    tracing::info!("Found {} stuck jobs to recover", stuck_jobs.len());

    for (job_id,) in stuck_jobs {
        // Reset job status to pending
        sqlx::query("UPDATE import_jobs SET status = 'pending' WHERE id = $1")
            .bind(job_id)
            .execute(pool)
            .await?;

        // Re-queue the job
        import_queue.send(job_id).await.map_err(|e| {
            anyhow::anyhow!("Failed to re-queue stuck job {}: {}", job_id, e)
        })?;

        tracing::info!("Recovered and re-queued stuck job: {}", job_id);
    }

    Ok(())
}
