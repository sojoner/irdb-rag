use anyhow::Result;
use rag_chat::config::Settings;
use rag_chat::infra::embedder::Embedder;
use rag_chat::services::indexing::index_path_with_config;
use sqlx::postgres::PgPoolOptions;
use std::path::Path;
use std::time::Instant;

#[tokio::test]
async fn test_manual_indexing_performance() -> Result<()> {
    // Load settings
    let settings = Settings::new()?;

    // Connect to database
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&settings.database.url)
        .await?;

    // Initialize embedder
    let embedder = Embedder::new(&settings.embedding)?;

    // Try different possible paths for the file
    let possible_paths = [
        "/data/books/Wellbeing/Women@PlayStation Inclusive Language Guide - The Hub.pdf",
        "/app/books/Wellbeing/Women@PlayStation Inclusive Language Guide - The Hub.pdf",
        "books/Wellbeing/Women@PlayStation Inclusive Language Guide - The Hub.pdf",
    ];

    let mut test_file = None;
    for path in possible_paths {
        if Path::new(path).exists() {
            test_file = Some(path);
            break;
        }
    }

    let test_file =
        test_file.expect("Could not find the test file in any of the expected locations");

    println!("🚀 Starting manual indexing test for: {}", test_file);
    let start = Instant::now();

    // Run indexing with settings to ensure correct docling URL
    let ids = index_path_with_config(&pool, &embedder, test_file, Some(&settings)).await?;

    let duration = start.elapsed();
    println!("✅ Indexing completed in {:.2}s", duration.as_secs_f64());
    println!("📝 Indexed document IDs: {:?}", ids);

    Ok(())
}
