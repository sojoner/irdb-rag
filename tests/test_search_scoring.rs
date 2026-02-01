use sqlx::PgPool;

async fn get_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string()
    });

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_search_scores_are_normalized() {
    let pool = get_test_pool().await;

    // Search for a common term that will return results
    let query = "deep";

    // Test BM25 search
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

    let results = rag_chat::infra::db::bm25_search(
        &pool, 
        query, 
        &filters, 
        20, 
        0, 
        &rag_chat::infra::db::SortOrder::Relevance
    )
        .await
        .expect("Search should succeed");

    // All scores should be between 0.0 and 1.0 for proper percentage display
    for result in &results {
        assert!(
            result.combined_score >= 0.0 && result.combined_score <= 1.0,
            "Score {} is outside valid range [0.0, 1.0]. This will display incorrectly as {}% in UI",
            result.combined_score,
            result.combined_score * 100.0
        );
    }

    // If we have results, the highest score should be close to 1.0
    if !results.is_empty() {
        let max_score = results.iter()
            .map(|r| r.combined_score)
            .fold(0.0f64, f64::max);

        // Top result should be normalized to close to 1.0
        assert!(
            max_score >= 0.9,
            "Top result score {} is too low. Normalization may not be working correctly",
            max_score
        );
    }
}

#[tokio::test]
async fn test_simple_bm25_search_normalizes_scores() {
    let pool = get_test_pool().await;

    let query = "deep learning";
    let search_fields = vec!["content", "title", "summary"];

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

    let results = rag_chat::infra::db::simple_bm25_search(
        &pool,
        query,
        &search_fields,
        &filters,
        20
    )
    .await
    .expect("Search should succeed");

    // Verify all scores are normalized to [0.0, 1.0]
    for result in &results {
        assert!(
            result.bm25_score >= 0.0 && result.bm25_score <= 1.0,
            "BM25 score {} is outside valid range",
            result.bm25_score
        );
        assert!(
            result.combined_score >= 0.0 && result.combined_score <= 1.0,
            "Combined score {} is outside valid range",
            result.combined_score
        );
    }
}
