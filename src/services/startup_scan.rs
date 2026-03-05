//! Startup knowledge base scanner
//!
//! Automatically scans configured local paths, URLs, and Chrome bookmarks
//! at startup and queues them for import with idempotency checks.

use anyhow::Result;
use sqlx::PgPool;
use uuid::Uuid;
use walkdir::WalkDir;

use crate::config::{KnowledgeBaseConfig, Settings};
use crate::infra::embedder::Embedder;
use crate::services::bookmark_parser;

/// Convenience function for startup scanning
pub async fn scan_and_import(
    pool: &PgPool,
    _embedder: &Embedder,
    import_queue: &tokio::sync::mpsc::Sender<Uuid>,
    settings: &Settings,
) -> Result<()> {
    let scanner = StartupScanner::new(
        pool.clone(),
        settings.knowledge_base.clone(),
        import_queue.clone(),
    );
    scanner.run().await
}

/// Handles startup scanning of knowledge base sources
pub struct StartupScanner {
    pool: PgPool,
    config: KnowledgeBaseConfig,
    import_queue: tokio::sync::mpsc::Sender<Uuid>,
}

impl StartupScanner {
    /// Create a new startup scanner
    pub fn new(
        pool: PgPool,
        config: KnowledgeBaseConfig,
        import_queue: tokio::sync::mpsc::Sender<Uuid>,
    ) -> Self {
        Self {
            pool,
            config,
            import_queue,
        }
    }

    /// Run the startup scan
    ///
    /// This performs the following steps:
    /// 1. Discovers files in each configured local path
    /// 2. Adds configured URLs
    /// 3. Parses Chrome bookmarks if configured
    /// 4. Filters out already-indexed paths (idempotency)
    /// 5. Creates and queues a single import job
    pub async fn run(&self) -> Result<()> {
        tracing::info!("Starting knowledge base scan");
        tracing::info!("Configured local_paths: {:?}", self.config.local_paths);
        tracing::info!("Configured urls: {:?}", self.config.urls);

        let mut sources = Vec::new();

        // 1. Discover files from local paths
        for local_path in &self.config.local_paths {
            match self.discover_local_files(local_path).await {
                Ok(files) => {
                    tracing::info!("Discovered {} files from {}", files.len(), local_path);
                    sources.extend(files);
                }
                Err(e) => {
                    tracing::warn!("Failed to scan {}: {}", local_path, e);
                }
            }
        }

        // 2. Add configured URLs
        sources.extend(self.config.urls.clone());

        // 3. Parse Chrome bookmarks if configured
        if let Some(bookmarks_path) = &self.config.chrome_bookmarks_path {
            if !bookmarks_path.is_empty() {
                match bookmark_parser::parse_chrome_bookmarks(bookmarks_path) {
                    Ok(urls) => {
                        tracing::info!("Parsed {} URLs from Chrome bookmarks", urls.len());
                        sources.extend(urls);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Failed to parse Chrome bookmarks from {}: {}",
                            bookmarks_path,
                            e
                        );
                    }
                }
            }
        }

        if sources.is_empty() {
            tracing::info!("No sources configured or discovered");
            return Ok(());
        }

        tracing::info!("Total sources discovered: {}", sources.len());

        // 4. Filter out already-indexed paths (idempotency)
        let filtered_sources = self.filter_indexed_sources(sources).await?;

        if filtered_sources.is_empty() {
            tracing::info!("All sources already indexed, skipping import");
            return Ok(());
        }

        tracing::info!(
            "Filtered to {} new sources (some already indexed)",
            filtered_sources.len()
        );

        // 5. Create and queue import job
        self.create_and_queue_job(filtered_sources).await?;

        Ok(())
    }

    /// Discover indexable files in a local path
    async fn discover_local_files(&self, local_path: &str) -> Result<Vec<String>> {
        let mut files = Vec::new();
        let extensions = self
            .config
            .file_extensions
            .clone()
            .unwrap_or_else(|| vec!["md".to_string(), "pdf".to_string()]);

        for entry in WalkDir::new(local_path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
        {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                if let Some(ext_str) = ext.to_str() {
                    if extensions.contains(&ext_str.to_lowercase()) {
                        files.push(path.to_string_lossy().to_string());
                    }
                }
            }
        }

        Ok(files)
    }

    /// Filter out sources that have already been indexed
    ///
    /// Checks both the documents table (by source_path) and import_items table
    /// (for pending/processing items)
    async fn filter_indexed_sources(&self, sources: Vec<String>) -> Result<Vec<String>> {
        if sources.is_empty() {
            return Ok(Vec::new());
        }

        // Batch query: Get all indexed sources in one query
        let indexed_docs: Vec<(String,)> =
            sqlx::query_as("SELECT source_path FROM documents WHERE source_path = ANY($1)")
                .bind(&sources)
                .fetch_all(&self.pool)
                .await?;

        let indexed_set: std::collections::HashSet<String> =
            indexed_docs.into_iter().map(|(path,)| path).collect();

        // Batch query: Get all pending/processing import items in one query
        let pending_items: Vec<(String,)> = sqlx::query_as(
            "SELECT source_path FROM import_items WHERE source_path = ANY($1) AND status IN ('pending', 'processing')"
        )
        .bind(&sources)
        .fetch_all(&self.pool)
        .await?;

        let pending_set: std::collections::HashSet<String> =
            pending_items.into_iter().map(|(path,)| path).collect();

        // Filter out sources that are already indexed or pending
        let filtered: Vec<String> = sources
            .into_iter()
            .filter(|source| {
                let skip = indexed_set.contains(source) || pending_set.contains(source);
                if skip {
                    tracing::debug!("Skipping already-indexed or pending: {}", source);
                }
                !skip
            })
            .collect();

        Ok(filtered)
    }

    /// Create an import job and queue it for processing
    async fn create_and_queue_job(&self, sources: Vec<String>) -> Result<()> {
        use crate::services::import::ImportJobRunner;

        let settings = crate::config::Settings::new()?;
        let runner = ImportJobRunner::new(settings.import.clone());

        // Create job
        let job_id = runner.create_job(&self.pool, "startup_scan", None).await?;

        // Create import items for each source
        let source_refs: Vec<&str> = sources.iter().map(|s| s.as_str()).collect();
        let item_ids = crate::services::import::ImportItemManager
            .create_items(&self.pool, job_id, source_refs)
            .await?;

        tracing::info!(
            "Created import job {} with {} items",
            job_id,
            item_ids.len()
        );

        // Queue job for processing
        self.import_queue.send(job_id).await?;

        tracing::info!("Queued startup scan job: {}", job_id);

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    #[ignore]
    async fn test_discover_local_files() {
        let temp_dir = TempDir::new().unwrap();
        let test_dir = temp_dir.path().join("test");
        fs::create_dir(&test_dir).unwrap();

        // Create some test files
        fs::write(test_dir.join("doc1.md"), "# Document 1").unwrap();
        fs::write(test_dir.join("doc2.pdf"), "PDF content").unwrap();
        fs::write(test_dir.join("ignore.txt"), "Text file").unwrap();

        let config = KnowledgeBaseConfig {
            local_paths: vec![test_dir.to_string_lossy().to_string()],
            urls: vec![],
            chrome_bookmarks_path: None,
            file_extensions: Some(vec!["md".to_string(), "pdf".to_string()]),
            scan_on_startup: true,
        };

        let (tx, _rx) = tokio::sync::mpsc::channel(100);

        // We can't easily test this without a real database, but we can test the discovery logic
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(1)
            .connect("postgres://rag_user:rag_password@localhost:15432/rag_chat")
            .await
            .unwrap_or_else(|_| panic!("Failed to connect to test database"));

        let scanner = StartupScanner::new(pool, config, tx);

        let files = scanner
            .discover_local_files(test_dir.to_str().unwrap())
            .await
            .unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.ends_with(".md")));
        assert!(files.iter().any(|f| f.ends_with(".pdf")));
    }

    #[test]
    fn test_startup_scanner_config() {
        let config = KnowledgeBaseConfig {
            local_paths: vec!["/data/books".to_string()],
            urls: vec!["https://example.com".to_string()],
            chrome_bookmarks_path: Some("/path/to/bookmarks.json".to_string()),
            file_extensions: Some(vec!["md".to_string(), "pdf".to_string()]),
            scan_on_startup: true,
        };

        assert_eq!(config.local_paths.len(), 1);
        assert_eq!(config.urls.len(), 1);
        assert!(config.chrome_bookmarks_path.is_some());
        assert!(config.scan_on_startup);
    }
}
