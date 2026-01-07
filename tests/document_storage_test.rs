use anyhow::Result;
use rag_chat::domain::models::Document;
use rag_chat::infra::db::SearchFilters;
use rag_chat::infra::db::{create_pool, hybrid_search};
use rag_chat::config::Settings;
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn setup_db() -> Result<PgPool> {
    std::env::set_var("RUN_ENV", "test");
    let settings = Settings::new()?;

    // Use the shared pool creation logic which handles config properly
    create_pool(&settings.database).await
}

#[tokio::test]
async fn test_store_and_retrieve_document() -> Result<()> {
    let pool = setup_db().await?;

    println!("\n💾 Testing document storage...\n");

    let doc_id = Uuid::new_v4();
    let title = "Test Document";
    let content = "This is test content for storage validation.";
    let summary = Some("A test document".to_string());
    let keywords = vec!["test".to_string(), "storage".to_string()];
    let entities = serde_json::json!(["TestEntity"]);
    let filepath = Some("test/path.pdf");
    let indexed_at = chrono::Utc::now();

    // Store document
    // Using sqlx::query instead of macro to avoid compile-time DB requirement
    sqlx::query(
        r#"
        INSERT INTO documents (id, title, content, summary, keywords, entities, source_path, source_type, indexed_at, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, 'pdf', $8, $9)
        "#
    )
    .bind(doc_id)
    .bind(title)
    .bind(content)
    .bind(summary.clone())
    .bind(&keywords)
    .bind(&entities)
    .bind(filepath)
    .bind(indexed_at)
    .bind(serde_json::json!({}))
    .execute(&pool)
    .await?;

    println!("✅ Document stored: {}", doc_id);

    // Retrieve document
    let retrieved = sqlx::query_as::<_, Document>(
        "SELECT * FROM documents WHERE id = $1"
    )
    .bind(doc_id)
    .fetch_one(&pool)
    .await?;

    println!("✅ Document retrieved: {}", retrieved.title);

    assert_eq!(retrieved.title, title);
    assert_eq!(retrieved.content, content);
    assert_eq!(retrieved.summary, summary);

    // Cleanup
    sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(doc_id)
        .execute(&pool)
        .await?;

    println!("✅ Test cleanup completed\n");

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn test_document_upsert() -> Result<()> {
    let pool = setup_db().await?;

    println!("\n🔄 Testing document upsert (update if exists)...\n");

    let doc_id = Uuid::new_v4();
    let title = "Upsert Test Document";

    // First insert
    sqlx::query(
        r#"
        INSERT INTO documents (id, title, content, source_path, source_type, indexed_at)
        VALUES ($1, $2, $3, $4, 'pdf', $5)
        "#
    )
    .bind(doc_id)
    .bind(title)
    .bind("Original content")
    .bind("test.pdf")
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await?;

    println!("✅ Document inserted");

    // Upsert (update)
    sqlx::query(
        r#"
        INSERT INTO documents (id, title, content, source_path, source_type, indexed_at)
        VALUES ($1, $2, $3, $4, 'pdf', $5)
        ON CONFLICT (id) DO UPDATE
        SET content = EXCLUDED.content, indexed_at = EXCLUDED.indexed_at
        "#
    )
    .bind(doc_id)
    .bind(title)
    .bind("Updated content")
    .bind("test.pdf")
    .bind(chrono::Utc::now())
    .execute(&pool)
    .await?;

    println!("✅ Document upserted");

    // Verify update
    let row = sqlx::query("SELECT content FROM documents WHERE id = $1")
        .bind(doc_id)
        .fetch_one(&pool)
        .await?;
    
    let content: String = row.get("content");

    assert_eq!(content, "Updated content");
    println!("✅ Content was updated correctly");

    // Cleanup
    sqlx::query("DELETE FROM documents WHERE id = $1")
        .bind(doc_id)
        .execute(&pool)
        .await?;

    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn test_hybrid_search_syntax() -> Result<()> {
    let pool = setup_db().await?;
    println!("\n🔍 Testing hybrid search syntax...\n");

    // We don't need to insert documents because we are testing query parsing/execution,
    // not result relevance (which is covered by other tests).
    // We just want to ensure these queries don't throw errors.

    let test_queries = vec![
        "id:()",
        "id: ()",
        "id:( )",
        "*",
        "id:(*)",
        "",
        "   ",
        "normal query",
    ];

    let dummy_embedding = vec![0.0; 1024]; // Assuming 1024 dim
    let filters = SearchFilters {
        category_id: None,
        date_from: None,
        date_to: None,
        locations: None,
        keywords: None,
        source_types: None,
        authors: None,
        concepts: None,
        organizations: None,
        persons: None,
        products: None,
        word_count_min: None,
        word_count_max: None,
    };

    for query in test_queries {
        println!("Testing query: '{}'", query);
        let result = hybrid_search(
            &pool,
            query,
            &dummy_embedding,
            &filters,
            5,
            0.5,
            0.5
        ).await;

        match result {
            Ok(_) => println!("✅ Query '{}' succeeded", query),
            Err(e) => {
                let error_msg = format!("{:?}", e);
                if error_msg.contains("could not parse query string") {
                    panic!("❌ Query '{}' caused parsing error: {}", query, error_msg);
                } else {
                    println!("⚠️ Query '{}' failed with other error (expected if DB empty or other issues): {:?}", query, e);
                }
            }
        }
    }

    pool.close().await;
    Ok(())
}
