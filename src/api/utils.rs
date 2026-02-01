//! Pure utility functions for API handlers
//!
//! This module contains pure, side-effect-free functions extracted from handlers.
//! These are easily testable and don't require mocking or database access.

use crate::domain::dtos::SearchRequest;
use crate::domain::dtos::SourceReference;
use crate::domain::models::DocumentChunk;
use crate::infra::db::SearchFilters;
use uuid::Uuid;

// ============================================
// Filter Construction
// ============================================

/// Build search filters from a search request
/// Pure function - no side effects
pub fn build_search_filters(req: &SearchRequest) -> SearchFilters {
    SearchFilters {
        category_id: req.category_id,
        date_from: req.date_from.as_ref().and_then(|d| d.parse().ok()),
        date_to: req.date_to.as_ref().and_then(|d| d.parse().ok()),
        locations: req.locations.clone(),
        keywords: req.keywords.clone(),
        source_types: None,
        authors: req.authors.clone(),
        concepts: req.concepts.clone(),
        organizations: req.organizations.clone(),
        persons: req.persons.clone(),
        products: req.products.clone(),
        word_count_min: req.word_count_min,
        word_count_max: req.word_count_max,
    }
}

// ============================================
// Context Building
// ============================================

/// Build context string from document chunks
/// Pure function - no side effects
pub fn build_context(chunks: &[DocumentChunk]) -> String {
    chunks
        .iter()
        .map(|c| format!("---\n{}\n", c.content))
        .collect()
}

// ============================================
// Prompt Building
// ============================================

/// Default system prompt for RAG
pub const DEFAULT_SYSTEM_PROMPT: &str =
    "You are a helpful assistant answering questions based on the provided context from documents. \
     Answer based ONLY on the context provided. If the context doesn't contain enough information to answer, say so. \
     Be concise and cite specific parts of the context when relevant.";

/// Resolve system prompt: prefer environment variable, fallback to default
/// Pure function - no side effects
pub fn resolve_system_prompt(env_override: Option<String>) -> String {
    env_override.unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string())
}

/// Build user prompt from context and message
/// Pure function - no side effects
pub fn build_user_prompt(context: &str, message: &str) -> String {
    format!("CONTEXT:\n{}\n\nQUESTION:\n{}", context, message)
}

/// Build complete LLM prompt (system + user)
/// Pure function - no side effects
#[allow(dead_code)]
pub fn build_full_prompt(system: &str, user: &str) -> (String, String) {
    (system.to_string(), user.to_string())
}

// ============================================
// Source Reference Building
// ============================================

/// Build source references from chunks
/// Pure function - no side effects
pub fn build_source_references(chunks: &[DocumentChunk]) -> Vec<SourceReference> {
    chunks
        .iter()
        .enumerate()
        .map(|(i, c)| SourceReference {
            document_id: c.document_id,
            title: c
                .section_title
                .clone()
                .unwrap_or_else(|| format!("Chunk {}", i + 1)),
            chunk: c.content.chars().take(200).collect::<String>() + "...",
            relevance: 1.0 - (i as f64 * 0.1),
        })
        .collect()
}

// ============================================
// Conversation ID Management
// ============================================

/// Generate new conversation ID or use provided one
/// Pure function - no side effects
pub fn resolve_conversation_id(provided_id: Option<Uuid>) -> Uuid {
    provided_id.unwrap_or_else(Uuid::new_v4)
}

// ============================================
// Validation
// ============================================

/// Validate search request parameters
/// Returns tuple: (is_valid, error_message)
pub fn validate_search_request(
    limit: i32,
    bm25_weight: f64,
    vector_weight: f64,
) -> (bool, Option<String>) {
    if limit <= 0 || limit > 100 {
        return (false, Some("limit must be between 1 and 100".to_string()));
    }

    if !(0.0..=1.0).contains(&bm25_weight) {
        return (
            false,
            Some("bm25_weight must be between 0.0 and 1.0".to_string()),
        );
    }

    if !(0.0..=1.0).contains(&vector_weight) {
        return (
            false,
            Some("vector_weight must be between 0.0 and 1.0".to_string()),
        );
    }

    (true, None)
}

/// Validate chat request parameters
pub fn validate_chat_request(context_chunks: i32, message: &str) -> (bool, Option<String>) {
    if context_chunks <= 0 || context_chunks > 50 {
        return (
            false,
            Some("context_chunks must be between 1 and 50".to_string()),
        );
    }

    if message.trim().is_empty() {
        return (false, Some("message cannot be empty".to_string()));
    }

    if message.len() > 10000 {
        return (
            false,
            Some("message too long (max 10000 chars)".to_string()),
        );
    }

    (true, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // Filter Construction Tests
    // ============================================

    #[test]
    fn test_build_search_filters_with_no_filters() {
        let req = SearchRequest {
            query: "test".to_string(),
            limit: 10,
            sort: Default::default(),
            search_fields: vec!["content".to_string()],
            bm25_weight: 0.5,
            vector_weight: 0.5,
            category_id: None,
            date_from: None,
            date_to: None,
            locations: None,
            keywords: None,
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: None,
            word_count_max: None,
        };

        let filters = build_search_filters(&req);
        assert!(filters.category_id.is_none());
        assert!(filters.date_from.is_none());
        assert!(filters.locations.is_none());
    }

    #[test]
    fn test_build_search_filters_with_all_filters() {
        let req = SearchRequest {
            query: "test".to_string(),
            limit: 10,
            sort: Default::default(),
            search_fields: vec!["content".to_string()],
            bm25_weight: 0.5,
            vector_weight: 0.5,
            category_id: Some(Uuid::new_v4()),
            date_from: Some("2024-01-01".to_string()),
            date_to: Some("2024-12-31".to_string()),
            locations: Some(vec!["NYC".to_string()]),
            keywords: Some(vec!["ai".to_string()]),
            authors: Some(vec!["John".to_string()]),
            concepts: Some(vec!["learning".to_string()]),
            organizations: Some(vec!["OpenAI".to_string()]),
            persons: Some(vec!["Alice".to_string()]),
            products: Some(vec!["GPT".to_string()]),
            word_count_min: Some(100),
            word_count_max: Some(10000),
        };

        let filters = build_search_filters(&req);
        assert!(filters.category_id.is_some());
        assert_eq!(filters.locations, Some(vec!["NYC".to_string()]));
        assert_eq!(filters.keywords, Some(vec!["ai".to_string()]));
        assert_eq!(filters.word_count_min, Some(100));
        assert_eq!(filters.word_count_max, Some(10000));
    }

    #[test]
    fn test_build_search_filters_with_invalid_dates() {
        let req = SearchRequest {
            query: "test".to_string(),
            limit: 10,
            sort: Default::default(),
            search_fields: vec!["content".to_string()],
            bm25_weight: 0.5,
            vector_weight: 0.5,
            category_id: None,
            date_from: Some("invalid-date".to_string()),
            date_to: Some("also-invalid".to_string()),
            locations: None,
            keywords: None,
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: None,
            word_count_max: None,
        };

        let filters = build_search_filters(&req);
        // Invalid dates should result in None after parsing
        assert!(filters.date_from.is_none());
        assert!(filters.date_to.is_none());
    }

    // ============================================
    // Context Building Tests
    // ============================================

    #[test]
    fn test_build_context_empty_chunks() {
        let chunks: Vec<DocumentChunk> = vec![];
        let context = build_context(&chunks);
        assert_eq!(context, "");
    }

    #[test]
    fn test_build_context_single_chunk() {
        let chunk = DocumentChunk {
            id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            chunk_index: 0,
            content: "This is the chunk content".to_string(),
            page_number: None,
            section_title: None,
        };

        let context = build_context(&[chunk]);
        assert!(context.contains("---"));
        assert!(context.contains("This is the chunk content"));
    }

    #[test]
    fn test_build_context_multiple_chunks() {
        let chunks = vec![
            DocumentChunk {
                id: Uuid::new_v4(),
                document_id: Uuid::new_v4(),
                chunk_index: 0,
                content: "Chunk 1".to_string(),
                page_number: None,
                section_title: None,
            },
            DocumentChunk {
                id: Uuid::new_v4(),
                document_id: Uuid::new_v4(),
                chunk_index: 1,
                content: "Chunk 2".to_string(),
                page_number: None,
                section_title: None,
            },
        ];

        let context = build_context(&chunks);
        assert!(context.contains("Chunk 1"));
        assert!(context.contains("Chunk 2"));
        let separator_count = context.matches("---").count();
        assert_eq!(separator_count, 2); // Format: "---\n{content}\n" so 1 separator per chunk
    }

    // ============================================
    // Prompt Building Tests
    // ============================================

    #[test]
    fn test_resolve_system_prompt_with_override() {
        let override_prompt = Some("Custom prompt".to_string());
        let resolved = resolve_system_prompt(override_prompt);
        assert_eq!(resolved, "Custom prompt");
    }

    #[test]
    fn test_resolve_system_prompt_without_override() {
        let resolved = resolve_system_prompt(None);
        assert_eq!(resolved, DEFAULT_SYSTEM_PROMPT);
    }

    #[test]
    fn test_build_user_prompt() {
        let context = "Context here";
        let message = "What is AI?";
        let prompt = build_user_prompt(context, message);

        assert!(prompt.contains("CONTEXT:"));
        assert!(prompt.contains("Context here"));
        assert!(prompt.contains("QUESTION:"));
        assert!(prompt.contains("What is AI?"));
    }

    #[test]
    fn test_build_user_prompt_with_empty_context() {
        let context = "";
        let message = "Test";
        let prompt = build_user_prompt(context, message);

        assert!(prompt.contains("CONTEXT:"));
        assert!(prompt.contains("QUESTION:"));
    }

    #[test]
    fn test_build_user_prompt_with_multiline_content() {
        let context = "Line 1\nLine 2\nLine 3";
        let message = "Question\nWith multiple lines";
        let prompt = build_user_prompt(context, message);

        assert!(prompt.contains("Line 1"));
        assert!(prompt.contains("Line 3"));
        assert!(prompt.contains("Question"));
    }

    // ============================================
    // Source Reference Building Tests
    // ============================================

    #[test]
    fn test_build_source_references_empty() {
        let chunks: Vec<DocumentChunk> = vec![];
        let sources = build_source_references(&chunks);
        assert!(sources.is_empty());
    }

    #[test]
    fn test_build_source_references_single_chunk() {
        let chunk = DocumentChunk {
            id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            chunk_index: 0,
            content: "This is a long chunk content that should be truncated at 200 characters when used as a source reference in the system."
                .to_string(),
            page_number: Some(1),
            section_title: Some("Introduction".to_string()),
        };
        let doc_id = chunk.document_id;

        let sources = build_source_references(&[chunk]);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].document_id, doc_id);
        assert_eq!(sources[0].title, "Introduction");
        assert!(sources[0].chunk.ends_with("..."));
        assert_eq!(sources[0].relevance, 1.0);
    }

    #[test]
    fn test_build_source_references_multiple_chunks() {
        let chunks = vec![
            DocumentChunk {
                id: Uuid::new_v4(),
                document_id: Uuid::new_v4(),
                chunk_index: 0,
                content: "Chunk 1".to_string(),
                page_number: Some(1),
                section_title: Some("Section A".to_string()),
            },
            DocumentChunk {
                id: Uuid::new_v4(),
                document_id: Uuid::new_v4(),
                chunk_index: 1,
                content: "Chunk 2".to_string(),
                page_number: Some(2),
                section_title: Some("Section B".to_string()),
            },
            DocumentChunk {
                id: Uuid::new_v4(),
                document_id: Uuid::new_v4(),
                chunk_index: 2,
                content: "Chunk 3".to_string(),
                page_number: Some(3),
                section_title: None,
            },
        ];

        let sources = build_source_references(&chunks);

        assert_eq!(sources.len(), 3);
        assert_eq!(sources[0].title, "Section A");
        assert_eq!(sources[1].title, "Section B");
        assert_eq!(sources[2].title, "Chunk 3"); // Generated title

        // Check relevance decreases
        assert_eq!(sources[0].relevance, 1.0);
        assert_eq!(sources[1].relevance, 0.9);
        assert!(sources[2].relevance < sources[1].relevance);
    }

    #[test]
    fn test_build_source_references_chunk_truncation() {
        let long_content = "a".repeat(300);
        let chunk = DocumentChunk {
            id: Uuid::new_v4(),
            document_id: Uuid::new_v4(),
            chunk_index: 0,
            content: long_content,
            page_number: None,
            section_title: None,
        };

        let sources = build_source_references(&[chunk]);

        assert!(sources[0].chunk.len() <= 203); // 200 chars + "..."
        assert!(sources[0].chunk.ends_with("..."));
    }

    // ============================================
    // Conversation ID Tests
    // ============================================

    #[test]
    fn test_resolve_conversation_id_with_provided_id() {
        let provided = Uuid::new_v4();
        let result = resolve_conversation_id(Some(provided));
        assert_eq!(result, provided);
    }

    #[test]
    fn test_resolve_conversation_id_generates_new() {
        let result1 = resolve_conversation_id(None);
        let result2 = resolve_conversation_id(None);
        assert_ne!(result1, result2); // Should generate different IDs
    }

    // ============================================
    // Validation Tests
    // ============================================

    #[test]
    fn test_validate_search_request_valid() {
        let (valid, error) = validate_search_request(10, 0.5, 0.5);
        assert!(valid);
        assert!(error.is_none());
    }

    #[test]
    fn test_validate_search_request_invalid_limit_zero() {
        let (valid, error) = validate_search_request(0, 0.5, 0.5);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_search_request_invalid_limit_too_high() {
        let (valid, error) = validate_search_request(101, 0.5, 0.5);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_search_request_valid_limit_boundaries() {
        let (valid, _) = validate_search_request(1, 0.5, 0.5);
        assert!(valid);

        let (valid, _) = validate_search_request(100, 0.5, 0.5);
        assert!(valid);
    }

    #[test]
    fn test_validate_search_request_invalid_bm25_weight() {
        let (valid, error) = validate_search_request(10, 1.5, 0.5);
        assert!(!valid);
        assert!(error.is_some());

        let (valid, error) = validate_search_request(10, -0.1, 0.5);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_search_request_invalid_vector_weight() {
        let (valid, error) = validate_search_request(10, 0.5, 1.5);
        assert!(!valid);
        assert!(error.is_some());

        let (valid, error) = validate_search_request(10, 0.5, -0.1);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_search_request_valid_weight_boundaries() {
        let (valid, _) = validate_search_request(10, 0.0, 1.0);
        assert!(valid);

        let (valid, _) = validate_search_request(10, 1.0, 0.0);
        assert!(valid);
    }

    #[test]
    fn test_validate_chat_request_valid() {
        let (valid, error) = validate_chat_request(5, "Hello");
        assert!(valid);
        assert!(error.is_none());
    }

    #[test]
    fn test_validate_chat_request_invalid_context_chunks_zero() {
        let (valid, error) = validate_chat_request(0, "Hello");
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chat_request_invalid_context_chunks_too_high() {
        let (valid, error) = validate_chat_request(51, "Hello");
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chat_request_empty_message() {
        let (valid, error) = validate_chat_request(5, "");
        assert!(!valid);
        assert!(error.is_some());

        let (valid, error) = validate_chat_request(5, "   ");
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chat_request_message_too_long() {
        let long_message = "a".repeat(10001);
        let (valid, error) = validate_chat_request(5, &long_message);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chat_request_valid_boundaries() {
        let (valid, _) = validate_chat_request(1, "a");
        assert!(valid);

        let (valid, _) = validate_chat_request(50, &"a".repeat(10000));
        assert!(valid);
    }
}
