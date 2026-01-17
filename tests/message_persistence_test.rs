//! Test message persistence for chat conversations
//!
//! Tests that messages are correctly saved and loaded from the database

use rag_chat::config::Settings;
use rag_chat::infra::db;
use uuid::Uuid;

async fn setup_pool() -> sqlx::PgPool {
    if std::env::var("RUN_ENV").is_err() {
        std::env::set_var("RUN_ENV", "test");
    }

    let settings = Settings::new().expect("Failed to load settings");
    let db_url = settings.database.url.clone();

    sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(30))
        .connect(&db_url)
        .await
        .expect("Failed to connect to database")
}

#[tokio::test]
async fn test_create_conversation() {
    let pool = setup_pool().await;

    let title = "Test Conversation";
    let conv_id = db::create_conversation(&pool, title)
        .await
        .expect("Failed to create conversation");

    // Verify conversation was created
    let conv = db::get_conversation(&pool, conv_id)
        .await
        .expect("Failed to get conversation")
        .expect("Conversation not found");

    assert_eq!(conv.0, conv_id);
    assert_eq!(conv.1, Some(title.to_string()));
    println!("✓ Created conversation: {}", conv_id);

    pool.close().await;
}

#[tokio::test]
async fn test_save_and_load_messages() {
    let pool = setup_pool().await;

    // Create a conversation
    let conv_id = db::create_conversation(&pool, "Message Test")
        .await
        .expect("Failed to create conversation");

    // Save messages
    let user_msg = "Hello, can you help me?";
    let assistant_msg = "Of course! I'm here to help.";

    db::save_message(&pool, conv_id, "user", user_msg)
        .await
        .expect("Failed to save user message");

    db::save_message(&pool, conv_id, "assistant", assistant_msg)
        .await
        .expect("Failed to save assistant message");

    // Load messages
    let messages = db::load_conversation(&pool, conv_id)
        .await
        .expect("Failed to load conversation");

    // Verify messages
    assert_eq!(messages.len(), 2, "Should have 2 messages");
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[0].content, user_msg);
    assert_eq!(messages[1].role, "assistant");
    assert_eq!(messages[1].content, assistant_msg);

    println!("✓ Saved and loaded messages correctly");
    pool.close().await;
}

#[tokio::test]
async fn test_multi_turn_conversation() {
    let pool = setup_pool().await;

    // Create conversation
    let conv_id = db::create_conversation(&pool, "Multi-turn Test")
        .await
        .expect("Failed to create conversation");

    // Simulate a multi-turn conversation
    let exchanges = vec![
        ("user", "What is Rust?"),
        ("assistant", "Rust is a systems programming language."),
        ("user", "Why should I learn Rust?"),
        (
            "assistant",
            "Rust provides memory safety and high performance without garbage collection.",
        ),
        ("user", "How do I get started?"),
        ("assistant", "You can download Rust from rustup.rs and follow the official book."),
    ];

    // Save all messages
    for (role, content) in &exchanges {
        db::save_message(&pool, conv_id, role, content)
            .await
            .expect("Failed to save message");
    }

    // Load all messages
    let messages = db::load_conversation(&pool, conv_id)
        .await
        .expect("Failed to load conversation");

    // Verify conversation integrity
    assert_eq!(messages.len(), exchanges.len());
    for (i, (role, content)) in exchanges.iter().enumerate() {
        assert_eq!(messages[i].role, *role);
        assert_eq!(messages[i].content, *content);
    }

    println!(
        "✓ Multi-turn conversation with {} exchanges loaded correctly",
        exchanges.len()
    );
    pool.close().await;
}

#[tokio::test]
async fn test_empty_conversation_load() {
    let pool = setup_pool().await;

    // Create a conversation without messages
    let conv_id = db::create_conversation(&pool, "Empty Conversation")
        .await
        .expect("Failed to create conversation");

    // Load messages from empty conversation
    let messages = db::load_conversation(&pool, conv_id)
        .await
        .expect("Failed to load conversation");

    // Should be empty
    assert_eq!(messages.len(), 0, "Empty conversation should have no messages");

    println!("✓ Empty conversation loaded correctly (0 messages)");
    pool.close().await;
}

#[tokio::test]
async fn test_update_conversation_title() {
    let pool = setup_pool().await;

    // Create conversation
    let conv_id = db::create_conversation(&pool, "Old Title")
        .await
        .expect("Failed to create conversation");

    // Update title
    let new_title = "Updated Title";
    db::update_conversation_title(&pool, conv_id, new_title)
        .await
        .expect("Failed to update title");

    // Verify title was updated
    let conv = db::get_conversation(&pool, conv_id)
        .await
        .expect("Failed to get conversation")
        .expect("Conversation not found");

    assert_eq!(conv.1, Some(new_title.to_string()));

    println!("✓ Updated conversation title: {}", new_title);
    pool.close().await;
}

#[tokio::test]
async fn test_message_order_preserved() {
    let pool = setup_pool().await;

    let conv_id = db::create_conversation(&pool, "Order Test")
        .await
        .expect("Failed to create conversation");

    // Save messages with slight delays to test ordering
    let messages_to_save = vec!["First", "Second", "Third", "Fourth", "Fifth"];

    for (i, msg) in messages_to_save.iter().enumerate() {
        db::save_message(&pool, conv_id, "user", msg)
            .await
            .expect("Failed to save message");

        // Small delay to ensure different timestamps
        if i < messages_to_save.len() - 1 {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    // Load and verify order
    let loaded = db::load_conversation(&pool, conv_id)
        .await
        .expect("Failed to load conversation");

    assert_eq!(loaded.len(), messages_to_save.len());
    for (i, expected_msg) in messages_to_save.iter().enumerate() {
        assert_eq!(
            loaded[i].content, *expected_msg,
            "Message order not preserved at index {}",
            i
        );
    }

    println!("✓ Message order preserved correctly");
    pool.close().await;
}

#[tokio::test]
async fn test_long_message_content() {
    let pool = setup_pool().await;

    let conv_id = db::create_conversation(&pool, "Long Message Test")
        .await
        .expect("Failed to create conversation");

    // Create a very long message (simulating full LLM response)
    let long_content = "Lorem ipsum dolor sit amet, consectetur adipiscing elit. \
        Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. \
        Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris. ".repeat(100);

    db::save_message(&pool, conv_id, "assistant", &long_content)
        .await
        .expect("Failed to save long message");

    // Load and verify
    let messages = db::load_conversation(&pool, conv_id)
        .await
        .expect("Failed to load conversation");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, long_content);

    println!(
        "✓ Long message ({} chars) saved and loaded correctly",
        long_content.len()
    );
    pool.close().await;
}
