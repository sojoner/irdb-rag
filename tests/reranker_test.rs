/// Re-ranker Service Integration Tests
///
/// Tests for the Qwen re-ranker service integration with Ollama.
/// The re-ranker improves search result and chat context relevance ordering.
///
/// Prerequisites:
/// - Ollama service running (docker compose up -d)
/// - Qwen reranker model pulled: `ollama pull dengcao/Qwen3-Reranker-0.6B:Q5_K_M`
/// - RUN_ENV=test-gpu for loading GPU test configuration

use anyhow::Result;
use rag_chat::config::Settings;
use rag_chat::infra::reranker::Reranker;

/// Check if Ollama reranker service is available
async fn is_ollama_reranker_available() -> bool {
    let settings = Settings::new().ok();
    if let Some(settings) = settings {
        if !settings.reranking.enabled {
            eprintln!("⚠️  Reranker disabled in configuration");
            return false;
        }

        let url = format!("{}/api/chat", settings.reranking.api_url);
        let response = reqwest::Client::new()
            .post(&url)
            .timeout(std::time::Duration::from_secs(10))
            .json(&serde_json::json!({
                "model": settings.reranking.model,
                "messages": [{"role": "user", "content": "test"}],
                "stream": false
            }))
            .send()
            .await;

        if let Ok(resp) = response {
            return resp.status().is_success();
        }
    }
    false
}

/// Test basic reranker connectivity and model availability
#[tokio::test]
async fn test_reranker_connectivity() -> Result<()> {
    eprintln!("🔍 Checking Ollama reranker availability...");

    if !is_ollama_reranker_available().await {
        eprintln!("⚠️  Skipping test: Ollama reranker service not available");
        eprintln!("    Make sure to:");
        eprintln!("    1. Start Ollama: docker compose up -d ollama");
        eprintln!("    2. Pull model: ollama pull dengcao/Qwen3-Reranker-0.6B:Q5_K_M");
        eprintln!("    3. Enable reranker in config: reranking.enabled = true");
        return Ok(());
    }

    eprintln!("✓ Ollama reranker is available");

    std::env::set_var("RUN_ENV", "test-gpu");
    let settings = Settings::new()?;

    let reranker = Reranker::new(&settings.reranking)?;
    reranker.init().await?;

    println!("✅ Reranker initialized successfully");
    println!("   Model: {}", reranker.get_model_name());
    println!("   API URL: {}", settings.reranking.api_url);

    Ok(())
}

/// Test single document reranking
#[tokio::test]
async fn test_rerank_single_document() -> Result<()> {
    if !is_ollama_reranker_available().await {
        eprintln!("⚠️  Skipping test: Ollama reranker not available");
        return Ok(());
    }

    std::env::set_var("RUN_ENV", "test-gpu");
    let settings = Settings::new()?;
    let reranker = Reranker::new(&settings.reranking)?;

    let query = "What is machine learning?";
    let relevant_doc = "Machine learning is a subset of artificial intelligence that enables systems to learn and improve from experience without being explicitly programmed.";
    let irrelevant_doc = "The capital of France is Paris. It is located in northern France on the Seine River.";

    let relevant_score = reranker.rerank_single(query, relevant_doc).await?;
    let irrelevant_score = reranker.rerank_single(query, irrelevant_doc).await?;

    println!("Query: '{}'", query);
    println!("Relevant doc score: {:.2}", relevant_score);
    println!("Irrelevant doc score: {:.2}", irrelevant_score);

    // Relevant should score higher than irrelevant
    assert!(
        relevant_score >= irrelevant_score,
        "Relevant document should score higher ({} >= {})",
        relevant_score,
        irrelevant_score
    );

    println!(
        "✅ Single document reranking works (relevant: {:.2}, irrelevant: {:.2})",
        relevant_score, irrelevant_score
    );

    Ok(())
}

/// Test batch reranking of multiple documents
#[tokio::test]
async fn test_rerank_batch_documents() -> Result<()> {
    if !is_ollama_reranker_available().await {
        eprintln!("⚠️  Skipping test: Ollama reranker not available");
        return Ok(());
    }

    std::env::set_var("RUN_ENV", "test-gpu");
    let settings = Settings::new()?;
    let reranker = Reranker::new(&settings.reranking)?;

    let query = "cloud computing platforms";
    let documents = vec![
        "Kubernetes is an open-source container orchestration platform for automating deployment, scaling, and management of containerized applications.",
        "AWS provides cloud computing services including compute, storage, and databases.",
        "The Eiffel Tower is a wrought-iron lattice tower in Paris, France.",
        "Microservices architecture breaks applications into loosely coupled, independently deployable services.",
        "Pizza is a traditional Italian dish consisting of a yeasted flatbread typically topped with tomato sauce and cheese.",
    ];

    let start = std::time::Instant::now();
    let scores = reranker.rerank_batch(query, &documents).await?;
    let elapsed = start.elapsed();

    println!("Query: '{}'", query);
    println!("Batch reranking {} documents took {:.2}s", documents.len(), elapsed.as_secs_f64());
    println!("\nScores:");

    for (i, (doc, score)) in documents.iter().zip(scores.iter()).enumerate() {
        let preview = if doc.len() > 60 {
            format!("{}...", &doc[..60])
        } else {
            doc.to_string()
        };
        println!("  [{}] {:.2} - {}", i, score, preview);
    }

    // Verify scores are in valid range
    for (i, score) in scores.iter().enumerate() {
        assert!(
            *score >= 0.0 && *score <= 1.0,
            "Score {} out of range: {}",
            i,
            score
        );
    }

    // Check that at least one cloud-related doc has higher score than pizza doc
    let pizza_idx = 4; // "Pizza is..."
    let has_higher_scores = scores[..4]
        .iter()
        .any(|s| s > &scores[pizza_idx]);

    assert!(
        has_higher_scores,
        "At least one cloud doc should score higher than pizza doc"
    );

    println!(
        "✅ Batch reranking verified ({:.2}s for {} docs)",
        elapsed.as_secs_f64(),
        documents.len()
    );

    Ok(())
}

/// Test rerank_and_sort functionality
#[tokio::test]
async fn test_rerank_and_sort() -> Result<()> {
    if !is_ollama_reranker_available().await {
        eprintln!("⚠️  Skipping test: Ollama reranker not available");
        return Ok(());
    }

    std::env::set_var("RUN_ENV", "test-gpu");
    let settings = Settings::new()?;
    let reranker = Reranker::new(&settings.reranking)?;

    let query = "web development frameworks";
    let documents = vec![
        "React is a JavaScript library for building user interfaces with reusable components.",
        "Python is a high-level programming language.",
        "Vue.js is a progressive JavaScript framework for building user interfaces.",
        "Cooking pasta requires boiling water and adding salt.",
        "Next.js is a React framework for building full-stack web applications.",
    ];

    let ranked = reranker
        .rerank_and_sort(query, &documents)
        .await?;

    println!("Query: '{}'", query);
    println!("Sorted results by relevance:\n");

    for (position, doc) in ranked.iter().enumerate() {
        let preview = if documents[doc.index].len() > 60 {
            format!("{}...", &documents[doc.index][..60])
        } else {
            documents[doc.index].to_string()
        };
        println!("  [{}] Score: {:.2} - {}", position, doc.score, preview);
    }

    // Verify results are sorted (descending)
    let scores: Vec<f64> = ranked.iter().map(|r| r.score).collect();
    for i in 0..scores.len() - 1 {
        assert!(
            scores[i] >= scores[i + 1],
            "Results not sorted: {:.2} < {:.2}",
            scores[i],
            scores[i + 1]
        );
    }

    println!(
        "✅ Results sorted correctly by relevance score"
    );

    Ok(())
}

/// Test reranking performance (parallel vs sequential would be)
#[tokio::test]
async fn test_reranking_performance() -> Result<()> {
    if !is_ollama_reranker_available().await {
        eprintln!("⚠️  Skipping test: Ollama reranker not available");
        return Ok(());
    }

    std::env::set_var("RUN_ENV", "test-gpu");
    let settings = Settings::new()?;
    let reranker = Reranker::new(&settings.reranking)?;

    let query = "artificial intelligence and machine learning";
    let documents = vec![
        "AI is transforming industries through automation and intelligent decision-making.",
        "Machine learning models learn patterns from data without explicit programming.",
        "Deep learning uses neural networks with multiple layers for complex pattern recognition.",
        "Natural language processing enables computers to understand and generate human language.",
        "Computer vision systems can analyze and interpret visual information from images and videos.",
        "Reinforcement learning trains agents through reward and penalty feedback mechanisms.",
        "Supervised learning uses labeled training data to train predictive models.",
        "Unsupervised learning discovers patterns in unlabeled data clustering and dimensionality reduction.",
        "Transfer learning leverages knowledge from one task to improve performance on another.",
        "The Pythagorean theorem states that a² + b² = c² in right triangles.",
    ];

    let start = std::time::Instant::now();
    let _scores = reranker
        .rerank_batch(query, &documents)
        .await?;
    let total_time = start.elapsed();

    let avg_per_doc = total_time.as_secs_f64() / documents.len() as f64;

    println!("Performance Test Results:");
    println!("  Total documents: {}", documents.len());
    println!("  Total time: {:.3}s", total_time.as_secs_f64());
    println!("  Average per document: {:.3}s", avg_per_doc);
    println!("  Throughput: {:.1} docs/sec", 1.0 / avg_per_doc);

    // Performance expectation: should complete batch of 10 in < 5 seconds
    // (parallel processing should be much faster than sequential)
    assert!(
        total_time.as_secs_f64() < 5.0,
        "Batch reranking too slow: {:.2}s for {} docs",
        total_time.as_secs_f64(),
        documents.len()
    );

    println!(
        "✅ Performance acceptable: {:.2}s for {} documents",
        total_time.as_secs_f64(),
        documents.len()
    );

    Ok(())
}

/// Test graceful degradation when model not found
#[tokio::test]
async fn test_reranker_with_invalid_model() -> Result<()> {
    // This test checks that providing a non-existent model is handled gracefully
    std::env::set_var("RUN_ENV", "test-gpu");

    let mut settings = Settings::new()?;
    // Override with a non-existent model
    settings.reranking.model = "nonexistent/model:invalid".to_string();

    match Reranker::new(&settings.reranking) {
        Ok(reranker) => {
            // Reranker created, but init should fail
            match reranker.init().await {
                Ok(_) => {
                    println!("⚠️  Init unexpectedly succeeded with invalid model");
                }
                Err(e) => {
                    println!("✅ Graceful error on invalid model: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✅ Graceful error on invalid model creation: {}", e);
        }
    }

    Ok(())
}
