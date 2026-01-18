//! Test chat conversation flow with message history

use rag_chat::domain::dtos::{ChatMessage, ChatConversationRequest, ChatConversationResponse};
use uuid::Uuid;

#[test]
fn test_chat_message_creation() {
    let msg = ChatMessage {
        role: "user".to_string(),
        content: "Hello, how are you?".to_string(),
    };

    assert_eq!(msg.role, "user");
    assert_eq!(msg.content, "Hello, how are you?");
}

#[test]
fn test_chat_conversation_request_with_history() {
    let history = vec![
        ChatMessage {
            role: "user".to_string(),
            content: "What is Rust?".to_string(),
        },
        ChatMessage {
            role: "assistant".to_string(),
            content: "Rust is a systems programming language.".to_string(),
        },
    ];

    let req = ChatConversationRequest {
        messages: history.clone(),
        conversation_id: Some(Uuid::new_v4()),
        document_ids: None,
        context_chunks: 5,
    };

    assert_eq!(req.messages.len(), 2);
    assert_eq!(req.messages[0].role, "user");
    assert_eq!(req.messages[1].role, "assistant");
}

#[test]
fn test_chat_conversation_request_with_search_context() {
    let doc_ids = vec![Uuid::new_v4(), Uuid::new_v4()];

    let req = ChatConversationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "Summarize these documents".to_string(),
        }],
        conversation_id: None,
        document_ids: Some(doc_ids.clone()),
        context_chunks: 10,
    };

    assert!(req.document_ids.is_some());
    assert_eq!(req.document_ids.unwrap().len(), 2);
    assert_eq!(req.context_chunks, 10);
}

#[test]
fn test_chat_conversation_response() {
    let sources = vec![];

    let resp = ChatConversationResponse {
        message: ChatMessage {
            role: "assistant".to_string(),
            content: "Here is my response".to_string(),
        },
        conversation_id: Uuid::new_v4(),
        sources,
    };

    assert_eq!(resp.message.role, "assistant");
    assert_eq!(resp.message.content, "Here is my response");
}

#[test]
fn test_empty_conversation_initialization() {
    let req = ChatConversationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: "First message".to_string(),
        }],
        conversation_id: None,
        document_ids: None,
        context_chunks: 5,
    };

    assert!(req.conversation_id.is_none());
    assert_eq!(req.messages.len(), 1);
}
