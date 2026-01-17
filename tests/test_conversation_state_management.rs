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
async fn test_conversation_list_reflects_creates() {
    let pool = get_test_pool().await;

    // Create a unique title for this test
    let test_title = format!("State Test Conversation {}", uuid::Uuid::new_v4());

    // Get initial count of conversations with this specific title
    let initial_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE title = $1")
        .bind(&test_title)
        .fetch_one(&pool)
        .await
        .expect("Failed to count conversations");

    // Create a conversation with the test title
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind(&test_title)
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    // Verify list includes new conversation
    let new_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM conversations WHERE title = $1")
        .bind(&test_title)
        .fetch_one(&pool)
        .await
        .expect("Failed to count conversations");

    assert_eq!(
        new_count,
        initial_count + 1,
        "Conversation list should reflect new conversation"
    );

    // Verify the conversation is retrievable
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = $1)"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to check existence");

    assert!(exists, "Newly created conversation should be retrievable");

    // Cleanup
    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup");
}

#[tokio::test]
async fn test_conversation_list_reflects_deletes() {
    let pool = get_test_pool().await;

    // Create a conversation to delete
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind("State Test Delete Conversation")
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    // Verify it exists
    let exists_before: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = $1)"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to check existence");

    assert!(exists_before, "Conversation should exist before delete");

    // Delete the conversation
    let result = sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to delete conversation");

    assert_eq!(result.rows_affected(), 1, "Delete should affect 1 row");

    // Verify it's gone from list
    let exists_after: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM conversations WHERE id = $1)"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to check existence");

    assert!(!exists_after, "Conversation should not exist after delete");
}

#[tokio::test]
async fn test_conversation_list_updates_on_message_add() {
    let pool = get_test_pool().await;

    // Create a conversation
    let conversation_id: Uuid = sqlx::query_scalar(
        "INSERT INTO conversations (title) VALUES ($1) RETURNING id"
    )
    .bind("State Test Message Conversation")
    .fetch_one(&pool)
    .await
    .expect("Failed to create conversation");

    // Check initial message count
    let initial_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = $1"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count messages");

    assert_eq!(initial_count, 0, "New conversation should have 0 messages");

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

    // Verify message count updated
    let new_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM messages WHERE conversation_id = $1"
    )
    .bind(conversation_id)
    .fetch_one(&pool)
    .await
    .expect("Failed to count messages");

    assert_eq!(
        new_count, 1,
        "Message count should update after adding message"
    );

    // Cleanup
    sqlx::query("DELETE FROM conversations WHERE id = $1")
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("Failed to cleanup");
}
