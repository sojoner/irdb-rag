//! LLM Integration Module
//!
//! Handles communication with OpenAI-compatible LLM APIs and embeddings APIs.

use anyhow::Result;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LLMConfig {
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub dimensions: u32,
}

impl LLMConfig {
    pub fn from_env() -> Self {
        Self {
            provider: std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string()),
            api_url: std::env::var("LLM_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            api_key: std::env::var("LLM_API_KEY").unwrap_or_default(),
            model: std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4".to_string()),
        }
    }

    /// Create config for metadata extraction (uses faster, non-reasoning model)
    pub fn for_metadata() -> Self {
        Self {
            provider: std::env::var("METADATA_LLM_PROVIDER")
                .unwrap_or_else(|_| std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string())),
            api_url: std::env::var("METADATA_LLM_API_URL")
                .unwrap_or_else(|_| std::env::var("LLM_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string())),
            api_key: std::env::var("METADATA_LLM_API_KEY")
                .unwrap_or_else(|_| std::env::var("LLM_API_KEY").unwrap_or_default()),
            model: std::env::var("METADATA_LLM_MODEL")
                .unwrap_or_else(|_| "ibm/granite-4-h-tiny".to_string()),
        }
    }

    /// Create config for NER (Named Entity Recognition)
    pub fn for_ner() -> Self {
        Self {
            provider: std::env::var("NER_LLM_PROVIDER")
                .unwrap_or_else(|_| std::env::var("METADATA_LLM_PROVIDER")
                    .unwrap_or_else(|_| std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string()))),
            api_url: std::env::var("NER_LLM_API_URL")
                .unwrap_or_else(|_| std::env::var("METADATA_LLM_API_URL")
                    .unwrap_or_else(|_| std::env::var("LLM_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()))),
            api_key: std::env::var("NER_LLM_API_KEY")
                .unwrap_or_else(|_| std::env::var("METADATA_LLM_API_KEY")
                    .unwrap_or_else(|_| std::env::var("LLM_API_KEY").unwrap_or_default())),
            model: std::env::var("NER_LLM_MODEL")
                .unwrap_or_else(|_| "google/gemini-3-flash-preview".to_string()),
        }
    }
}

impl EmbeddingConfig {
    #[allow(dead_code)]
    pub fn from_env() -> Self {
        let dimensions = std::env::var("EMBEDDING_DIMENSIONS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .expect("EMBEDDING_DIMENSIONS environment variable must be set");

        Self {
            provider: std::env::var("EMBEDDING_PROVIDER").unwrap_or_else(|_| "openai".to_string()),
            api_url: std::env::var("EMBEDDING_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            api_key: std::env::var("EMBEDDING_API_KEY").unwrap_or_default(),
            model: std::env::var("EMBEDDING_MODEL").unwrap_or_else(|_| "text-embedding-3-small".to_string()),
            dimensions,
        }
    }
}

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

/// Generate embeddings for a text string
#[allow(dead_code)]
pub async fn get_embedding(config: &EmbeddingConfig, text: &str) -> Result<Vec<f32>> {
    let embeddings = get_embeddings_batch(config, &[text]).await?;
    Ok(embeddings.into_iter().next().unwrap_or_default())
}

/// Generate embeddings for multiple text strings (batch operation)
#[allow(dead_code)]
pub async fn get_embeddings_batch(config: &EmbeddingConfig, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
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
