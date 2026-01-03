use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use leptos::prelude::LeptosOptions;
use axum::extract::FromRef;

use crate::domain::models::LLMConfig;
use crate::infra::embedder::Embedder;

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub embedder: Arc<Embedder>,
    pub llm_config: Arc<RwLock<LLMConfig>>,
    pub log_buffer: Arc<Mutex<Vec<String>>>,
    pub leptos_options: LeptosOptions,
}

impl AppState {
    pub fn new(
        pool: PgPool,
        embedder: Embedder,
        log_buffer: Arc<Mutex<Vec<String>>>,
        leptos_options: LeptosOptions,
    ) -> Self {
        Self {
            pool,
            embedder: Arc::new(embedder),
            llm_config: Arc::new(RwLock::new(LLMConfig::from_env())),
            log_buffer,
            leptos_options,
        }
    }
}

impl FromRef<AppState> for LeptosOptions {
    fn from_ref(state: &AppState) -> Self {
        state.leptos_options.clone()
    }
}
