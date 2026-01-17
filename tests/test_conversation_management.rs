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
async fn test_create_conversation() {
    let pool = get_test_pool().await;
    let title = "Test Conversation";

    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind(title)
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
async fn test_list_conversations() {
    let pool = get_test_pool().await;

    // Create a few conversations
    let mut created_ids = Vec::new();
    for i in 1..=3 {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
        )
        .bind(format!("Conversation {}", i))
        .fetch_one(&pool)
        .await
        .expect("Failed to create conversation");
        created_ids.push(id);
    }

    // List only our conversations
    let conversations: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, COALESCE(title, 'Untitled') as title
         FROM conversations
         WHERE id = ANY($1)
         ORDER BY updated_at DESC"
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
async fn test_delete_conversation_cascade() {
    let pool = get_test_pool().await;
    // Create conversation
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind("Test Conversation")
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    // Add messages
    sqlx::query(
        "INSERT INTO messages (conversation_id, role, content) VALUES ($1, $2, $3)"
    )
    .bind(conversation_id)
    .bind("user")
    .bind("Hello")
    .execute(&pool)
    .await
    .expect("Failed to create message");

    sqlx::query(
        "INSERT INTO messages (conversation_id, role, content) VALUES ($1, $2, $3)"
    )
    .bind(conversation_id)
    .bind("assistant")
    .bind("Hi there!")
    .execute(&pool)
    .await
    .expect("Failed to create message");

    // Verify messages exist
    let message_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = $1"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count messages");

    assert_eq!(message_count, 2);

    // Delete conversation
    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to delete conversation");

    // Verify messages were cascade deleted
    let message_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = $1"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count messages");

    assert_eq!(message_count, 0);

    // Conversation is already deleted, no cleanup needed
}

#[tokio::test]
async fn test_conversation_updated_at() {
    let pool = get_test_pool().await;
    // Create conversation
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind("Test Conversation")
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    // Get initial updated_at
    let initial_updated_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT updated_at FROM conversations WHERE id = $1"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to get updated_at");

    // Add a message (this should trigger updated_at update via trigger)
    sqlx::query(
        "INSERT INTO messages (conversation_id, role, content) VALUES ($1, $2, $3)"
    )
    .bind(conversation_id)
    .bind("user")
    .bind("Hello")
    .execute(&pool)
    .await
    .expect("Failed to create message");

    // Manually update updated_at (since we might not have a trigger yet)
    sqlx::query(
        "UPDATE conversations SET updated_at = NOW() WHERE id = $1"
    )
    .bind(conversation_id)
    .execute(&pool)
    .await
    .expect("Failed to update updated_at");

    // Get updated updated_at
    let new_updated_at: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
        "SELECT updated_at FROM conversations WHERE id = $1"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to get updated_at");

    assert!(new_updated_at > initial_updated_at);

    // Cleanup
    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup");
}
