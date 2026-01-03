//! LLM Integration Module
//! 
//! Handles communication with OpenAI-compatible LLM APIs.

use anyhow::Result;

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LLMConfig {
    #[allow(dead_code)]
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
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
            provider: std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "openai".to_string()),
            api_url: std::env::var("LLM_API_URL").unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),
            api_key: std::env::var("LLM_API_KEY").unwrap_or_default(),
            // Use entity extraction model if available, otherwise fall back to main LLM
            model: std::env::var("ENTITY_EXTRACTION_MODEL")
                .unwrap_or_else(|_| std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4".to_string())),
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
