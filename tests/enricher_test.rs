use rag_chat::enricher::{enrich_chunk, Enricher};

#[test]
fn test_enrich_chunk() {
    let title = "Test Document";
    let summary = "This is a summary.";
    let keywords = vec!["key1".to_string(), "key2".to_string()];
    let questions = vec!["What is this?".to_string(), "Why?".to_string()];
    let chunk = "This is the chunk content.";

    let enriched = enrich_chunk(title, summary, &keywords, &questions, chunk);

    let expected = "Title: Test Document\nSummary: This is a summary.\nKeywords: key1, key2\nQuestions:\n- What is this?\n- Why?\n---\nThis is the chunk content.";

    assert_eq!(enriched, expected);
}

#[test]
fn test_enrich_chunk_no_questions() {
    let title = "Test Document";
    let summary = "This is a summary.";
    let keywords = vec!["key1".to_string()];
    let questions: Vec<String> = vec![];
    let chunk = "Chunk content.";

    let enriched = enrich_chunk(title, summary, &keywords, &questions, chunk);

    let expected = "Title: Test Document\nSummary: This is a summary.\nKeywords: key1\nQuestions:\n\n---\nChunk content.";

    assert_eq!(enriched, expected);
}

#[tokio::test]
async fn test_enrich_file_integration() {
    // This test requires LLM services to be running
    // Skip if METADATA_LLM_API_URL is not set
    if std::env::var("METADATA_LLM_API_URL").is_err() {
        println!("Skipping integration test - METADATA_LLM_API_URL not set");
        return;
    }

    let enricher = Enricher::new();

    // Create a test file
    let test_content = "The Golden Path for Platform Engineering\n\n\
        Platform engineering is about providing easy-to-use Golden Paths to reduce effort for teams. \
        At Financial One ACME, we identified several use cases that can be provided as self-service. \
        Golden Paths were created by companies like Spotify to guide engineers through supported ways \
        of getting things done using particular services.";

    let test_dir = std::env::temp_dir().join("rag_test");
    std::fs::create_dir_all(&test_dir).unwrap();
    let test_file = test_dir.join("test_doc.txt");
    std::fs::write(&test_file, test_content).unwrap();

    // Test enrichment
    let result = enricher.enrich_file(&test_file).await;

    // Cleanup
    std::fs::remove_file(&test_file).ok();

    if let Err(e) = &result {
        eprintln!("Enrichment error: {:?}", e);
    }
    assert!(result.is_ok(), "Enrichment should succeed: {:?}", result.as_ref().err());

    let (content, metadata) = result.unwrap();

    // Verify content was extracted
    assert!(!content.is_empty(), "Content should not be empty");

    // Verify metadata fields are populated
    assert!(metadata.title.is_some(), "Title should be set");
    assert!(metadata.summary.is_some(), "Summary should be generated");

    let summary = metadata.summary.unwrap();
    assert!(!summary.is_empty(), "Summary should not be empty");
    println!("Generated summary: {}", summary);

    // Keywords should be extracted
    assert!(!metadata.keywords.is_empty(), "Keywords should be extracted");
    println!("Generated keywords: {:?}", metadata.keywords);

    // Entities should have proper structure even if empty
    assert!(metadata.entities.is_object(), "Entities should be an object");
    println!("Extracted entities: {}", serde_json::to_string_pretty(&metadata.entities).unwrap());
}
