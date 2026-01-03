use rag_chat::services::enrichment::Enricher;

#[tokio::test]
async fn test_ner_extraction_debug() {
    // Skip if no LLM configured
    if std::env::var("METADATA_LLM_API_URL").is_err() {
        println!("Skipping NER test - METADATA_LLM_API_URL not set");
        return;
    }

    let enricher = Enricher::new();

    // Simple text with obvious entities
    let test_text = "Steve Jobs founded Apple Inc. in Cupertino, California. \
                     Tim Cook later became CEO. They created the iPhone and iPad products. \
                     Microsoft and Google are competitors.";

    // Use reflection to call the private method for testing
    // This simulates what extract_metadata does
    let result = enricher.extract_metadata(test_text, "Test Doc").await;

    match result {
        Ok(metadata) => {
            println!("\n=== NER Results ===");
            println!("Entities: {}", serde_json::to_string_pretty(&metadata.entities).unwrap());
            println!("Summary: {:?}", metadata.summary);
            println!("Keywords: {:?}", metadata.keywords);
            println!("Author: {:?}", metadata.author);

            // Check if any entities were extracted
            let persons = metadata.entities["persons"].as_array().unwrap();
            let orgs = metadata.entities["organizations"].as_array().unwrap();
            let locs = metadata.entities["locations"].as_array().unwrap();
            let products = metadata.entities["products"].as_array().unwrap();

            println!("\n=== Entity Counts ===");
            println!("Persons: {}", persons.len());
            println!("Organizations: {}", orgs.len());
            println!("Locations: {}", locs.len());
            println!("Products: {}", products.len());

            // Print what we got vs what we expected
            println!("\n=== Expected vs Actual ===");
            println!("Expected persons: Steve Jobs, Tim Cook");
            println!("Got persons: {:?}", persons);

            println!("\nExpected orgs: Apple Inc., Microsoft, Google");
            println!("Got orgs: {:?}", orgs);

            println!("\nExpected locations: Cupertino, California");
            println!("Got locations: {:?}", locs);

            println!("\nExpected products: iPhone, iPad");
            println!("Got products: {:?}", products);

            // The model should extract at least SOME entities from this obvious text
            let total_entities = persons.len() + orgs.len() + locs.len() + products.len();
            println!("\nTotal entities extracted: {}", total_entities);
            if total_entities == 0 {
                eprintln!("\n❌ WARNING: No entities extracted at all!");
                eprintln!("This suggests the NER model is not working properly");
            } else if total_entities < 5 {
                eprintln!("\n⚠️  WARNING: Only {} entities extracted from obvious text!", total_entities);
                eprintln!("Expected at least: 2 persons + 3 orgs + 2 locations + 2 products = 9 entities");
            }
        }
        Err(e) => {
            eprintln!("NER extraction failed: {:?}", e);
            panic!("Test failed");
        }
    }
}
