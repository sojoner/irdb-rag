use rag_chat::domain::dtos::ChatMessage;
use rag_chat::infra::db::get_pool;
use sqlx::PgPool;
use uuid::Uuid;

async fn setup_test_conversation(pool: &PgPool) -> (Uuid, Vec<ChatMessage>) {
    // Create a conversation
    let conversation_id = Uuid::new_v4();
    sqlx::query("INSERT INTO conversations (id, title) VALUES ($1, $2)")
        .bind(conversation_id)
        .bind("Test Conversation")
        .execute(pool)
        .await
        .expect("Failed to create test conversation");

    // Add test messages
    let messages = vec![
        ChatMessage {
            role: "user".to_string(),
            content: "Hello, this is a test message".to_string(),
        },
        ChatMessage {
            role: "assistant".to_string(),
            content: "This is a test response".to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: "Another user message".to_string(),
        },
    ];

    for msg in &messages {
        sqlx::query("INSERT INTO messages (conversation_id, role, content) VALUES ($1, $2, $3)")
            .bind(conversation_id)
            .bind(&msg.role)
            .bind(&msg.content)
            .execute(pool)
            .await
            .expect("Failed to insert test message");
    }

    (conversation_id, messages)
}

async fn cleanup_test_conversation(pool: &PgPool, conversation_id: Uuid) {
    sqlx::query("DELETE FROM messages WHERE conversation_id = $1")
        .bind(conversation_id)
        .execute(pool)
        .await
        .ok();

    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_conversation_messages_can_be_retrieved() {
    let pool = get_pool().await.expect("Failed to get database pool");

    let (conversation_id, expected_messages) = setup_test_conversation(&pool).await;

    // Query messages from the database
    let retrieved_messages: Vec<ChatMessage> = sqlx::query_as(
        "SELECT role, content FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .expect("Failed to fetch messages");

    // Verify we got the expected number of messages
    assert_eq!(
        retrieved_messages.len(),
        expected_messages.len(),
        "Should retrieve same number of messages as inserted"
    );

    // Verify content matches
    for (retrieved, expected) in retrieved_messages.iter().zip(expected_messages.iter()) {
        assert_eq!(retrieved.role, expected.role, "Role should match");
        assert_eq!(retrieved.content, expected.content, "Content should match");
    }

    // Cleanup
    cleanup_test_conversation(&pool, conversation_id).await;
}

#[tokio::test]
async fn test_conversation_with_multiple_messages() {
    let pool = get_pool().await.expect("Failed to get database pool");

    let conversation_id = Uuid::new_v4();
    sqlx::query("INSERT INTO conversations (id, title) VALUES ($1, $2)")
        .bind(conversation_id)
        .bind("Multi-turn Test")
        .execute(&pool)
        .await
        .expect("Failed to create conversation");

    // Add 6 messages (matching the test report's "Multi-turn Test" conversation)
    let message_pairs = vec![
        ("user", "First question"),
        ("assistant", "First response"),
        ("user", "Second question"),
        ("assistant", "Second response"),
        ("user", "Third question"),
        ("assistant", "Third response"),
    ];

    for (role, content) in &message_pairs {
        sqlx::query("INSERT INTO messages (conversation_id, role, content) VALUES ($1, $2, $3)")
            .bind(conversation_id)
            .bind(role)
            .bind(content)
            .execute(&pool)
            .await
            .expect("Failed to insert message");
    }

    // Retrieve messages
    let messages: Vec<ChatMessage> = sqlx::query_as(
        "SELECT role, content FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC",
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .expect("Failed to fetch messages");

    assert_eq!(messages.len(), 6, "Should have 6 messages");

    // Verify alternating roles
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[2].role, "user");
    assert_eq!(messages[3].role, "assistant");

    // Cleanup
    cleanup_test_conversation(&pool, conversation_id).await;
}
