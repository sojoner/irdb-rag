use axum::extract::FromRef;
use leptos::prelude::LeptosOptions;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, RwLock};
use uuid::Uuid;

use crate::config::Settings;
use crate::domain::models::LLMConfig;
use crate::infra::embedder::Embedder;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub embedder: Arc<Embedder>,
    pub llm_config: Arc<RwLock<LLMConfig>>,
    pub log_buffer: Arc<Mutex<Vec<String>>>,
    pub leptos_options: LeptosOptions,
    pub import_job_queue: mpsc::Sender<Uuid>,
    pub settings: Arc<Settings>,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        embedder: Embedder,
        log_buffer: Arc<Mutex<Vec<String>>>,
        leptos_options: LeptosOptions,
        import_job_queue: mpsc::Sender<Uuid>,
        settings: Arc<Settings>,
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
        }
    }
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}
