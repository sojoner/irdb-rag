use anyhow::Result;
use async_openai::{
    config::OpenAIConfig,
    types::{CreateEmbeddingRequestArgs, EmbeddingInput},
    Client,
};
use rag_chat::config::Settings;

/// Generate embedding for a single text using OpenAI-compatible API
async fn generate_embedding(text: &str) -> Result<Vec<f32>> {
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new()?;
    let api_url = settings.embedding.api_url.clone();
    let api_key = settings.embedding.api_key.clone();
    let model = settings.embedding.model.clone();
    let expected_dims: usize = settings.embedding.dimensions as usize;

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

    anyhow::ensure!(
        embedding.len() == expected_dims,
        "Embedding dimension mismatch: expected {}, got {}",
        expected_dims,
        embedding.len()
    );

    Ok(embedding)
}

#[tokio::test]
async fn test_basic_embedding_generation() -> Result<()> {
    let text = "This is a test sentence for embedding generation.";

    let start = std::time::Instant::now();
    let embedding = generate_embedding(text).await?;
    let elapsed = start.elapsed();

    anyhow::ensure!(!embedding.is_empty(), "Embedding should not be empty");
    anyhow::ensure!(
        elapsed.as_secs() < 10,
        "Embedding generation took {:.2}s, should be under 10s",
        elapsed.as_secs_f64()
    );

    println!("✅ Generated embedding with {} dimensions in {:.2}s", embedding.len(), elapsed.as_secs_f64());

    Ok(())
}

#[tokio::test]
async fn test_embedding_similarity() -> Result<()> {
    let text1 = "Kubernetes is a container orchestration platform.";
    let text2 = "Kubernetes manages Docker containers in production.";
    let text3 = "Pizza is a delicious Italian food.";

    let (emb1, emb2, emb3) = tokio::join!(
        generate_embedding(text1),
        generate_embedding(text2),
        generate_embedding(text3),
    );

    let emb1 = emb1?;
    let emb2 = emb2?;
    let emb3 = emb3?;

    // Cosine similarity: dot product / (norm_a * norm_b)
    let cosine_sim = |a: &[f32], b: &[f32]| -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
        dot / (norm_a * norm_b)
    };

    let sim_related = cosine_sim(&emb1, &emb2);
    let sim_unrelated = cosine_sim(&emb1, &emb3);

    anyhow::ensure!(
        sim_related > sim_unrelated,
        "Related texts should be more similar: {:.4} > {:.4}",
        sim_related,
        sim_unrelated
    );

    println!("✅ Semantic similarity verified (related: {:.4}, unrelated: {:.4})", sim_related, sim_unrelated);

    Ok(())
}

#[tokio::test]
async fn test_embedding_determinism() -> Result<()> {
    let text = "Platform engineering improves developer experience.";

    let emb1 = generate_embedding(text).await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    let emb2 = generate_embedding(text).await?;

    // Count significant differences (tolerance for quantized models)
    let differences: usize = emb1
        .iter()
        .zip(emb2.iter())
        .filter(|(a, b)| (*a - *b).abs() > 0.001)
        .count();

    let max_diff = emb1
        .iter()
        .zip(emb2.iter())
        .map(|(a, b)| (*a - *b).abs())
        .fold(0.0, f32::max);

    // Allow up to 1% differences due to quantization in embedding models
    let max_allowed = emb1.len() / 100;
    anyhow::ensure!(
        differences < max_allowed,
        "Too many embedding differences: {}/{} ({:.2}%), max diff: {:.6}",
        differences,
        emb1.len(),
        (differences as f32 / emb1.len() as f32) * 100.0,
        max_diff
    );

    println!("✅ Embeddings are stable (diffs: {}/{}, max: {:.6})", differences, emb1.len(), max_diff);

    Ok(())
}

#[tokio::test]
async fn test_batch_embedding_speed() -> Result<()> {
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new()?;
    let api_url = settings.embedding.api_url.clone();
    let api_key = settings.embedding.api_key.clone();
    let model = settings.embedding.model.clone();

    let texts = vec![
        "First test sentence.",
        "Second test sentence.",
        "Third test sentence.",
        "Fourth test sentence.",
        "Fifth test sentence.",
    ];

    // Measure individual requests
    let start = std::time::Instant::now();
    for text in texts.iter() {
        let _ = generate_embedding(text).await?;
    }
    let individual_time = start.elapsed();

    // Measure batch request
    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    let response = client
        .post(format!("{}/embeddings", api_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "input": texts,
            "encoding_format": "float"
        }))
        .send()
        .await?;
    let batch_time = start.elapsed();

    anyhow::ensure!(
        response.status().is_success(),
        "Batch API failed: {}",
        response.text().await?
    );

    let speedup = individual_time.as_secs_f64() / batch_time.as_secs_f64();
    println!("✅ Batch is {:.2}x faster ({:.2}s individual vs {:.2}s batch)",
        speedup, individual_time.as_secs_f64(), batch_time.as_secs_f64());

    Ok(())
}

#[tokio::test]
async fn test_batch_embedding_api() -> Result<()> {
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new()?;
    let api_url = settings.embedding.api_url.clone();
    let api_key = settings.embedding.api_key.clone();
    let model = settings.embedding.model.clone();
    let expected_dims: usize = settings.embedding.dimensions as usize;

    let texts = vec![
        "Kubernetes is a container orchestration platform.",
        "Docker containers are lightweight and portable.",
        "Cloud computing enables on-demand resource allocation.",
    ];

    let client = reqwest::Client::new();
    let start = std::time::Instant::now();
    let response = client
        .post(format!("{}/embeddings", api_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": model,
            "input": texts.clone(),
            "encoding_format": "float"
        }))
        .send()
        .await?;
    let elapsed = start.elapsed();

    anyhow::ensure!(
        response.status().is_success(),
        "Batch API failed: {}",
        response.text().await?
    );

    let json: serde_json::Value = response.json().await?;
    let data = json["data"].as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid batch response format"))?;

    anyhow::ensure!(
        data.len() == texts.len(),
        "Expected {} embeddings, got {}",
        texts.len(),
        data.len()
    );

    // Verify all embeddings have correct dimensions
    for (i, item) in data.iter().enumerate() {
        let embedding = item["embedding"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Missing embedding at index {}", i))?;

        anyhow::ensure!(
            embedding.len() == expected_dims,
            "Embedding {} has {} dims, expected {}",
            i,
            embedding.len(),
            expected_dims
        );
    }

    println!("✅ Batch API returned {} correct embeddings in {:.2}s", data.len(), elapsed.as_secs_f64());

    Ok(())
}
