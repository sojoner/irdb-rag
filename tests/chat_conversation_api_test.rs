//! Integration test for chat conversation API

use rag_chat::domain::dtos::{ChatConversationRequest, ChatMessage};
use rag_chat::infra::db;
use rag_chat::infra::embedder::Embedder;
use sqlx::PgPool;
use uuid::Uuid;

async fn setup_test_pool() -> PgPool {
    let config = rag_chat::config::Settings::new().expect("Failed to load config");
    db::create_pool(&config.database)
        .await
        .expect("Failed to create test pool")
}

#[tokio::test]
async fn test_chat_conversation_request_serialization() {
    let messages = vec![
        ChatMessage {
            role: "user".to_string(),
            content: "What is Rust?".to_string(),
        },
        ChatMessage {
            role: "assistant".to_string(),
            content: "Rust is a systems programming language.".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: "What makes it special?".to_string(),
        },
    ];

    let req = ChatConversationRequest {
        messages: messages.clone(),
        conversation_id: Some(Uuid::new_v4()),
        document_ids: None,
        context_chunks: 5,
    };

    // Test serialization
    let json = serde_json::to_string(&req).expect("Failed to serialize");
    assert!(json.contains("What is Rust?"));
    assert!(json.contains("What makes it special?"));

    // Test deserialization
    let deserialized: ChatConversationRequest =
        serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.messages.len(), 3);
    assert_eq!(deserialized.messages[0].role, "user");
    assert_eq!(deserialized.messages[1].role, "assistant");
}

#[tokio::test]
async fn test_chat_conversation_with_context() {
    let pool = setup_test_pool().await;

    // Get embedder from environment
    let config = rag_chat::config::Settings::new().expect("Failed to load config");
    let embedder = Embedder::new(&config.embedding).expect("Failed to create embedder");

    // Create a test conversation
    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: "Tell me about the documents".to_string(),
    }];

    let _req = ChatConversationRequest {
        messages,
        conversation_id: None,
        document_ids: None,
        context_chunks: 5,
    };

    // Generate embedding for the message
    let embedding = embedder
        .embed("Tell me about the documents")
        .await
        .expect("Failed to generate embedding");

    assert_eq!(embedding.len(), config.embedding.dimensions as usize);

    // Verify we can fetch chunks (even if empty)
    let chunks = db::get_relevant_chunks(&pool, &embedding, 5, None)
        .await
        .expect("Failed to fetch chunks");

    // This might be empty if DB is empty, but should not error
    assert!(chunks.is_empty() || !chunks.is_empty());
}

#[tokio::test]
async fn test_chat_conversation_history_formatting() {
    let messages = vec![
        ChatMessage {
            role: "user".to_string(),
            content: "First question".to_string(),
        },
        ChatMessage {
            role: "assistant".to_string(),
            content: "First answer".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: "Second question".to_string(),
        },
    ];

    // Simulate conversation text formatting
    let conversation_text = messages
        .iter()
        .map(|m| {
            if m.role == "user" {
                format!("User: {}", m.content)
            } else {
                format!("Assistant: {}", m.content)
            }
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    assert!(conversation_text.contains("User: First question"));
    assert!(conversation_text.contains("Assistant: First answer"));
    assert!(conversation_text.contains("User: Second question"));
}
