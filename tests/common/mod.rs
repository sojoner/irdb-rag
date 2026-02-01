use reqwest::Client;
use std::time::Duration;

/// Get the base URL for tests. Uses TEST_API_URL env var, or derives from RUN_ENV config.
fn get_base_url() -> String {
    // First check for explicit TEST_API_URL
    if let Ok(url) = std::env::var("TEST_API_URL") {
        return url;
    }

    // Check RUN_ENV to determine host
    match std::env::var("RUN_ENV").as_deref() {
        Ok("local-bm3090") => "http://bm3090:3000/api".to_string(),
        Ok("test-gpu") => "http://bm3090:3000/api".to_string(),
        _ => "http://localhost:3000/api".to_string(),
    }
}

pub struct TestClient {
    pub client: Client,
    pub base_url: String,
}

impl TestClient {
    pub fn new() -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap();

        Self {
            client,
            base_url: get_base_url(),
        }
    }

    pub async fn is_server_running(&self) -> bool {
        let health_url = format!("{}/health", self.base_url);
        match self.client.get(&health_url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    pub async fn ensure_server_running(&self) {
        if !self.is_server_running().await {
            panic!("Server is not running on {}. Please start it with 'cargo run' in a separate terminal.", self.base_url);
        }
    }

    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}
