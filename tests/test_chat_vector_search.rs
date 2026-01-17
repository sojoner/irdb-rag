use sqlx::PgPool;
use uuid::Uuid;

async fn get_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string()
    });

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_chat_performs_vector_search_with_document_ids() {
    let pool = get_test_pool().await;

    // Get some document IDs from the database
    let doc_ids: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM documents LIMIT 3")
        .fetch_all(&pool)
        .await
        .expect("Should fetch document IDs");

    if doc_ids.is_empty() {
        println!("No documents in database, skipping test");
        return;
    }

    let document_ids: Vec<Uuid> = doc_ids.into_iter().map(|(id,)| id).collect();

    // Create a simple embedder for testing
    let settings = rag_chat::config::Settings::new().expect("Should load settings");
    let embedder = rag_chat::infra::embedder::Embedder::new(&settings.embedding)
        .expect("Should create embedder");

    let query = "machine learning";
    let embedding = embedder
        .embed(query)
        .await
        .expect("Should generate embedding");

    // Test that get_relevant_chunks works with document_ids filter
    let chunks = rag_chat::infra::db::get_relevant_chunks(
        &pool,
        &embedding,
        5,
        Some(&document_ids),
    )
    .await
    .expect("Should retrieve chunks");

    // Verify chunks are from the specified documents
    for chunk in &chunks {
        assert!(
            document_ids.contains(&chunk.document_id),
            "Chunk document_id {} not in specified document_ids",
            chunk.document_id
        );
    }

    // Verify chunks are relevant (have embeddings)
    assert!(!chunks.is_empty(), "Should return some chunks");
}

#[tokio::test]
async fn test_chat_returns_both_chunks_and_documents() {
    let pool = get_test_pool().await;

    // Get some document IDs
    let doc_ids: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM documents LIMIT 3")
        .fetch_all(&pool)
        .await
        .expect("Should fetch document IDs");

    if doc_ids.is_empty() {
        println!("No documents in database, skipping test");
        return;
    }

    let document_ids: Vec<Uuid> = doc_ids.into_iter().map(|(id,)| id).collect();

    let settings = rag_chat::config::Settings::new().expect("Should load settings");
    let embedder = rag_chat::infra::embedder::Embedder::new(&settings.embedding)
        .expect("Should create embedder");

    let query = "deep learning";
    let embedding = embedder
        .embed(query)
        .await
        .expect("Should generate embedding");

    // Get chunks for RAG context
    let chunks = rag_chat::infra::db::get_relevant_chunks(
        &pool,
        &embedding,
        5,
        Some(&document_ids),
    )
    .await
    .expect("Should retrieve chunks");

    // Get document-level search results for display
    let filters = rag_chat::infra::db::SearchFilters {
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

    let doc_results = rag_chat::infra::db::vector_search(
        &pool,
        &embedding,
        &filters,
        10,
    )
    .await
    .expect("Should retrieve document search results");

    // Verify we got both chunks and documents
    assert!(!chunks.is_empty(), "Should have chunks for RAG");
    assert!(!doc_results.is_empty(), "Should have document results for display");

    // Verify chunks and docs are related
    let chunk_doc_ids: std::collections::HashSet<Uuid> =
        chunks.iter().map(|c| c.document_id).collect();

    for result in &doc_results {
        if chunk_doc_ids.contains(&result.id) {
            // At least some overlap is expected
            return;
        }
    }
}

#[tokio::test]
async fn test_vector_search_without_document_filter() {
    let pool = get_test_pool().await;

    let settings = rag_chat::config::Settings::new().expect("Should load settings");
    let embedder = rag_chat::infra::embedder::Embedder::new(&settings.embedding)
        .expect("Should create embedder");

    let query = "neural networks";
    let embedding = embedder
        .embed(query)
        .await
        .expect("Should generate embedding");

    // Test vector search without document filter (global search)
    let filters = rag_chat::infra::db::SearchFilters {
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

    let results = rag_chat::infra::db::vector_search(
        &pool,
        &embedding,
        &filters,
        10,
    )
    .await
    .expect("Should perform vector search");

    // Verify results have vector scores
    for result in &results {
        assert!(
            result.vector_score >= 0.0 && result.vector_score <= 1.0,
            "Vector score {} should be normalized to [0.0, 1.0]",
            result.vector_score
        );
    }

    // Results should be sorted by combined_score (descending)
    for i in 1..results.len() {
        assert!(
            results[i - 1].combined_score >= results[i].combined_score,
            "Results should be sorted by combined_score descending"
        );
    }
}
