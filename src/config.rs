//! Application configuration module
//!
//! Handles loading and validating application settings from TOML files
//! and environment variables using the `config` crate.
//!
//! Configuration hierarchy (highest to lowest priority):
//! 1. Environment variables (e.g., APP_DATABASE__URL)
//! 2. Environment-specific file (test.toml, production.toml)
//! 3. default.toml

use serde::Deserialize;
use config as config_crate;
use config_crate::{Config, ConfigError, Environment, File};

/// Complete application settings
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub llm: LLMConfigSet,
    pub embedding: EmbeddingConfig,
    pub docling: DoclingConfig,
    pub import: ImportConfig,
    pub rag: RagConfig,
}

/// Server configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub log_level: String,
}

/// Database connection pool configuration
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub acquire_timeout_seconds: u64,
}

/// LLM configuration set (multiple LLM providers for different purposes)
#[derive(Debug, Deserialize, Clone)]
pub struct LLMConfigSet {
    pub chat: LLMProviderConfig,
    pub metadata: LLMProviderConfig,
    pub ner: LLMProviderConfig,
    pub timeout_seconds: u64,
}

/// Single LLM provider configuration
#[derive(Debug, Deserialize, Clone)]
pub struct LLMProviderConfig {
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
}

/// Embedding service configuration
#[derive(Debug, Deserialize, Clone)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub dimensions: u32,
    pub timeout_seconds: u64,
}

/// Docling document processing service configuration
#[derive(Debug, Deserialize, Clone)]
pub struct DoclingConfig {
    pub url: String,
    pub timeout_seconds: u64,
    pub models: Option<String>,
}

/// Import job configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ImportConfig {
    pub workers: usize,
    pub max_retries: u32,
    pub retry_base_delay_ms: u64,
    pub retry_max_delay_ms: u64,
}

/// RAG system configuration
#[derive(Debug, Deserialize, Clone)]
pub struct RagConfig {
    pub system_prompt: Option<String>,
    pub entity_extraction_enabled: bool,
}

impl Settings {
    /// Load settings from configuration files and environment variables
    ///
    /// Configuration hierarchy (highest to lowest priority):
    /// 1. Environment variables prefixed with APP_ (e.g., APP_DATABASE__URL)
    /// 2. Environment-specific TOML file based on RUN_ENV (e.g., config/test.toml)
    /// 3. Default configuration (config/default.toml)
    ///
    /// # Errors
    /// Returns ConfigError if:
    /// - Configuration files are malformed
    /// - Required configuration values are missing
    /// - Validation fails (e.g., invalid dimensions)
    pub fn new() -> Result<Self, ConfigError> {
        let run_env = std::env::var("RUN_ENV")
            .unwrap_or_else(|_| "development".into());

        let config = Config::builder()
            // Start with defaults
            .add_source(File::with_name("config/default"))

            // Layer environment-specific config
            .add_source(
                File::with_name(&format!("config/{}", run_env))
                    .required(false)
            )

            // Override with environment variables
            // APP_DATABASE__URL overrides database.url
            // APP_LLM__CHAT__API_KEY overrides llm.chat.api_key
            .add_source(
                Environment::with_prefix("APP")
                    .separator("__")
            )

            .build()?;

        let settings: Self = config.try_deserialize()?;
        settings.validate()?;
        Ok(settings)
    }

    /// Validate critical configuration values
    ///
    /// # Errors
    /// Returns ConfigError if validation fails
    fn validate(&self) -> Result<(), ConfigError> {
        if self.embedding.dimensions == 0 {
            return Err(ConfigError::Message(
                "embedding.dimensions must be > 0".into()
            ));
        }

        if self.database.url.is_empty() {
            return Err(ConfigError::Message(
                "database.url is required".into()
            ));
        }

        Ok(())
    }
}
