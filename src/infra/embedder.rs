//! Embedding Service
//!
//! Handles communication with embedding APIs using async-openai.

use crate::config::EmbeddingConfig;
use anyhow::{Context, Result};
use async_openai::types::CreateEmbeddingRequestArgs;
use async_openai::{
    config::{Config as OpenAIConfigTrait, OpenAIConfig},
    Client,
};
use std::sync::Arc;

/// Local embedding model wrapper using OpenAI-compatible APIs (LM Studio, OpenRouter, etc.)
#[derive(Clone)]
pub struct Embedder {
    client: Arc<Client<OpenAIConfig>>,
    model_name: String,
    api_url: String,
}

impl Embedder {
    pub fn new(config: &EmbeddingConfig) -> Result<Self> {
        // Configure OpenAI client for OpenAI-compatible APIs
        // Note: async-openai adds /v1 automatically to the base URL
        let mut openai_config = OpenAIConfig::new().with_api_base(&config.api_url);

        // Only set API key if it's not empty
        if !config.api_key.is_empty() {
            openai_config = openai_config.with_api_key(&config.api_key);
        }

        tracing::info!(
            "Initializing Embedder - Base URL: {}, Full API Base: {}, Model: {}",
            config.api_url,
            openai_config.api_base(),
            config.model
        );

        let client = Client::with_config(openai_config);

        Ok(Self {
            client: Arc::new(client),
            model_name: config.model.clone(),
            api_url: config.api_url.clone(),
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
        // Try to list models to verify connectivity
        // If this fails, just warn but don't fail hard
        match self.client.models().list().await {
            Ok(models) => {
                let model_ids: Vec<String> = models.data.iter().map(|m| m.id.clone()).collect();

                if !model_ids.is_empty() && !model_ids.contains(&self.model_name) {
                    tracing::warn!(
                        "Model '{}' not found in available models: {:?}",
                        self.model_name,
                        model_ids
                    );
                } else if !model_ids.is_empty() {
                    tracing::info!("Verified model '{}' is available", self.model_name);
                }
            }
            Err(e) => {
                tracing::warn!("Could not verify models: {}", e);
            }
        }

        Ok(())
    }

    /// Generate embedding for a single text with retry logic
    pub async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.model_name)
            .input(text)
            .build()?;

        // First attempt
        let response = self.client.embeddings().create(request.clone()).await;

        let response = match response {
            Ok(r) => r,
            Err(e) => {
                let error_msg = e.to_string();
                // If model is unloaded or doesn't exist, wait and retry
                if error_msg.contains("Model unloaded") || error_msg.contains("does not exist") {
                    tracing::warn!(
                        "Embedding model needs loading, waiting 5 seconds and retrying..."
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

                    // Retry
                    self.client
                        .embeddings()
                        .create(request)
                        .await
                        .context("Embedding API error (after retry)")?
                } else {
                    return Err(e.into());
                }
            }
        };

        // Extract the embedding vector
        let embedding = response
            .data
            .first()
            .context("No embedding data in response")?
            .embedding
            .clone();

        Ok(embedding)
    }

    /// Generate embeddings for multiple texts in a single API call
    pub async fn embed_batch(&self, texts: Vec<&str>) -> Result<Vec<Vec<f32>>> {
        let input: Vec<String> = texts.iter().map(|s| s.to_string()).collect();

        let request = CreateEmbeddingRequestArgs::default()
            .model(&self.model_name)
            .input(input)
            .build()?;

        let response = self.client.embeddings().create(request).await?;

        // Extract all embeddings and maintain order
        let embeddings: Vec<Vec<f32>> = response
            .data
            .into_iter()
            .map(|item| item.embedding)
            .collect();

        Ok(embeddings)
    }
}
