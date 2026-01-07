//! Embedding Service
//!
//! Handles communication with embedding APIs.

use anyhow::{Context, Result};
use serde_json::json;
use std::sync::Arc;
use std::time::Duration;
use crate::config::EmbeddingConfig;

/// Local embedding model wrapper using OpenAI-compatible APIs (LM Studio, OpenRouter, etc.)
#[derive(Clone)]
pub struct Embedder {
    client: Arc<reqwest::Client>,
    api_url: String,
    api_key: Option<String>,
    model_name: String,
}

impl Embedder {
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        let api_key = if config.api_key.is_empty() {
            None
        } else {
            Some(config.api_key.clone())
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_seconds))
            .connect_timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10) // Reuse connections
            .build()?;

        tracing::info!("Initializing Embedder with URL: {}, Model: {}, Timeout: {}s",
            config.api_url, config.model, config.timeout_seconds);

        Ok(Self {
            client: Arc::new(client),
            api_url: config.api_url.clone(),
            api_key,
            model_name: config.model.clone(),
        })
    }

    pub fn get_model_name(&self) -> &str {
        &self.model_name
    }

    pub fn get_api_url(&self) -> &str {
        &self.api_url
    }

    /// Initialize and verify the embedding model
    pub async fn init(&self) -> Result<()> {
        // Skip model verification for OpenRouter (embedding models aren't in /models endpoint)
        if self.api_url.contains("openrouter.ai") {
            tracing::info!("Skipping model verification for OpenRouter - embedding models not in /models endpoint");
            return Ok(());
        }

        let url = format!("{}/models", self.api_url);

        let response = self.client.get(&url)
            .send()
            .await;

        // If we can't connect to list models, just warn but don't fail hard
        // as some providers might not support this endpoint or have different auth
        let response = match response {
            Ok(res) => res,
            Err(e) => {
                tracing::warn!("Could not verify models at {}: {}", url, e);
                return Ok(());
            }
        };

        if !response.status().is_success() {
            tracing::warn!("Failed to list models: {}", response.status());
            return Ok(());
        }

        let json: serde_json::Value = response.json().await?;
        let models = json["data"].as_array();

        if let Some(models) = models {
            let model_ids: Vec<String> = models.iter()
                .filter_map(|m| m["id"].as_str().map(String::from))
                .collect();

            // Only verify if we got a non-empty model list
            if !model_ids.is_empty() {
                if !model_ids.contains(&self.model_name) {
                    tracing::error!("Model '{}' not found in available models: {:?}", self.model_name, model_ids);
                    anyhow::bail!("Model '{}' not found. Available models: {:?}", self.model_name, model_ids);
                } else {
                    tracing::info!("Verified model '{}' is available", self.model_name);
                }
            } else {
                tracing::warn!("Model list is empty, skipping model verification for '{}'", self.model_name);
            }
        }

        Ok(())
    }

    /// Generate embedding for a single text with retry logic
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/embeddings", self.api_url);

        // First attempt
        let mut request = self.client.post(&url);

        // Add authorization header if API key is provided
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .json(&json!({
                "input": text,
                "model": self.model_name
            }))
            .send()
            .await?;

        // Check if model needs loading
        if !response.status().is_success() {
            let error_text = response.text().await?;

            // If model is unloaded or doesn't exist, wait and retry
            if error_text.contains("Model unloaded") || error_text.contains("does not exist") {
                tracing::warn!("Embedding model needs loading, waiting 5 seconds and retrying...");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                // Retry
                let mut retry_request = self.client.post(&url);

                // Add authorization header if API key is provided
                if let Some(api_key) = &self.api_key {
                    retry_request = retry_request.header("Authorization", format!("Bearer {}", api_key));
                }

                let retry_response = retry_request
                    .json(&json!({
                        "input": text,
                        "model": self.model_name
                    }))
                    .send()
                    .await?;

                if !retry_response.status().is_success() {
                    let retry_error = retry_response.text().await?;
                    anyhow::bail!("Embedding API error (after retry): {}", retry_error);
                }

                let json: serde_json::Value = retry_response.json().await?;
                let embedding = json["data"][0]["embedding"]
                    .as_array()
                    .context("Invalid embedding response format")?
                    .iter()
                    .map(|v| v.as_f64().unwrap() as f32)
                    .collect();

                return Ok(embedding);
            } else {
                anyhow::bail!("Embedding API error: {}", error_text);
            }
        }

        let json: serde_json::Value = response.json().await?;
        let embedding = json["data"][0]["embedding"]
            .as_array()
            .context("Invalid embedding response format")?
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();

        Ok(embedding)
    }

    /// Generate embeddings for multiple texts in a single API call
    pub async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        let url = format!("{}/embeddings", self.api_url);

        let mut request = self.client.post(&url);

        // Add authorization header if API key is provided
        if let Some(api_key) = &self.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = request
            .json(&json!({
                "input": texts,
                "model": self.model_name
            }))
            .send()
            .await?;

        if !response.status().is_success() {
            let error = response.text().await?;
            anyhow::bail!("Embedding API error: {}", error);
        }

        let json: serde_json::Value = response.json().await?;
        let data = json["data"].as_array().context("Invalid embedding response format")?;
        
        let mut embeddings = Vec::new();
        for item in data {
            let embedding = item["embedding"]
                .as_array()
                .context("Invalid embedding format")?
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
                .collect();
            embeddings.push(embedding);
        }

        Ok(embeddings)
    }
}
