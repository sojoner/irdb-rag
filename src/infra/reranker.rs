//! Re-ranking Service
//!
//! Handles re-ranking of search results and chat chunks using Qwen reranker model
//! via Ollama's chat API endpoint. Re-ranking is optional and gracefully degrades
//! if the service is unavailable.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::task;

use crate::config::RerankerConfig;

/// Re-ranker service for improving document/chunk relevance ordering
#[derive(Clone)]
pub struct Reranker {
    client: Arc<reqwest::Client>,
    api_url: String,
    #[allow(dead_code)]
    api_key: String,
    model: String,
}

/// Ranked document result with original index and relevance score
#[derive(Debug, Clone)]
pub struct RankedDocument {
    pub index: usize,
    pub score: f64, // 0.0 = not relevant, 1.0 = relevant
}

// ============================================
// Internal API Structures (Ollama Chat Format)
// ============================================

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    stream: bool,
    options: ChatOptions,
}

#[derive(Debug, Serialize)]
struct ChatOptions {
    temperature: f32,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    message: ChatMessage,
    #[allow(dead_code)]
    done: bool,
}

// ============================================
// Reranker Implementation
// ============================================

impl Reranker {
    /// Create a new reranker instance with configured HTTP client
    pub fn new(config: &RerankerConfig) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_seconds))
            .connect_timeout(std::time::Duration::from_secs(30))
            .build()
            .context("Failed to create HTTP client for reranker")?;

        tracing::info!(
            "Initializing Reranker - API URL: {}, Model: {}",
            config.api_url,
            config.model
        );

        Ok(Self {
            client: Arc::new(client),
            api_url: config.api_url.clone(),
            api_key: config.api_key.clone(),
            model: config.model.clone(),
        })
    }

    /// Initialize and verify the reranker connection to Ollama
    pub async fn init(&self) -> Result<()> {
        // Try a simple rerank with a test query to verify connectivity
        match self.rerank_single("test", "test document").await {
            Ok(_) => {
                tracing::info!("Reranker verified and ready");
                Ok(())
            }
            Err(e) => {
                tracing::warn!("Reranker initialization check failed: {}", e);
                Err(e).context("Reranker initialization failed")
            }
        }
    }

    pub fn get_model_name(&self) -> &str {
        &self.model
    }

    /// Re-rank a single query-document pair
    /// Returns 1.0 if document is relevant, 0.0 if not
    pub async fn rerank_single(&self, query: &str, document: &str) -> Result<f64> {
        let prompt = format!(
            "You are an expert relevance grader. Is the following document relevant to the user's query? Reply 'Yes' or 'No'.\n\nQuery: {}\n\nDocument: {}",
            query, document
        );

        let request = ChatRequest {
            model: self.model.clone(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: prompt,
            }],
            stream: false,
            options: ChatOptions { temperature: 0.0 },
        };

        let url = format!("{}/api/chat", self.api_url);

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .context("Failed to send rerank request")?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Reranking API error ({}): {}", status, error_text);
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .context("Failed to parse rerank response")?;

        // Parse response: check if it contains "yes" (case-insensitive)
        let score = if chat_response
            .message
            .content
            .to_lowercase()
            .contains("yes")
        {
            1.0
        } else {
            0.0
        };

        Ok(score)
    }

    /// Re-rank multiple documents in parallel for speed
    /// Uses tokio::spawn to parallelize requests
    pub async fn rerank_batch(&self, query: &str, documents: &[&str]) -> Result<Vec<f64>> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let mut handles = Vec::new();

        // Spawn async tasks for each document
        for (idx, doc) in documents.iter().enumerate() {
            let client = self.client.clone();
            let api_url = self.api_url.clone();
            let model = self.model.clone();
            let query = query.to_string();
            let document = doc.to_string();

            let handle = task::spawn(async move {
                let prompt = format!(
                    "You are an expert relevance grader. Is the following document relevant to the user's query? Reply 'Yes' or 'No'.\n\nQuery: {}\n\nDocument: {}",
                    query, document
                );

                let request = ChatRequest {
                    model,
                    messages: vec![ChatMessage {
                        role: "user".to_string(),
                        content: prompt,
                    }],
                    stream: false,
                    options: ChatOptions { temperature: 0.0 },
                };

                let url = format!("{}/api/chat", api_url);

                match client
                    .post(&url)
                    .json(&request)
                    .send()
                    .await
                {
                    Ok(response) if response.status().is_success() => {
                        match response.json::<ChatResponse>().await {
                            Ok(chat_response) => {
                                let score = if chat_response
                                    .message
                                    .content
                                    .to_lowercase()
                                    .contains("yes")
                                {
                                    1.0
                                } else {
                                    0.0
                                };
                                (idx, Ok::<f64, anyhow::Error>(score))
                            }
                            Err(e) => {
                                tracing::warn!("Failed to parse rerank response for doc {}: {}", idx, e);
                                (idx, Ok(0.5)) // Default to neutral score on parse error
                            }
                        }
                    }
                    Ok(response) => {
                        let status = response.status();
                        tracing::warn!("Rerank API error for doc {}: status {}", idx, status);
                        (idx, Ok(0.5)) // Default to neutral score on API error
                    }
                    Err(e) => {
                        tracing::warn!("Rerank request failed for doc {}: {}", idx, e);
                        (idx, Ok(0.5)) // Default to neutral score on request error
                    }
                }
            });

            handles.push(handle);
        }

        // Collect results maintaining order
        let mut results = vec![0.5f64; documents.len()]; // Default neutral scores

        for handle in handles {
            match handle.await {
                Ok((idx, Ok(score))) => {
                    if idx < results.len() {
                        results[idx] = score;
                    }
                }
                Ok((idx, Err(e))) => {
                    tracing::warn!("Error in parallel rerank task {}: {}", idx, e);
                    // Keep default neutral score
                }
                Err(e) => {
                    tracing::warn!("Rerank task panicked: {}", e);
                    // Keep default neutral scores
                }
            }
        }

        Ok(results)
    }

    /// Re-rank and return documents sorted by relevance (highest first)
    pub async fn rerank_and_sort(&self, query: &str, documents: &[&str]) -> Result<Vec<RankedDocument>> {
        let scores = self.rerank_batch(query, documents).await?;

        let mut ranked: Vec<RankedDocument> = scores
            .into_iter()
            .enumerate()
            .map(|(index, score)| RankedDocument { index, score })
            .collect();

        // Sort by score descending (highest relevance first)
        ranked.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(ranked)
    }
}
