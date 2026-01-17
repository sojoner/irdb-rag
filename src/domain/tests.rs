//! Tests for domain models and pure utility functions

#[cfg(test)]
mod error_type_tests {
    use crate::domain::models::ErrorType;
    use std::str::FromStr;

    // ============================================
    // Error Classification Tests
    // ============================================

    #[test]
    fn test_classify_timeout_error() {
        let error_type = ErrorType::classify("Connection timeout");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_timeout_error_case_insensitive() {
        let error_type = ErrorType::classify("CONNECTION TIMEOUT");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_503_error() {
        let error_type = ErrorType::classify("HTTP 503 Service Unavailable");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_connection_refused() {
        let error_type = ErrorType::classify("Connection refused");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_rate_limit_error() {
        let error_type = ErrorType::classify("Rate limit exceeded");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_rate_limit_case_insensitive() {
        let error_type = ErrorType::classify("RATE LIMIT");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_file_not_found() {
        let error_type = ErrorType::classify("File not found");
        assert_eq!(error_type, ErrorType::Permanent);
    }

    #[test]
    fn test_classify_permission_denied() {
        let error_type = ErrorType::classify("Permission denied");
        assert_eq!(error_type, ErrorType::Permanent);
    }

    #[test]
    fn test_classify_invalid_url() {
        let error_type = ErrorType::classify("Invalid URL format");
        assert_eq!(error_type, ErrorType::Permanent);
    }

    #[test]
    fn test_classify_generic_error() {
        let error_type = ErrorType::classify("Unknown error occurred");
        assert_eq!(error_type, ErrorType::Permanent);
    }

    // ============================================
    // Error Type Conversion Tests
    // ============================================

    #[test]
    fn test_error_type_as_str_transient() {
        assert_eq!(ErrorType::Transient.as_str(), "transient");
    }

    #[test]
    fn test_error_type_as_str_permanent() {
        assert_eq!(ErrorType::Permanent.as_str(), "permanent");
    }

    #[test]
    fn test_error_type_parse_transient() {
        assert_eq!(ErrorType::parse("transient"), Some(ErrorType::Transient));
    }

    #[test]
    fn test_error_type_parse_permanent() {
        assert_eq!(ErrorType::parse("permanent"), Some(ErrorType::Permanent));
    }

    #[test]
    fn test_error_type_parse_invalid() {
        assert_eq!(ErrorType::parse("invalid"), None);
    }

    #[test]
    fn test_error_type_parse_empty() {
        assert_eq!(ErrorType::parse(""), None);
    }

    #[test]
    fn test_error_type_from_str_transient() {
        let result = ErrorType::from_str("transient");
        assert_eq!(result, Ok(ErrorType::Transient));
    }

    #[test]
    fn test_error_type_from_str_permanent() {
        let result = ErrorType::from_str("permanent");
        assert_eq!(result, Ok(ErrorType::Permanent));
    }

    #[test]
    fn test_error_type_from_str_invalid() {
        let result = ErrorType::from_str("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_error_type_roundtrip_transient() {
        let original = ErrorType::Transient;
        let as_str = original.as_str();
        let parsed = ErrorType::parse(as_str);
        assert_eq!(parsed, Some(original));
    }

    #[test]
    fn test_error_type_roundtrip_permanent() {
        let original = ErrorType::Permanent;
        let as_str = original.as_str();
        let parsed = ErrorType::parse(as_str);
        assert_eq!(parsed, Some(original));
    }

    // ============================================
    // Classification Edge Cases
    // ============================================

    #[test]
    fn test_classify_multiple_keywords() {
        // Contains both "timeout" and other words
        let error_type = ErrorType::classify("Database query timeout after 30 seconds");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_substring_match() {
        // Test that "timeout" is matched as substring
        let error_type = ErrorType::classify("The operation timeout");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_partial_status_code() {
        // Test 503 status code detection
        let error_type = ErrorType::classify("Error 503");
        assert_eq!(error_type, ErrorType::Transient);
    }

    #[test]
    fn test_classify_empty_string() {
        let error_type = ErrorType::classify("");
        assert_eq!(error_type, ErrorType::Permanent);
    }

    #[test]
    fn test_classify_whitespace_only() {
        let error_type = ErrorType::classify("   ");
        assert_eq!(error_type, ErrorType::Permanent);
    }

    // ============================================
    // Error Type Equality Tests
    // ============================================

    #[test]
    fn test_error_type_equality() {
        assert_eq!(ErrorType::Transient, ErrorType::Transient);
        assert_eq!(ErrorType::Permanent, ErrorType::Permanent);
    }

    #[test]
    fn test_error_type_inequality() {
        assert_ne!(ErrorType::Transient, ErrorType::Permanent);
    }
}

#[cfg(test)]
mod import_config_tests {
    use crate::services::import::ImportConfig;

    #[test]
    fn test_import_config_default_values() {
        let config = ImportConfig {
            workers: 2,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 4096,
            max_retries: 3,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 30000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_base_delay_ms, 1000);
        assert_eq!(config.retry_max_delay_ms, 30000);
    }

    #[test]
    fn test_import_config_clone() {
        let config = ImportConfig {
            workers: 4,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 4096,
            max_retries: 5,
            retry_base_delay_ms: 2000,
            retry_max_delay_ms: 60000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        let cloned = config.clone();
        assert_eq!(config.max_retries, cloned.max_retries);
        assert_eq!(config.retry_base_delay_ms, cloned.retry_base_delay_ms);
        assert_eq!(config.retry_max_delay_ms, cloned.retry_max_delay_ms);
    }
}

#[cfg(test)]
mod retry_delay_tests {
    use crate::services::import::{calculate_retry_delay, ImportConfig};

    #[test]
    fn test_calculate_retry_delay_first_attempt() {
        let config = ImportConfig {
            workers: 2,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 4096,
            max_retries: 3,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 30000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        let delay = calculate_retry_delay(0, &config);
        // Exponential backoff: 1000 * 2^0 = 1000ms + jitter
        assert!(delay.as_millis() >= 1000);
        assert!(delay.as_millis() <= 1100);
    }

    #[test]
    fn test_calculate_retry_delay_exponential_growth() {
        let config = ImportConfig {
            workers: 2,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 4096,
            max_retries: 3,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 60000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        let delay_0 = calculate_retry_delay(0, &config);
        let delay_1 = calculate_retry_delay(1, &config);
        let delay_2 = calculate_retry_delay(2, &config);

        // Each retry should have a longer base delay
        assert!(delay_1.as_millis() > delay_0.as_millis());
        assert!(delay_2.as_millis() > delay_1.as_millis());
    }

    #[test]
    fn test_calculate_retry_delay_capped() {
        let config = ImportConfig {
            workers: 2,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 4096,
            max_retries: 10,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 5000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        let delay = calculate_retry_delay(10, &config);
        // Should be capped at max_delay_ms
        assert!(delay.as_millis() <= 5500); // 5000 + 10% jitter
    }

    #[test]
    fn test_calculate_retry_delay_respects_max() {
        let config = ImportConfig {
            workers: 2,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 4096,
            max_retries: 3,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 10000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        for attempt in 0..20 {
            let delay = calculate_retry_delay(attempt, &config);
            assert!(delay.as_millis() <= 11000); // max + 10% jitter
        }
    }

    #[test]
    fn test_calculate_retry_delay_jitter_variance() {
        let config = ImportConfig {
            workers: 2,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 512,
            max_retries: 3,
            retry_base_delay_ms: 1000,
            retry_max_delay_ms: 30000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        // Calculate multiple times to see variance from jitter
        let delays: Vec<_> = (0..5).map(|_| calculate_retry_delay(1, &config)).collect();

        // All should be in reasonable range but not identical
        for delay in &delays {
            assert!(delay.as_millis() >= 2000);
            assert!(delay.as_millis() <= 2200);
        }
    }

    #[test]
    fn test_calculate_retry_delay_zero_base() {
        let config = ImportConfig {
            workers: 2,
            indexing_batch_size: 4,
            max_concurrent_documents: 32,
            entity_extraction_batches: 16,
            chunk_size_tokens: 512,
            max_retries: 3,
            retry_base_delay_ms: 0,
            retry_max_delay_ms: 30000,
            cleanup: crate::config::JobCleanupConfig::default(),
        };

        let delay = calculate_retry_delay(0, &config);
        // Even with 0 base, should get minimal jitter
        assert!(delay.as_millis() < 100);
    }
}

#[cfg(test)]
mod import_progress_tests {
    use crate::domain::models::{ImportJob, ImportProgress};
    use chrono::Utc;
    use uuid::Uuid;

    fn create_test_job(total: i32, processed: i32, failed: i32, skipped: i32) -> ImportJob {
        ImportJob {
            id: Uuid::new_v4(),
            status: "running".to_string(),
            source_type: "file".to_string(),
            source_path: Some("/test".to_string()),
            total_items: total,
            processed_items: processed,
            failed_items: failed,
            skipped_items: skipped,
            error_message: None,
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
        }
    }

    #[test]
    fn test_import_progress_from_job_all_completed() {
        let job = create_test_job(10, 10, 0, 0);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 10);
        assert_eq!(progress.processed, 10);
        assert_eq!(progress.completed, 10);
        assert_eq!(progress.failed, 0);
        assert_eq!(progress.skipped, 0);
        assert_eq!(progress.percent, 100.0);
    }

    #[test]
    fn test_import_progress_from_job_partial_completion() {
        let job = create_test_job(10, 5, 0, 0);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 10);
        assert_eq!(progress.processed, 5);
        assert_eq!(progress.completed, 5);
        assert_eq!(progress.percent, 50.0);
    }

    #[test]
    fn test_import_progress_from_job_with_failures() {
        let job = create_test_job(10, 7, 2, 0);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 10);
        assert_eq!(progress.processed, 7);
        assert_eq!(progress.failed, 2);
        assert_eq!(progress.completed, 5); // 7 - 2 - 0
        assert_eq!(progress.percent, 70.0);
    }

    #[test]
    fn test_import_progress_from_job_with_skipped() {
        let job = create_test_job(10, 9, 1, 3);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 10);
        assert_eq!(progress.processed, 9);
        assert_eq!(progress.failed, 1);
        assert_eq!(progress.skipped, 3);
        assert_eq!(progress.completed, 5); // 9 - 1 - 3
        assert_eq!(progress.percent, 90.0);
    }

    #[test]
    fn test_import_progress_from_job_no_items() {
        let job = create_test_job(0, 0, 0, 0);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 0);
        assert_eq!(progress.processed, 0);
        assert_eq!(progress.completed, 0);
        // Should not panic due to max(1) in from_job
        assert_eq!(progress.percent, 0.0);
    }

    #[test]
    fn test_import_progress_from_job_partial() {
        let job = create_test_job(100, 33, 5, 2);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 100);
        assert_eq!(progress.processed, 33);
        assert_eq!(progress.failed, 5);
        assert_eq!(progress.skipped, 2);
        assert_eq!(progress.completed, 26); // 33 - 5 - 2
        assert_eq!(progress.percent, 33.0);
    }

    #[test]
    fn test_import_progress_percent_precision() {
        let job = create_test_job(3, 1, 0, 0);
        let progress = ImportProgress::from_job(&job);

        // 1/3 = 0.333... * 100 = 33.333...
        assert!(progress.percent > 33.0);
        assert!(progress.percent < 34.0);
    }

    #[test]
    fn test_import_progress_all_failed() {
        let job = create_test_job(10, 10, 10, 0);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 10);
        assert_eq!(progress.processed, 10);
        assert_eq!(progress.failed, 10);
        assert_eq!(progress.completed, 0); // 10 - 10 - 0
        assert_eq!(progress.percent, 100.0);
    }

    #[test]
    fn test_import_progress_all_skipped() {
        let job = create_test_job(10, 10, 0, 10);
        let progress = ImportProgress::from_job(&job);

        assert_eq!(progress.total, 10);
        assert_eq!(progress.processed, 10);
        assert_eq!(progress.skipped, 10);
        assert_eq!(progress.completed, 0); // 10 - 0 - 10
        assert_eq!(progress.percent, 100.0);
    }

    #[test]
    fn test_import_progress_serialization_roundtrip() {
        let job = create_test_job(10, 5, 1, 1);
        let progress = ImportProgress::from_job(&job);

        let json = serde_json::to_string(&progress).unwrap();
        let deserialized: ImportProgress = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.total, progress.total);
        assert_eq!(deserialized.processed, progress.processed);
        assert_eq!(deserialized.completed, progress.completed);
        assert_eq!(deserialized.failed, progress.failed);
        assert_eq!(deserialized.skipped, progress.skipped);
        assert_eq!(deserialized.percent, progress.percent);
    }
}

#[cfg(test)]
mod dto_tests {
    use crate::domain::dtos::{ChatRequest, SearchRequest};
    use uuid::Uuid;

    #[test]
    fn test_search_request_default_values() {
        let json = r#"{"query": "test"}"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.query, "test");
        assert_eq!(req.limit, 10);
        assert_eq!(req.bm25_weight, 0.5);
        assert_eq!(req.vector_weight, 0.5);
    }

    #[test]
    fn test_search_request_custom_values() {
        let json = r#"{
            "query": "ai",
            "limit": 20,
            "bm25_weight": 0.7,
            "vector_weight": 0.3
        }"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.query, "ai");
        assert_eq!(req.limit, 20);
        assert_eq!(req.bm25_weight, 0.7);
        assert_eq!(req.vector_weight, 0.3);
    }

    #[test]
    fn test_search_request_with_filters() {
        let json = r#"{
            "query": "test",
            "keywords": ["ai", "ml"],
            "locations": ["NYC"],
            "word_count_min": 100,
            "word_count_max": 5000
        }"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.keywords, Some(vec!["ai".to_string(), "ml".to_string()]));
        assert_eq!(req.locations, Some(vec!["NYC".to_string()]));
        assert_eq!(req.word_count_min, Some(100));
        assert_eq!(req.word_count_max, Some(5000));
    }

    #[test]
    fn test_chat_request_default_values() {
        let json = r#"{"message": "Hello"}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();

        assert_eq!(req.message, "Hello");
        assert_eq!(req.context_chunks, 5);
        assert!(req.conversation_id.is_none());
    }

    #[test]
    fn test_chat_request_with_conversation_id() {
        let conv_id = Uuid::new_v4();
        let json = serde_json::json!({
            "message": "Hello",
            "conversation_id": conv_id.to_string(),
            "context_chunks": 10
        });
        let req: ChatRequest = serde_json::from_str(&json.to_string()).unwrap();

        assert_eq!(req.message, "Hello");
        assert_eq!(req.context_chunks, 10);
        assert_eq!(req.conversation_id, Some(conv_id));
    }

    #[test]
    fn test_chat_request_with_document_ids() {
        let doc_id1 = Uuid::new_v4();
        let doc_id2 = Uuid::new_v4();
        let json = serde_json::json!({
            "message": "Search in these docs",
            "document_ids": [doc_id1.to_string(), doc_id2.to_string()]
        });
        let req: ChatRequest = serde_json::from_str(&json.to_string()).unwrap();

        assert_eq!(req.message, "Search in these docs");
        assert_eq!(req.document_ids.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_search_request_serialization_roundtrip() {
        let json = r#"{"query": "test", "limit": 25, "bm25_weight": 0.6, "vector_weight": 0.4}"#;
        let req: SearchRequest = serde_json::from_str(json).unwrap();
        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: SearchRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(req.query, deserialized.query);
        assert_eq!(req.limit, deserialized.limit);
        assert_eq!(req.bm25_weight, deserialized.bm25_weight);
        assert_eq!(req.vector_weight, deserialized.vector_weight);
    }

    #[test]
    fn test_chat_request_serialization_roundtrip() {
        let json = r#"{"message": "Test message", "context_chunks": 7}"#;
        let req: ChatRequest = serde_json::from_str(json).unwrap();
        let serialized = serde_json::to_string(&req).unwrap();
        let deserialized: ChatRequest = serde_json::from_str(&serialized).unwrap();

        assert_eq!(req.message, deserialized.message);
        assert_eq!(req.context_chunks, deserialized.context_chunks);
    }
}
