use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use tokio::sync::{RwLock, mpsc};
use leptos::prelude::LeptosOptions;
use axum::extract::FromRef;
use uuid::Uuid;

use crate::config::Settings;
use crate::domain::models::LLMConfig;
use crate::infra::embedder::Embedder;
use crate::infra::reranker::Reranker;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub embedder: Arc<Embedder>,
    pub llm_config: Arc<RwLock<LLMConfig>>,
    pub log_buffer: Arc<Mutex<Vec<String>>>,
    pub leptos_options: LeptosOptions,
    pub import_job_queue: mpsc::Sender<Uuid>,
    pub settings: Arc<Settings>,
    pub reranker: Option<Arc<Reranker>>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        embedder: Embedder,
        log_buffer: Arc<Mutex<Vec<String>>>,
        leptos_options: LeptosOptions,
        import_job_queue: mpsc::Sender<Uuid>,
        settings: Arc<Settings>,
        reranker: Option<Arc<Reranker>>,
    ) -> Self {
        let llm_config = LLMConfig::from_provider_config(&settings.llm.chat);
        Self {
            pool,
            embedder: Arc::new(embedder),
            llm_config: Arc::new(RwLock::new(llm_config)),
            log_buffer,
            leptos_options,
            import_job_queue,
            settings,
            reranker,
        }
    }
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}
