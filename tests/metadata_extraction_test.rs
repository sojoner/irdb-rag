use anyhow::Result;
use rag_chat::indexer::parse_metadata_response;

#[test]
fn test_metadata_parsing_tech_document() -> Result<()> {
    println!("\n🏷️  Testing metadata parsing (Tech Document)...\n");

    // Simulated LLM response in SLIM NER format
    let llm_response = "
        summary: ['This document discusses cloud providers and microservices.'],
        topics: ['cloud computing', 'microservices', 'infrastructure'],
        persons: ['Martin Fowler'],
        organizations: ['Amazon Web Services', 'Microsoft Azure', 'Netflix', 'Spotify', 'CNCF'],
        products: ['Kubernetes', 'Prometheus'],
        locations: [],
        concepts: [],
        questions: ['What are the major cloud providers?', 'Who uses AWS?']
    ";

    let (summary, keywords, entities) = parse_metadata_response(llm_response);

    println!("📝 Summary: {}", summary);
    println!("🔑 Keywords: {:?}", keywords);
    println!("📦 Entities: {}", serde_json::to_string_pretty(&entities)?);

    // Verify summary
    assert!(!summary.is_empty(), "Should have a summary");
    assert!(summary.contains("cloud providers"), "Summary content check");

    // Verify keywords
    assert!(!keywords.is_empty(), "Should have keywords");
    assert!(keywords.contains(&"cloud computing".to_string()), "Should contain 'cloud computing'");

    // Verify entities structure
    let entities_obj = entities.as_object().expect("Entities should be an object");
    
    let get_list = |key: &str| -> Vec<String> {
        entities_obj.get(key)
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };

    let organizations = get_list("organizations");
    let products = get_list("products");
    let persons = get_list("persons");

    // Should identify organizations
    assert!(
        organizations.contains(&"Amazon Web Services".to_string()),
        "Should identify AWS"
    );

    // Should identify products/technologies
    assert!(
        products.contains(&"Kubernetes".to_string()),
        "Should identify Kubernetes"
    );

    // Should identify persons
    assert!(
        persons.contains(&"Martin Fowler".to_string()),
        "Should identify Martin Fowler"
    );

    Ok(())
}

#[test]
fn test_metadata_parsing_principles_document() -> Result<()> {
    println!("\n💡 Testing metadata parsing (Principles Document)...\n");

    let llm_response = "
        summary: ['Overview of key decision-making principles.'],
        topics: ['decision-making', 'problem-solving', 'management', 'psychology'],
        concepts: ['Pareto Principle', 'Solomon\\'s Paradox', 'Occam\\'s Razor'],
        persons: [],
        organizations: [],
        products: []
    ";

    let (_, keywords, entities) = parse_metadata_response(llm_response);

    println!("🔑 Keywords: {:?}", keywords);
    println!("📦 Entities: {}", serde_json::to_string_pretty(&entities)?);

    let entities_obj = entities.as_object().expect("Entities should be an object");
    let concepts = entities_obj.get("concepts")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
        .unwrap_or_default();

    // Should identify concepts/principles
    assert!(
        concepts.contains(&"Pareto Principle".to_string()),
        "Should identify Pareto Principle"
    );

    // Keywords should be themes
    assert!(
        keywords.contains(&"decision-making".to_string()),
        "Should include decision-making"
    );

    Ok(())
}

#[test]
fn test_metadata_parsing_minimal_content() -> Result<()> {
    println!("\n📝 Testing minimal content parsing...\n");

    let llm_response = "
        summary: ['Minimal summary.'],
        topics: ['best practices'],
        concepts: [],
        persons: [],
        organizations: [],
        products: []
    ";

    let (summary, keywords, entities) = parse_metadata_response(llm_response);

    println!("📝 Summary: {}", summary);
    println!("🔑 Keywords: {:?}", keywords);
    println!("📦 Entities: {}", serde_json::to_string_pretty(&entities)?);

    assert!(!summary.is_empty(), "Should extract summary");
    assert!(!keywords.is_empty(), "Should extract keywords");

    Ok(())
}
