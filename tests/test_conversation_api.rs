use sqlx::PgPool;
use uuid::Uuid;

async fn get_test_pool() -> PgPool {
    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://rag_user:rag_password@localhost:15432/rag_chat".to_string()
    });

    PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

#[tokio::test]
async fn test_create_conversation_api() {
    let pool = get_test_pool().await;

    // Direct DB test (handlers are tested via integration tests with full AppState)
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind("Test API Conversation")
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    assert!(!conversation_id.is_nil());

    // Cleanup
    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup");
}

#[tokio::test]
async fn test_list_conversations_api() {
    let pool = get_test_pool().await;

    // Create test conversations
    let mut created_ids = Vec::new();
    for i in 1..=3 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
        )
        .bind(format!("API Test Conversation {}", i))
        .fetch_one(&pool)
        .await
        .expect("Failed to create conversation");
        created_ids.push(id);
    }

    // List conversations
    let conversations: Vec<(Uuid, Option<String>)> = sqlx::query_as(
        "SELECT id, title FROM conversations WHERE id = ANY($1) ORDER BY updated_at DESC"
    )
    .bind(&created_ids)
    .fetch_all(&pool)
    .await
    .expect("Failed to list conversations");

    assert_eq!(conversations.len(), 3);

    // Cleanup
    for id in created_ids {
        sqlx::query("DELETE FROM conversations WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await
            .expect("Failed to cleanup");
    }
}

#[tokio::test]
async fn test_delete_conversation_api() {
    let pool = get_test_pool().await;

    // Create conversation with messages
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind("API Test Delete Conversation")
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    // Add a message
    sqlx::query(
        "INSERT INTO messages (conversation_id, role, content) VALUES ($1, $2, $3)"
    )
    .bind(conversation_id)
    .bind("user")
    .bind("Test message")
    .execute(&pool)
    .await
    .expect("Failed to create message");

    // Delete conversation
    let result = sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to delete conversation");

    assert_eq!(result.rows_affected(), 1);

    // Verify messages were cascade deleted
    let message_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = $1"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count messages");

    assert_eq!(message_count, 0);
}

#[tokio::test]
async fn test_get_conversation_with_messages_api() {
    let pool = get_test_pool().await;

    // Create conversation
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind("API Test Get Conversation")
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    // Add messages
    for (role, content) in &[("user", "Hello"), ("assistant", "Hi there!")] {
        sqlx::query(
            "INSERT INTO messages (conversation_id, role, content) VALUES ($1, $2, $3)"
        )
        .bind(conversation_id)
        .bind(role)
        .bind(content)
        .execute(&pool)
        .await
        .expect("Failed to create message");
    }

    // Get conversation with messages
    let messages: Vec<(String, String)> = sqlx::query_as(
        "SELECT role, content FROM messages WHERE conversation_id = $1 ORDER BY created_at ASC"
    )
    .bind(conversation_id)
    .fetch_all(&pool)
    .await
    .expect("Failed to get messages");

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].0, "user");
    assert_eq!(messages[0].1, "Hello");
    assert_eq!(messages[1].0, "assistant");
    assert_eq!(messages[1].1, "Hi there!");

    // Cleanup
    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup");
}
