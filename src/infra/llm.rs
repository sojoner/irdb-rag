//! LLM Integration Module
//!
//! Handles communication with OpenAI-compatible LLM APIs and embeddings APIs.

use anyhow::Result;
use futures::stream::BoxStream;

use crate::domain::models::{LLMConfig, InfraEmbeddingConfig};

/// Call the LLM API with system and user prompts
pub async fn call_llm(config: &LLMConfig, system: &str, user: &str) -> Result<String> {
    call_llm_with_options(config, system, user, None, None).await
}

/// Call the LLM API with custom max_tokens and temperature
pub async fn call_llm_with_options(
    config: &LLMConfig,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
) -> Result<String> {
    use std::time::Duration;

    // Create client with timeout configuration
    let timeout = std::env::var("LLM_TIMEOUT_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(300); // Default 5 minutes for local LLM Studio

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .connect_timeout(Duration::from_secs(30))
        .build()?;

    // OpenAI-compatible format (works with OpenRouter, etc.)
    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "max_tokens": max_tokens.unwrap_or(2048),
        "temperature": temperature.unwrap_or(0.7)
    });

    let url = format!("{}/chat/completions", config.api_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        anyhow::bail!("LLM API error: {}", error);
    }

    let json: serde_json::Value = response.json().await?;

    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("No response")
        .to_string();

    Ok(content)
}

/// Stream LLM response using SSE
pub async fn stream_llm(
    config: &LLMConfig,
    system: &str,
    user: &str,
) -> Result<BoxStream<'static, Result<String>>> {
    use std::time::Duration;
    use futures::stream::StreamExt;

    let timeout = std::env::var("LLM_TIMEOUT_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(300);

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .connect_timeout(Duration::from_secs(30))
        .build()?;

    let body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user}
        ],
        "max_tokens": 2048,
        "temperature": 0.7,
        "stream": true
    });

    let url = format!("{}/chat/completions", config.api_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        anyhow::bail!("LLM API error: {}", error);
    }

    // Use async_stream to simplify streaming with proper line buffering
    let stream = async_stream::stream! {
        let mut buffer = Vec::new();
        let mut bytes_stream = response.bytes_stream();

        while let Some(result) = bytes_stream.next().await {
            match result {
                Ok(bytes) => {
                    buffer.extend_from_slice(&bytes);

                    // Split on \n boundaries to get complete lines
                    while let Some(pos) = buffer.iter().position(|&b| b == b'\n') {
                        let line = String::from_utf8_lossy(&buffer[..pos]).to_string();
                        buffer.drain(..=pos);

                        // Process SSE data lines
                        if let Some(data) = line.strip_prefix("data: ") {
                            if data == "[DONE]" {
                                continue;
                            }

                            // Parse the JSON chunk from OpenAI API
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                                if let Some(choices) = json.get("choices").and_then(|c| c.as_array()) {
                                    if let Some(choice) = choices.first() {
                                        if let Some(delta) = choice.get("delta") {
                                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                                if !content.is_empty() {
                                                    yield Ok(content.to_string());
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    yield Err(e.into());
                    break;
                }
            }
        }
    }.boxed();

    Ok(stream)
}

/// Generate embeddings for a text string
#[allow(dead_code)]
pub async fn get_embedding(config: &InfraEmbeddingConfig, text: &str) -> Result<Vec<f32>> {
    let embeddings = get_embeddings_batch(config, &[text]).await?;
    Ok(embeddings.into_iter().next().unwrap_or_default())
}

/// Generate embeddings for multiple text strings (batch operation)
#[allow(dead_code)]
pub async fn get_embeddings_batch(config: &InfraEmbeddingConfig, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
    use std::time::Duration;

    let timeout = std::env::var("EMBEDDING_TIMEOUT_SECONDS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(120); // Default 2 minutes

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout))
        .connect_timeout(Duration::from_secs(30))
        .build()?;

    // OpenAI-compatible embeddings API format supports both single string and array of strings
    let input = if texts.len() == 1 {
        serde_json::json!(texts[0])
    } else {
        serde_json::json!(texts)
    };

    let body = serde_json::json!({
        "model": config.model,
        "input": input,
        "encoding_format": "float"
    });

    let url = format!("{}/embeddings", config.api_url);

    let response = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error = response.text().await?;
        anyhow::bail!("Embedding API error: {}", error);
    }

    let json: serde_json::Value = response.json().await?;

    let data = json["data"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("Invalid embedding response format"))?;

    let mut embeddings = vec![vec![]; texts.len()];

    for item in data {
        let index = item["index"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Missing index in embedding response"))? as usize;

        let embedding = item["embedding"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("Invalid embedding format at index {}", index))?
            .iter()
            .filter_map(|v| v.as_f64().map(|f| f as f32))
            .collect::<Vec<f32>>();

        if embedding.len() != config.dimensions as usize {
            anyhow::bail!(
                "Embedding dimension mismatch at index {}: expected {}, got {}",
                index,
                config.dimensions,
                embedding.len()
            );
        }

        embeddings[index] = embedding;
    }

    Ok(embeddings)
}
