#[derive(Debug)]
pub enum DomainError {
    NotFound,
    Internal(String),
    Database(String),
    LLM(String),
    Config(String),
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DomainError::NotFound => write!(f, "Entity not found"),
            DomainError::Internal(msg) => write!(f, "Internal error: {}", msg),
            DomainError::Database(msg) => write!(f, "Database error: {}", msg),
            DomainError::LLM(msg) => write!(f, "LLM error: {}", msg),
            DomainError::Config(msg) => write!(f, "Configuration error: {}", msg),
        }
    }
}

impl std::error::Error for DomainError {}
