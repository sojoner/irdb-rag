/// Database pool connection tests
///
/// Tests to validate database connection pool behavior and identify timeout issues

use rag_chat::config::Settings;

#[tokio::test]
async fn test_basic_db_connection() {
    if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }

    let settings = Settings::new().expect("Failed to load settings");
    let db_url = settings.database.url.clone();

    println!("Connecting to database: {}", db_url);

    // Create pool with reasonable settings
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    // Simple query to verify connection works
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("Failed to query documents table");

    println!("✓ Database connected successfully");
    println!("  Document count: {}", result.0);

    pool.close().await;
}

#[tokio::test]
async fn test_concurrent_db_queries() {
    if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }

    let settings = Settings::new().expect("Failed to load settings");
    let db_url = settings.database.url.clone();

    // Create pool with limited connections to test pool behavior
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(3)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    println!("Testing concurrent queries with 3 max connections...");

    // Run 5 concurrent queries (more than pool size)
    let mut handles = vec![];

    for i in 0..5 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            println!("  Query {} starting...", i);
            let start = std::time::Instant::now();

            let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
                .fetch_one(&pool_clone)
                .await
                .expect("Query failed");

            let elapsed = start.elapsed();
            println!("  Query {} completed in {:?} (count: {})", i, elapsed, result.0);
        });
        handles.push(handle);
    }

    // Wait for all queries to complete
    for (i, handle) in handles.into_iter().enumerate() {
        handle.await.unwrap_or_else(|_| panic!("Query {} panicked", i));
    }

    println!("✓ All concurrent queries completed successfully");

    pool.close().await;
}

#[tokio::test]
async fn test_db_pool_under_load() {
    if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }

    let settings = Settings::new().expect("Failed to load settings");
    let db_url = settings.database.url.clone();

    // Create pool with same settings as main application
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .idle_timeout(std::time::Duration::from_secs(300))
        .max_lifetime(std::time::Duration::from_secs(1800))
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    println!("Testing pool under load (10 connections)...");
    println!("Pool stats - Max connections: 10");

    // Simulate heavy load with 20 concurrent operations
    let mut handles = vec![];

    for i in 0..20 {
        let pool_clone = pool.clone();
        let handle = tokio::spawn(async move {
            let start = std::time::Instant::now();

            // Simulate a more complex query with a small sleep
            let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
                .fetch_one(&pool_clone)
                .await
                .expect("Query failed");

            // Small delay to simulate processing
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;

            let elapsed = start.elapsed();
            if elapsed.as_secs() > 5 {
                eprintln!("⚠ Query {} took longer than expected: {:?}", i, elapsed);
            }

            result.0
        });
        handles.push(handle);
    }

    // Wait for all to complete
    let mut results = vec![];
    for (i, handle) in handles.into_iter().enumerate() {
        match handle.await {
            Ok(count) => results.push(count),
            Err(e) => panic!("Query {} failed: {:?}", i, e),
        }
    }

    println!("✓ Completed {} queries successfully", results.len());

    pool.close().await;
}

#[tokio::test]
async fn test_db_connection_with_slow_query() {
    if std::env::var("RUN_ENV").is_err() { if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); } }

    let settings = Settings::new().expect("Failed to load settings");
    let db_url = settings.database.url.clone();

    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&db_url)
        .await
        .expect("Failed to connect to database");

    println!("Testing slow query handling...");

    // Run a deliberately slow query
    let start = std::time::Instant::now();
    sqlx::query("SELECT pg_sleep(1)")
        .execute(&pool)
        .await
        .expect("Slow query failed");

    let elapsed = start.elapsed();
    println!("✓ Slow query completed in {:?}", elapsed);

    // Verify pool is still functional
    let result: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM documents")
        .fetch_one(&pool)
        .await
        .expect("Failed to query after slow query");

    println!("✓ Pool still functional (count: {})", result.0);

    pool.close().await;
}
