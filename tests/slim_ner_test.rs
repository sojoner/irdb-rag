use rag_chat::llm::{self, LLMConfig};


#[tokio::test]
async fn test_slim_ner_prompting() {
    // Setup
    dotenvy::from_filename("tests/test.env").ok();
    
    let config = LLMConfig::for_metadata();
    eprintln!("Testing model: {}", config.model);

    let text = "Apple Inc. was founded by Steve Jobs in Cupertino. The iPhone was released in 2007.";

    // Attempt 1: Standard Chat Prompt
    eprintln!("\n--- Attempt 1: Standard Chat Prompt ---");
    let system_prompt = "Extract entities: persons, organizations, locations, products.";
    let response = llm::call_llm_with_options(&config, system_prompt, text, None, Some(0.1)).await;
    match response {
        Ok(r) => eprintln!("Response:\n{}", r),
        Err(e) => eprintln!("Error: {}", e),
    }

    // Attempt 3: SLIM NER Format with Topics
    eprintln!("\n--- Attempt 3: SLIM NER Format with Topics ---");
    let params = "persons, organizations, locations, products, topics";
    let special_prompt = format!("<human>: {}\n<classify> {} </classify>\n<bot>:", text, params);
    
    let response = llm::call_llm_with_options(&config, "", &special_prompt, None, Some(0.1)).await;
    match response {
        Ok(r) => eprintln!("Response:\n{}", r),
        Err(e) => eprintln!("Error: {}", e),
    }
}
