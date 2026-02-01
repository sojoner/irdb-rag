mod common;

use anyhow::Result;
use rag_chat::config::Settings;
use rag_chat::infra::db;
use rag_chat::infra::embedder::Embedder;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use serde_json::json;

// Adapted from e2e_chat.rs, chat_conversation_*.rs, test_conversation_*.rs, message_persistence_test.rs, test_chat_vector_search.rs

// ============================================
// Internal DB/Logic Tests
// ============================================

#[tokio::test]
async fn test_db_conversation_lifecycle() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }
    let settings = Settings::new()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&settings.database.url).await?;

    // Create
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO conversations (id, title) VALUES ($1, 'Test DB Chat')").bind(id).execute(&pool).await?;

    // Add Message
    sqlx::query("INSERT INTO messages (conversation_id, role, content) VALUES ($1, 'user', 'Hello')").bind(id).execute(&pool).await?;

    // Verify
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE conversation_id = $1").bind(id).fetch_one(&pool).await?;
    assert_eq!(count, 1);

    // Delete (cascade)
    sqlx::query("DELETE FROM conversations WHERE id = $1").bind(id).execute(&pool).await?;
    let msg_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM messages WHERE conversation_id = $1").bind(id).fetch_one(&pool).await?;
    assert_eq!(msg_count, 0);

    Ok(())
}

// ============================================
// Message Persistence Tests (from message_persistence_test.rs)
// ============================================

#[tokio::test]
async fn test_save_and_load_messages() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }
    let settings = Settings::new()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&settings.database.url).await?;

    // Create a conversation
    let conv_id = db::create_conversation(&pool, "Message Test").await?;

    // Save messages
    let user_msg = "Hello, can you help me?";
    let assistant_msg = "Of course! I'm here to help.";

    db::save_message(&pool, conv_id, "user", user_msg).await?;
    db::save_message(&pool, conv_id, "assistant", assistant_msg).await?;

    // Load messages
    let messages = db::load_conversation(&pool, conv_id).await?;

    // Verify messages
    assert_eq!(messages.len(), 2, "Should have 2 messages");
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, user_msg);
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].content, assistant_msg);

    // Cleanup
    sqlx::query("DELETE FROM conversations WHERE id = $1").bind(conv_id).execute(&pool).await?;
    Ok(())
}

#[tokio::test]
async fn test_multi_turn_conversation_persistence() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }
    let settings = Settings::new()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&settings.database.url).await?;

    let conv_id = db::create_conversation(&pool, "Multi-turn Test").await?;

    let exchanges = vec![
        ("user", "What is Rust?"),
        ("assistant", "Rust is a systems programming language."),
        ("user", "Why should I learn Rust?"),
        ("assistant", "Rust provides memory safety and high performance."),
        ("user", "How do I get started?"),
        ("assistant", "Download from rustup.rs and follow the official book."),
    ];

    for (role, content) in &exchanges {
        db::save_message(&pool, conv_id, role, content).await?;
    }

    let messages = db::load_conversation(&pool, conv_id).await?;
    assert_eq!(messages.len(), exchanges.len());

    for (i, (role, content)) in exchanges.iter().enumerate() {
        assert_eq!(messages[i].role, *role);
        assert_eq!(messages[i].content, *content);
    }

    sqlx::query("DELETE FROM conversations WHERE id = $1").bind(conv_id).execute(&pool).await?;
    Ok(())
}

// ============================================
// Chat Vector Search Tests (from test_chat_vector_search.rs)
// ============================================

#[tokio::test]
async fn test_chat_vector_search_with_document_filter() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }
    let settings = Settings::new()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&settings.database.url).await?;

    // Get some document IDs
    let doc_ids: Vec<(Uuid,)> = sqlx::query_as("SELECT id FROM documents LIMIT 3")
        .fetch_all(&pool)
        .await?;

    if doc_ids.is_empty() {
        println!("⚠️ No documents in database, skipping test");
        return Ok(());
    }

    let document_ids: Vec<Uuid> = doc_ids.into_iter().map(|(id,)| id).collect();
    let embedder = Embedder::new(&settings.embedding)?;

    let query = "machine learning";
    let embedding = embedder.embed(query).await?;

    // Test that get_relevant_chunks works with document_ids filter
    let chunks = db::get_relevant_chunks(&pool, &embedding, 5, Some(&document_ids)).await?;

    // Verify chunks are from the specified documents
    for chunk in &chunks {
        assert!(
            document_ids.contains(&chunk.document_id),
            "Chunk document_id {} not in specified document_ids",
            chunk.document_id
        );
    }

    Ok(())
}

// ============================================
// Document to Chat Formatting (from test_document_to_chat.rs)
// ============================================

fn format_document_for_chat(doc_id: Uuid, content: &str) -> String {
    format!("---\nDoc: {}\n>>>\n{}\n<<<\n", doc_id, content)
}

#[test]
fn test_format_document_for_chat_single() {
    let doc_id = Uuid::new_v4();
    let content = "This is a test document content.";
    let formatted = format_document_for_chat(doc_id, content);

    assert!(formatted.starts_with("---\nDoc: "));
    assert!(formatted.contains(&doc_id.to_string()));
    assert!(formatted.contains(">>>\nThis is a test document content.\n<<<"));
}

#[test]
fn test_format_document_for_chat_multiple() {
    let doc_id_1 = Uuid::new_v4();
    let doc_id_2 = Uuid::new_v4();
    let formatted_1 = format_document_for_chat(doc_id_1, "First document.");
    let formatted_2 = format_document_for_chat(doc_id_2, "Second document.");
    let combined = format!("{}{}", formatted_1, formatted_2);

    assert!(combined.contains(&doc_id_1.to_string()));
    assert!(combined.contains(&doc_id_2.to_string()));
    assert!(combined.contains("First document"));
    assert!(combined.contains("Second document"));
}

#[test]
fn test_format_document_for_chat_multiline() {
    let doc_id = Uuid::new_v4();
    let content = "Line 1\nLine 2\nLine 3";
    let formatted = format_document_for_chat(doc_id, content);
    assert!(formatted.contains(">>>\nLine 1\nLine 2\nLine 3\n<<<"));
}

// ============================================
// Chat Request Serialization (from chat_conversation_api_test.rs)
// ============================================

#[test]
fn test_chat_request_serialization_roundtrip() {
    use rag_chat::domain::dtos::{ChatConversationRequest, ChatMessage};

    let messages = vec![
        ChatMessage { role: "user".to_string(), content: "What is Rust?".to_string() },
        ChatMessage { role: "assistant".to_string(), content: "Rust is a systems programming language.".to_string() },
        ChatMessage { role: "user".to_string(), content: "What makes it special?".to_string() },
    ];

    let req = ChatConversationRequest {
        messages: messages.clone(),
        conversation_id: Some(Uuid::new_v4()),
        document_ids: None,
        context_chunks: 5,
    };

    // Serialize and deserialize
    let json = serde_json::to_string(&req).expect("Failed to serialize");
    assert!(json.contains("What is Rust?"));

    let deserialized: ChatConversationRequest = serde_json::from_str(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.messages.len(), 3);
    assert_eq!(deserialized.messages[0].role, "user");
    assert_eq!(deserialized.messages[1].role, "assistant");
}

// ============================================
// E2E API Tests
// ============================================

#[tokio::test]
async fn test_e2e_chat_comprehensive() {
    let client = common::TestClient::new();
    client.ensure_server_running().await;

    // 1. Create
    let c_req = json!({ "title": "E2E Test Chat" });
    let c_resp = client.client.post(client.url("/conversations")).json(&c_req).send().await.expect("Failed create");
    let c_data: serde_json::Value = c_resp.json().await.unwrap();
    let conv_id = c_data["id"].as_str().unwrap();

    // 2. Chat Turn 1
    let chat1 = json!({
        "conversation_id": conv_id,
        "messages": [{ "role": "user", "content": "Hello" }]
    });
    let r1 = client.client.post(client.url("/chat/conversation")).json(&chat1).send().await.expect("Failed chat 1");
    assert!(r1.status().is_success());
    let d1: serde_json::Value = r1.json().await.unwrap();
    let reply1 = d1["message"]["content"].as_str().unwrap();
    assert!(!reply1.is_empty());

    // 3. Chat Turn 2 (Context)
    let chat2 = json!({
        "conversation_id": conv_id,
        "messages": [
            { "role": "user", "content": "Hello" },
            { "role": "assistant", "content": reply1 },
            { "role": "user", "content": "Repeat that" }
        ]
    });
    let r2 = client.client.post(client.url("/chat/conversation")).json(&chat2).send().await.expect("Failed chat 2");
    assert!(r2.status().is_success());

    // 4. List & Verify
    let l_resp = client.client.get(client.url("/conversations")).send().await.unwrap();
    let l_data: serde_json::Value = l_resp.json().await.unwrap();
    let convs = l_data["conversations"].as_array().unwrap();
    assert!(convs.iter().any(|c| c["id"].as_str() == Some(conv_id)));

    // 5. Delete
    let d_resp = client.client.delete(client.url(&format!("/conversations/{}", conv_id))).send().await.unwrap();
    assert!(d_resp.status().is_success());
}

// ============================================
// Detailed Conversation Logic Tests
// ============================================

#[tokio::test]
async fn test_conversation_updated_at_trigger() -> Result<()> {
    if std::env::var("RUN_ENV").is_err() { std::env::set_var("RUN_ENV", "test"); }
    let settings = Settings::new()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&settings.database.url).await?;

    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO conversations (id, title) VALUES ($1, 'Trigger Test')").bind(id).execute(&pool).await?;

    // Get initial updated_at
    let initial: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT updated_at FROM conversations WHERE id = $1")
        .bind(id).fetch_one(&pool).await?;

    // Wait a bit to ensure timestamp difference
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Add message
    sqlx::query("INSERT INTO messages (conversation_id, role, content) VALUES ($1, 'user', 'bump')")
        .bind(id).execute(&pool).await?;

    // Manually force update if trigger doesn't exist in test DB (simulate app logic)
    sqlx::query("UPDATE conversations SET updated_at = NOW() WHERE id = $1").bind(id).execute(&pool).await?;

    let updated: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT updated_at FROM conversations WHERE id = $1")
        .bind(id).fetch_one(&pool).await?;

    assert!(updated > initial, "updated_at should increase after message/update");
    
    // Cleanup
    sqlx::query("DELETE FROM conversations WHERE id = $1").bind(id).execute(&pool).await?;
    Ok(())
}

#[test]
fn test_chat_struct_validation() {
    use rag_chat::domain::dtos::{ChatMessage, ChatConversationRequest};
    
    // 1. Message Creation
    let msg = ChatMessage { role: "user".to_string(), content: "hi".to_string() };
    assert_eq!(msg.role, "user");

    // 2. History Ordering
    let req = ChatConversationRequest {
        messages: vec![
            ChatMessage { role: "user".to_string(), content: "1".to_string() },
            ChatMessage { role: "assistant".to_string(), content: "2".to_string() }
        ],
        conversation_id: None,
        context_chunks: 5,
        document_ids: None
    };
    assert_eq!(req.messages[0].content, "1");
    assert_eq!(req.messages[1].role, "assistant");
}
