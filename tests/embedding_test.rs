use anyhow::Result;
use async_openai::{
    config::OpenAIConfig,
    types::{CreateEmbeddingRequestArgs, EmbeddingInput},
    Client,
};

async fn generate_embedding(text: &str) -> Result<Vec<f32>> {
    dotenvy::from_filename("tests/test.env").ok();

    let api_url = std::env::var("EMBEDDING_API_URL")?;
    let api_key = std::env::var("LLM_API_KEY")?;
    let model = std::env::var("EMBEDDING_MODEL")?;
    let expected_dims: usize = std::env::var("EMBEDDING_DIMENSIONS")
        .unwrap_or_else(|_| "1024".to_string())
        .parse()?;

    let config = OpenAIConfig::new()
        .with_api_base(&api_url)
        .with_api_key(&api_key);

    let client = Client::with_config(config);

    let request = CreateEmbeddingRequestArgs::default()
        .model(&model)
        .input(EmbeddingInput::String(text.to_string()))
        .build()?;

    let response = client.embeddings().create(request).await?;

    let embedding = response
        .data
        .first()
        .ok_or_else(|| anyhow::anyhow!("No embedding in response"))?
        .embedding
        .clone();

    assert_eq!(
        embedding.len(),
        expected_dims,
        "Embedding dimension mismatch"
    );

    Ok(embedding)
}

#[tokio::test]
async fn test_basic_embedding_generation() -> Result<()> {
    println!("\n🔢 Testing basic embedding generation...\n");

    let text = "This is a test sentence for embedding generation.";

    let start = std::time::Instant::now();
    let embedding = generate_embedding(text).await?;
    let elapsed = start.elapsed();

    println!("⏱️  Generation took {:.2}s", elapsed.as_secs_f64());
    println!("📊 Embedding dimensions: {}", embedding.len());
    println!("📈 Sample values: {:?}", &embedding[..5.min(embedding.len())]);

    assert!(!embedding.is_empty(), "Embedding should not be empty");
    assert_eq!(
        embedding.len(),
        1024,
        "Expected 1024 dimensions for Qwen3-Embedding"
    );
    assert!(
        elapsed.as_secs() < 10,
        "Embedding generation should be fast"
    );

    Ok(())
}

#[tokio::test]
async fn test_embedding_similarity() -> Result<()> {
    println!("\n🔍 Testing embedding similarity (cosine)...\n");

    let text1 = "Kubernetes is a container orchestration platform.";
    let text2 = "Kubernetes manages Docker containers in production.";
    let text3 = "Pizza is a delicious Italian food.";

    let emb1 = generate_embedding(text1).await?;
    let emb2 = generate_embedding(text2).await?;
    let emb3 = generate_embedding(text3).await?;

    // Cosine similarity calculation
    let cosine_similarity = |a: &[f32], b: &[f32]| -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (norm_a * norm_b)
    };

    let sim_1_2 = cosine_similarity(&emb1, &emb2);
    let sim_1_3 = cosine_similarity(&emb1, &emb3);

    println!("Similarity (Kubernetes texts): {:.4}", sim_1_2);
    println!("Similarity (Kubernetes vs Pizza): {:.4}", sim_1_3);

    // Related texts should be more similar than unrelated ones
    assert!(
        sim_1_2 > sim_1_3,
        "Related texts should have higher similarity. Got: {} vs {}",
        sim_1_2,
        sim_1_3
    );

    println!("✅ Semantic similarity works correctly");

    Ok(())
}

#[tokio::test]
async fn test_embedding_determinism() -> Result<()> {
    println!("\n🔄 Testing embedding determinism...\n");

    let text = "Platform engineering improves developer experience.";

    let emb1 = generate_embedding(text).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let emb2 = generate_embedding(text).await?;

    // Embeddings should be identical for the same input
    let differences: usize = emb1
        .iter()
        .zip(emb2.iter())
        .filter(|(a, b)| (*a - *b).abs() > 1e-6)
        .count();

    println!(
        "Differences between runs: {} / {}",
        differences,
        emb1.len()
    );

    assert!(
        differences == 0,
        "Embeddings should be deterministic (found {} differences)",
        differences
    );

    println!("✅ Embeddings are deterministic");

    Ok(())
}

#[tokio::test]
async fn test_batch_embedding_speed() -> Result<()> {
    println!("\n⚡ Testing batch embedding speed...\n");

    let texts = vec![
        "First test sentence.",
        "Second test sentence.",
        "Third test sentence.",
        "Fourth test sentence.",
        "Fifth test sentence.",
    ];

    let start = std::time::Instant::now();

    for (i, text) in texts.iter().enumerate() {
        let _ = generate_embedding(text).await?;
        println!("  Embedded text {} / {}", i + 1, texts.len());
    }

    let elapsed = start.elapsed();
    let avg_per_text = elapsed.as_secs_f64() / texts.len() as f64;

    println!("\n⏱️  Total time: {:.2}s", elapsed.as_secs_f64());
    println!("📊 Average per text: {:.2}s", avg_per_text);

    assert!(
        avg_per_text < 5.0,
        "Average embedding time should be under 5 seconds"
    );

    Ok(())
}
