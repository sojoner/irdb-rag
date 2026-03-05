//! RAG Chat - Hybrid Search RAG System
//!
//! A Rust-based RAG system using ParadeDB (BM25 + pgvector) for hybrid search,
//! Leptos for the UI, and FastEmbed for local embeddings.

use anyhow::Result;
use clap::{Parser, Subcommand};
use leptos::prelude::get_configuration;
use std::sync::Arc;
use std::time::Duration;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use rag_chat::api::{self, state::AppState};
use rag_chat::config::Settings;
use rag_chat::infra::{db, embedder::Embedder, reranker::Reranker};
use rag_chat::logging;
use rag_chat::services::indexing;

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
    /// Import Wikipedia multistream dump
    ImportWikipedia {
        #[arg(short, long)]
        path: String,
    },
    /// Run knowledge base scan manually
    Scan {
        #[arg(short, long)]
        paths: Vec<String>,
    },
    /// Process pending embeddings for documents indexed in BM25-only mode
    ProcessEmbeddings {
        #[arg(short, long, default_value = "100")]
        limit: usize,
        #[arg(short, long, default_value = "64")]
        batch_size: usize,
    },
    /// Enrich unenriched documents (summary, keywords, entities, embeddings)
    EnrichDocuments {
        #[arg(short, long, default_value = "1000")]
        limit: usize,
        #[arg(short, long, default_value = "10")]
        batch_size: usize,
    },
    /// Batch skip all unenriched documents to prevent enrichment attempt
    SkipUnenriched,
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
    let settings =
        Settings::new().map_err(|e| anyhow::anyhow!("Failed to load configuration: {}", e))?;

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Serve { port: cli_port }) => {
            // CLI port overrides config if provided
            let port = if cli_port != 3000 {
                cli_port
            } else {
                settings.server.port
            };
            serve(port, log_buffer, settings).await?;
        }
        Some(Commands::Index { path, url }) => {
            let pool = db::create_pool(&settings.database).await?;
            let embedder = Embedder::new(&settings.embedding)?;
            embedder.init().await?;

            if let Some(path) = path {
                indexing::index_path_with_config(&pool, &embedder, &path, Some(&settings), false).await?;
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

            tracing::info!(
                "Created import job {} with {} bookmarks",
                job_id,
                urls.len()
            );
            println!("Import job created: {}", job_id);
            println!("Queued {} bookmarks for import", urls.len());
        }
        Some(Commands::ImportWikipedia { path }) => {
            let pool = db::create_pool(&settings.database).await?;

            use rag_chat::services::import_wiki;
            use rag_chat::services::import::ImportJobRunner;
            use std::path::PathBuf;

            let file_path = PathBuf::from(&path);
            if !file_path.exists() {
                eprintln!("File not found: {}", path);
                return Err(anyhow::anyhow!("Wikipedia dump file not found: {}", path));
            }

            tracing::info!("Starting Wikipedia import from: {}", path);
            println!("Starting Wikipedia import from: {}", path);
            println!("This may take a significant amount of time for large dumps...");

            let runner = ImportJobRunner::new(settings.import.clone());
            let job_id = runner
                .create_job(&pool, "wikipedia_dump", Some(&path))
                .await?;

            println!("Import job created: {}", job_id);
            println!("Processing Wikipedia dump...");

            match import_wiki::import_wikipedia_dump(&pool, job_id, file_path).await {
                Ok(_) => {
                    println!("✓ Wikipedia import completed successfully!");
                    if let Ok(job) = runner.get_job(&pool, job_id).await {
                        println!("  Total processed: {}", job.total_items);
                        println!("  Successfully inserted: {}", job.processed_items);
                        println!("  Failed: {}", job.failed_items);
                    }
                }
                Err(e) => {
                    eprintln!("✗ Wikipedia import failed: {}", e);
                    return Err(e);
                }
            }
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
        Some(Commands::ProcessEmbeddings { limit, batch_size }) => {
            let pool = db::create_pool(&settings.database).await?;
            let embedder = Embedder::new(&settings.embedding)?;
            embedder.init().await?;

            use rag_chat::services::embedding_worker;

            tracing::info!("Starting background embedding processor...");
            match embedding_worker::process_pending_embeddings(&pool, &embedder, limit, batch_size).await {
                Ok(stats) => {
                    println!("Embedding processing complete!");
                    println!("  Documents processed: {}", stats.documents_processed);
                    println!("  Embeddings generated: {}", stats.embeddings_generated);
                    println!("  Failures: {}", stats.failures);
                }
                Err(e) => {
                    eprintln!("Embedding processing failed: {}", e);
                    return Err(e);
                }
            }
        }
        Some(Commands::EnrichDocuments { limit, batch_size }) => {
            let pool = db::create_pool(&settings.database).await?;
            let embedder = Embedder::new(&settings.embedding)?;
            embedder.init().await?;

            use rag_chat::services::enrichment_worker;

            tracing::info!("Starting background enrichment processor...");
            let config = enrichment_worker::EnrichmentWorkerConfig {
                limit,
                batch_size,
                retry_attempts: 3,
                retry_delay_ms: 1000,
            };

            match enrichment_worker::process_unenriched_documents(&pool, &embedder, config).await {
                Ok(stats) => {
                    println!("✓ Enrichment processing complete!");
                    println!("  Documents processed: {}", stats.documents_processed);
                    println!("  Documents enriched: {}", stats.documents_enriched);
                    println!("  Chunks created: {}", stats.chunks_created);
                    println!("  Embeddings generated: {}", stats.embeddings_generated);
                    println!("  Failures: {}", stats.failures);
                }
                Err(e) => {
                    eprintln!("✗ Enrichment processing failed: {}", e);
                    return Err(e);
                }
            }
        }
        Some(Commands::SkipUnenriched) => {
            let pool = db::create_pool(&settings.database).await?;

            println!("🔄 Batch skipping all unenriched documents...");
            println!("   This prevents the enricher from attempting to process them.");
            println!();

            match db::batch_skip_unenriched_documents(&pool).await {
                Ok(skipped_count) => {
                    println!("✓ Batch skip complete!");
                    println!("  Documents marked as 'skipped': {}", skipped_count);
                    println!();
                    println!("You can now safely run the enricher without it attempting");
                    println!("to enrich 6 million Wikipedia pages.");
                }
                Err(e) => {
                    eprintln!("✗ Batch skip failed: {}", e);
                    return Err(e);
                }
            }
        }
        None => {
            serve(settings.server.port, log_buffer, settings).await?;
        }
    }

    Ok(())
}


async fn serve(
    port: u16,
    log_buffer: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    settings: Settings,
) -> Result<()> {
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
    rag_chat::services::job_cleanup::spawn_cleanup_task(pool.clone(), &settings.import.cleanup);

    // Spawn background enrichment worker for unenriched documents
    {
        let pool_clone = pool.clone();
        let embedder_clone = embedder.clone();
        tokio::spawn(async move {
            use rag_chat::services::enrichment_worker;
            let config = enrichment_worker::EnrichmentWorkerConfig {
                limit: 100,
                batch_size: 5,
                retry_attempts: 3,
                retry_delay_ms: 1000,
            };
            
            // Run enrichment in a loop, checking every 5 minutes
            loop {
                match enrichment_worker::process_unenriched_documents(&pool_clone, &embedder_clone, config.clone()).await {
                    Ok(stats) => {
                        if stats.documents_processed > 0 {
                            tracing::info!(
                                "✓ Enrichment batch completed: processed={}, enriched={}, chunks={}, embeddings={}, failures={}",
                                stats.documents_processed,
                                stats.documents_enriched,
                                stats.chunks_created,
                                stats.embeddings_generated,
                                stats.failures
                            );
                        }
                    }
                    Err(e) => {
                        tracing::error!("Enrichment processing error: {}", e);
                    }
                }
                
                // Wait 5 minutes before next enrichment batch
                tokio::time::sleep(Duration::from_secs(300)).await;
            }
        });
    }
    tracing::info!("Spawned background enrichment worker");

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
        "#,
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
        import_queue
            .send(job_id)
            .await
            .map_err(|e| anyhow::anyhow!("Failed to re-queue stuck job {}: {}", job_id, e))?;

        tracing::info!("Recovered and re-queued stuck job: {}", job_id);
    }

    Ok(())
}
