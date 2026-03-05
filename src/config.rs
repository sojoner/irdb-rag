//! Application configuration module
//!
//! Handles loading and validating application settings from TOML files
//! and environment variables using the `config` crate.
//!
//! Configuration hierarchy (highest to lowest priority):
//! 1. Environment variables (e.g., APP_DATABASE__URL)
//! 2. Environment-specific file (test.toml, production.toml)
//! 3. default.toml

use config as config_crate;
use config_crate::{Config, ConfigError, Environment, File};
use serde::Deserialize;

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
    #[serde(default)]
    pub knowledge_base: KnowledgeBaseConfig,
    #[serde(default)]
    pub reranking: RerankerConfig,
    #[serde(default)]
    pub enrichment: EnrichmentConfig,
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

    /// Performance optimization: disable table structure extraction (faster for text-heavy docs)
    #[serde(default = "default_true")]
    pub do_table_structure: bool,

    /// Performance optimization: disable layout analysis (faster for simple documents)
    #[serde(default = "default_true")]
    pub do_layout_analysis: bool,

    /// Performance optimization: disable OCR (only for native text PDFs, much faster)
    #[serde(default = "default_true")]
    pub do_ocr: bool,

    /// Maximum number of chunks to embed in a single batch request
    #[serde(default = "default_64")]
    pub batch_embedding_limit: usize,
}

/// Enrichment service configuration
#[derive(Debug, Deserialize, Clone)]
pub struct EnrichmentConfig {
    /// Enable/disable enrichment service (metadata, summaries, entities, categories)
    /// When disabled, documents will still be indexed but without LLM-based enrichment
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Enable/disable entity extraction (slower, requires NER model)
    #[serde(default = "default_true")]
    pub extract_entities: bool,

    /// Enable/disable summary generation
    #[serde(default = "default_true")]
    pub extract_summary: bool,

    /// Enable/disable keyword extraction
    #[serde(default = "default_true")]
    pub extract_keywords: bool,

    /// Enable/disable document category classification
    #[serde(default = "default_true")]
    pub classify_category: bool,
}

/// Import job configuration
#[derive(Debug, Deserialize, Clone)]
pub struct ImportConfig {
    /// Number of worker threads consuming the job queue
    pub workers: usize,

    /// Number of documents to process in parallel during batch indexing
    #[serde(default = "default_4")]
    pub indexing_batch_size: usize,

    /// Maximum concurrent documents being processed (FIFO queue depth)
    #[serde(default = "default_32")]
    pub max_concurrent_documents: usize,

    /// Maximum parallel entity extraction batches per document
    #[serde(default = "default_16")]
    pub entity_extraction_batches: usize,

    /// Target token size for text chunks (text-splitter creates ~22% of target size)
    #[serde(default = "default_512")]
    pub chunk_size_tokens: usize,

    /// Maximum retries for transient errors
    pub max_retries: u32,

    /// Base delay for exponential backoff (milliseconds)
    pub retry_base_delay_ms: u64,

    /// Maximum delay cap for retries (milliseconds)
    pub retry_max_delay_ms: u64,

    /// Cleanup configuration
    #[serde(default)]
    pub cleanup: JobCleanupConfig,
}

/// Job cleanup configuration
#[derive(Debug, Deserialize, Clone)]
pub struct JobCleanupConfig {
    /// Enable automatic cleanup (default: true)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Retention period in hours (default: 24)
    #[serde(default = "default_24")]
    pub retention_hours: u64,

    /// Cleanup check interval in hours (default: 1)
    #[serde(default = "default_1")]
    pub check_interval_hours: u64,

    /// Cleanup interval in seconds (for background task)
    #[serde(default = "default_3600")]
    pub interval_seconds: u64,
}

impl Default for JobCleanupConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retention_hours: 24,
            check_interval_hours: 1,
            interval_seconds: 3600,
        }
    }
}

/// Knowledge base configuration for initial import
#[derive(Debug, Deserialize, Clone, Default)]
pub struct KnowledgeBaseConfig {
    /// Local file paths to scan for documents (.md, .pdf, etc.)
    #[serde(default)]
    pub local_paths: Vec<String>,

    /// URLs to index directly
    #[serde(default)]
    pub urls: Vec<String>,

    /// Path to Chrome JSON bookmark file (optional)
    #[serde(default)]
    pub chrome_bookmarks_path: Option<String>,

    /// File extensions to index (default: md, pdf)
    #[serde(default)]
    pub file_extensions: Option<Vec<String>>,

    /// Whether to perform initial scan on startup (default: true)
    #[serde(default = "default_true")]
    pub scan_on_startup: bool,
}

fn default_true() -> bool {
    true
}

fn default_4() -> usize {
    4
}

fn default_32() -> usize {
    32
}

fn default_16() -> usize {
    16
}

fn default_24() -> u64 {
    24
}

fn default_1() -> u64 {
    1
}

fn default_3600() -> u64 {
    3600
}

fn default_10() -> usize {
    10
}

fn default_8() -> usize {
    8
}

fn default_64() -> usize {
    64
}

fn default_512() -> usize {
    512
}

/// RAG system configuration
#[derive(Debug, Deserialize, Clone)]
pub struct RagConfig {
    /// System prompt for RAG-based chat (when document context is provided)
    pub system_prompt: Option<String>,

    /// System prompt for standalone chat (no document context)
    pub chat_system_prompt: Option<String>,

    pub entity_extraction_enabled: bool,
}

/// Re-ranking service configuration
#[derive(Debug, Deserialize, Clone)]
pub struct RerankerConfig {
    #[serde(default)]
    pub enabled: bool,
    pub api_url: String,
    pub api_key: String,
    pub model: String,
    pub timeout_seconds: u64,
    #[serde(default = "default_10")]
    pub max_search_results: usize,
    #[serde(default = "default_8")]
    pub max_chat_chunks: usize,
}

impl Default for RerankerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_url: "http://localhost:11434".to_string(),
            api_key: String::new(),
            model: "dengcao/Qwen3-Reranker-0.6B:Q5_K_M".to_string(),
            timeout_seconds: 30,
            max_search_results: 10,
            max_chat_chunks: 8,
        }
    }
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            extract_entities: true,
            extract_summary: true,
            extract_keywords: true,
            classify_category: true,
        }
    }
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
        let run_env = std::env::var("RUN_ENV").unwrap_or_else(|_| "development".into());

        let config = Config::builder()
            // Start with defaults
            .add_source(File::with_name("config/default"))
            // Layer environment-specific config
            .add_source(File::with_name(&format!("config/{}", run_env)).required(false))
            // Override with environment variables
            // APP_DATABASE__URL overrides database.url
            // APP_LLM__CHAT__API_KEY overrides llm.chat.api_key
            // Arrays can be specified with comma separator:
            // APP_KNOWLEDGE_BASE__LOCAL_PATHS="/app/books,/app/docs"
            // APP_KNOWLEDGE_BASE__URLS="https://example.com,https://example2.com"
            .add_source(
                Environment::with_prefix("APP")
                    .separator("__")
                    .list_separator(",")
                    .with_list_parse_key("knowledge_base.local_paths")
                    .with_list_parse_key("knowledge_base.urls"),
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
                "embedding.dimensions must be > 0".into(),
            ));
        }

        if self.database.url.is_empty() {
            return Err(ConfigError::Message("database.url is required".into()));
        }

        Ok(())
    }
}
