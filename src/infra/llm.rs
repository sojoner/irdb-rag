//! LLM Integration Module
//!
//! Handles communication with OpenAI-compatible LLM APIs using async-openai.

use anyhow::Result;
use async_openai::types::{
    ChatCompletionRequestMessage, ChatCompletionRequestSystemMessage,
    ChatCompletionRequestUserMessage, CreateChatCompletionRequestArgs,
};
use async_openai::{config::OpenAIConfig, Client};
use futures::stream::BoxStream;

use crate::domain::models::{InfraEmbeddingConfig, LLMConfig};

/// Create an OpenAI client from LLM config
fn create_client(config: &LLMConfig) -> Client<OpenAIConfig> {
    let mut openai_config = OpenAIConfig::new().with_api_base(&config.api_url);

    // Only set API key if it's not empty
    if !config.api_key.is_empty() {
        openai_config = openai_config.with_api_key(&config.api_key);
    }

    Client::with_config(openai_config)
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
    call_llm_with_timeout(config, system, user, max_tokens, temperature, 300).await
}

/// Call the LLM API with custom timeout
pub async fn call_llm_with_timeout(
    config: &LLMConfig,
    system: &str,
    user: &str,
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    timeout_seconds: u64,
) -> Result<String> {
    let mut openai_config = OpenAIConfig::new().with_api_base(&config.api_url);

    // Only set API key if it's not empty
    if !config.api_key.is_empty() {
        openai_config = openai_config.with_api_key(&config.api_key);
    }

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(timeout_seconds))
        .build()?;

    let client = Client::with_config(openai_config).with_http_client(http_client);

    let messages = vec![
        ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
            content: system.to_string().into(),
            ..Default::default()
        }),
        ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: user.to_string().into(),
            ..Default::default()
        }),
    ];

    let request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(messages)
        .max_tokens(max_tokens.unwrap_or(2048))
        .temperature(temperature.unwrap_or(0.7))
        .build()?;

    let response = client.chat().create(request).await?;

    let content = response
        .choices
        .first()
        .and_then(|choice| choice.message.content.as_ref())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "No response".to_string());

    Ok(content)
}

/// Stream LLM response using SSE
pub async fn stream_llm(
    config: &LLMConfig,
    system: &str,
    user: &str,
) -> Result<BoxStream<'static, Result<String>>> {
    stream_llm_with_timeout(config, system, user, 300).await
}

/// Stream LLM response using SSE with custom timeout
pub async fn stream_llm_with_timeout(
    config: &LLMConfig,
    system: &str,
    user: &str,
    _timeout_seconds: u64, // TODO: async-openai doesn't expose timeout config easily
) -> Result<BoxStream<'static, Result<String>>> {
    use futures::stream::StreamExt;

    let client = create_client(config);

    let messages = vec![
        ChatCompletionRequestMessage::System(ChatCompletionRequestSystemMessage {
            content: system.to_string().into(),
            ..Default::default()
        }),
        ChatCompletionRequestMessage::User(ChatCompletionRequestUserMessage {
            content: user.to_string().into(),
            ..Default::default()
        }),
    ];

    let request = CreateChatCompletionRequestArgs::default()
        .model(&config.model)
        .messages(messages)
        .max_tokens(2048_u32)
        .temperature(0.7)
        .build()?;

    let mut stream = client.chat().create_stream(request).await?;

    // Transform the async-openai stream into our Result<String> stream
    let content_stream = async_stream::stream! {
        while let Some(result) = stream.next().await {
            match result {
                Ok(response) => {
                    // Extract content delta from the response
                    if let Some(choice) = response.choices.first() {
                        if let Some(content) = &choice.delta.content {
                            if !content.is_empty() {
                                yield Ok(content.clone());
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
    }
    .boxed();

    Ok(content_stream)
}

/// Generate embeddings for a text string
#[allow(dead_code)]
pub async fn get_embedding(config: &InfraEmbeddingConfig, text: &str) -> Result<Vec<f32>> {
    let embeddings = get_embeddings_batch(config, &[text]).await?;
    Ok(embeddings.into_iter().next().unwrap_or_default())
}

/// Generate embeddings for multiple text strings (batch operation)
#[allow(dead_code)]
pub async fn get_embeddings_batch(
    config: &InfraEmbeddingConfig,
    texts: &[&str],
) -> Result<Vec<Vec<f32>>> {
    get_embeddings_batch_with_timeout(config, texts, 120).await
}

/// Generate embeddings for multiple text strings with custom timeout
#[allow(dead_code)]
pub async fn get_embeddings_batch_with_timeout(
    config: &InfraEmbeddingConfig,
    texts: &[&str],
    timeout_seconds: u64,
) -> Result<Vec<Vec<f32>>> {
    use std::time::Duration;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(timeout_seconds))
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
            .ok_or_else(|| anyhow::anyhow!("Missing index in embedding response"))?
            as usize;

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
