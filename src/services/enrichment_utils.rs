//! Pure functions for document enrichment
//!
//! This module extracts deterministic, side-effect-free functions used in document enrichment,
//! metadata extraction, and entity processing.

use serde_json::{json, Value};
use std::collections::HashSet;
use uuid::Uuid;

/// Parse comma-separated keyword string into a vector
///
/// # Examples
/// ```
/// use rag_chat::services::enrichment_utils::parse_keywords_from_string;
/// let keywords = "rust, database, vector".to_string();
/// let parsed = parse_keywords_from_string(&keywords);
/// assert_eq!(parsed.len(), 3);
/// ```
pub fn parse_keywords_from_string(response: &str) -> Vec<String> {
    response
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() < 50)
        .take(8) // Limit to 8 keywords
        .collect()
}

/// Split text into sentences using basic sentence boundary detection
///
/// This is a deterministic text processing function that uses simple heuristics
/// to split text into sentences based on common punctuation marks.
pub fn split_text_by_sentences(text: &str) -> Vec<String> {
    use unicode_segmentation::UnicodeSegmentation;

    text.split_sentence_bounds()
        .map(|s: &str| s.trim().to_string())
        .filter(|s: &String| !s.is_empty())
        .collect()
}

/// Collect context from sentences up to a character limit
///
/// # Arguments
/// * `sentences` - List of sentences to process
/// * `char_limit` - Maximum character count before stopping
///
/// # Returns
/// Joined sentences up to the character limit
///
/// # Examples
/// ```
/// use rag_chat::services::enrichment_utils::collect_context;
/// let sentences = vec!["Hello.".to_string(), "World.".to_string()];
/// let context = collect_context(&sentences, 20);
/// assert_eq!(context, "Hello. World.");
/// ```
pub fn collect_context(sentences: &[String], char_limit: usize) -> String {
    let mut result = String::new();
    let mut len = 0;

    for sentence in sentences {
        if len + sentence.len() <= char_limit {
            if !result.is_empty() {
                result.push(' ');
                len += 1;
            }
            result.push_str(sentence);
            len += sentence.len();
        } else {
            break;
        }
    }

    result
}

/// Extract first N characters from text with fallback
pub fn extract_preview(text: &str, max_chars: usize) -> String {
    if text.len() > max_chars {
        text[..max_chars].to_string()
    } else {
        text.to_string()
    }
}

/// Clean JSON response by removing markdown code blocks
///
/// # Examples
/// ```
/// use rag_chat::services::enrichment_utils::clean_json_response;
/// let response = "```json\n{\"key\": \"value\"}\n```";
/// let cleaned = clean_json_response(response);
/// assert_eq!(cleaned, "{\"key\": \"value\"}");
/// ```
pub fn clean_json_response(response: &str) -> String {
    response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim()
        .to_string()
}

/// Extract author from entities (first person in persons array)
pub fn extract_author_from_entities(entities: &Value) -> Option<String> {
    entities["persons"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Merge entities from source into target, deduplicating values
///
/// # Arguments
/// * `target` - Mutable reference to target entity object
/// * `source` - Source entity object to merge
pub fn merge_entities(target: &mut Value, source: &Value) {
    if let (Some(target_obj), Some(source_obj)) = (target.as_object_mut(), source.as_object()) {
        for (key, source_array) in source_obj {
            if let Some(source_items) = source_array.as_array() {
                let target_array = target_obj.entry(key.clone()).or_insert_with(|| json!([]));

                if let Some(target_items) = target_array.as_array_mut() {
                    for item in source_items {
                        if let Some(s) = item.as_str() {
                            // Only add if not already present (deduplicate)
                            if !target_items.iter().any(|v| v.as_str() == Some(s)) {
                                target_items.push(json!(s));
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Check if document structure indicates it has tables
pub fn has_tables(table_count: i32) -> bool {
    table_count > 0
}

/// Check if document structure indicates it has figures
pub fn has_figures(figure_count: i32) -> bool {
    figure_count > 0
}

/// Check if document content contains formulas
pub fn has_formulas(content: &str) -> bool {
    content.contains("$$") || content.contains("\\[")
}

/// Collect unique element types from document structure
pub fn collect_element_types(elements: &[String]) -> Vec<String> {
    elements
        .iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .cloned()
        .collect()
}

/// Calculate extraction quality confidence score
///
/// # Arguments
/// * `has_content` - If document has page count > 0
/// * `has_structure` - If document structure was extracted
///
/// # Returns
/// Confidence score between 0.0 and 1.0
pub fn calculate_confidence_score(has_content: bool, has_structure: bool) -> f32 {
    if has_content && has_structure {
        0.9
    } else {
        0.6
    }
}

/// Calculate extraction quality completeness score
pub fn calculate_completeness_score(
    has_content: bool,
    has_structure: bool,
    has_metadata: bool,
) -> f32 {
    if has_content && has_structure && has_metadata {
        0.95
    } else {
        0.7
    }
}

/// Generate a deterministic UUID for a category based on its name
///
/// This ensures the same category name always produces the same UUID,
/// useful for consistent category identification.
///
/// # Examples
/// ```
/// use rag_chat::services::enrichment_utils::generate_category_uuid;
/// let uuid1 = generate_category_uuid("Technology");
/// let uuid2 = generate_category_uuid("Technology");
/// assert_eq!(uuid1, uuid2);
/// ```
pub fn generate_category_uuid(category_name: &str) -> Uuid {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    category_name.hash(&mut hasher);
    let hash = hasher.finish();

    // Create a deterministic UUID from the hash
    let bytes: [u8; 16] = [
        ((hash >> 56) & 0xFF) as u8,
        ((hash >> 48) & 0xFF) as u8,
        ((hash >> 40) & 0xFF) as u8,
        ((hash >> 32) & 0xFF) as u8,
        ((hash >> 24) & 0xFF) as u8,
        ((hash >> 16) & 0xFF) as u8,
        ((hash >> 8) & 0xFF) as u8,
        (hash & 0xFF) as u8,
        ((hash >> 56) & 0xFF) as u8,
        ((hash >> 48) & 0xFF) as u8,
        ((hash >> 40) & 0xFF) as u8,
        ((hash >> 32) & 0xFF) as u8,
        ((hash >> 24) & 0xFF) as u8,
        ((hash >> 16) & 0xFF) as u8,
        ((hash >> 8) & 0xFF) as u8,
        (hash & 0xFF) as u8,
    ];

    Uuid::from_bytes(bytes)
}

/// Extract string value from JSON object at specified path
pub fn extract_json_string(json: &Value, path: &[&str]) -> Option<String> {
    let mut current = json;
    for key in path {
        current = &current[key];
    }
    current.as_str().map(String::from)
}

/// Calculate text word count
pub fn calculate_word_count(text: &str) -> i32 {
    text.split_whitespace().count() as i32
}

/// Batch text into chunks with size limit
///
/// # Arguments
/// * `sentences` - List of sentences to batch
/// * `target_batch_size` - Target character count per batch
/// * `max_batches` - Maximum number of batches to create
pub fn batch_text(
    sentences: Vec<String>,
    target_batch_size: usize,
    max_batches: usize,
) -> Vec<String> {
    let mut batches = Vec::new();
    let mut batch = String::new();

    for sentence in sentences {
        // Skip very short sentences (likely fragments)
        if sentence.len() < 10 {
            continue;
        }

        // Add to batch
        if !batch.is_empty() {
            batch.push(' ');
        }
        batch.push_str(&sentence);

        // When batch reaches target size, save it and start new one
        if batch.len() >= target_batch_size {
            if batches.len() < max_batches {
                batches.push(batch.clone());
                batch.clear();
            } else {
                // If we've hit max batches, append remaining to last batch
                if let Some(last) = batches.last_mut() {
                    last.push(' ');
                    last.push_str(&batch);
                }
                batch.clear();
            }
        }
    }

    // Add any remaining content to batches
    if !batch.is_empty() {
        if batches.len() < max_batches {
            batches.push(batch);
        } else if let Some(last) = batches.last_mut() {
            last.push(' ');
            last.push_str(&batch);
        }
    }

    batches
}

/// Ensure JSON entity object has all required fields with empty arrays
pub fn ensure_entity_fields(json: &Value) -> Value {
    let mut entities = json!({
        "persons": [],
        "organizations": [],
        "products": [],
        "locations": [],
        "concepts": [],
        "questions": []
    });

    if let Some(obj) = json.as_object() {
        if let Some(entities_obj) = entities.as_object_mut() {
            for (key, value) in obj {
                if entities_obj.contains_key(key) {
                    if let Some(arr) = value.as_array() {
                        entities_obj.insert(key.clone(), json!(arr));
                    }
                }
            }
        }
    }

    entities
}

/// Sanitize filename for Docling to handle special characters
/// Replaces problematic Unicode characters and ensures .txt -> .md conversion
/// Converts to ASCII where possible for maximum compatibility.
pub fn sanitize_filename_for_docling(filename: &str) -> String {
    let mut sanitized = filename.to_string();

    // Convert .txt to .md (Docling doesn't support .txt)
    if sanitized.ends_with(".txt") {
        sanitized = format!("{}.md", sanitized.trim_end_matches(".txt"));
    }

    // Replace problematic Unicode characters that cause Docling failures
    // Em-dash variants
    sanitized = sanitized.replace('\u{2014}', "-"); // U+2014 em-dash
    sanitized = sanitized.replace('\u{2013}', "-"); // U+2013 en-dash
    sanitized = sanitized.replace('\u{2015}', "-"); // U+2015 horizontal bar

    // Quotes
    sanitized = sanitized.replace('\u{201C}', "\""); // U+201C left double quote
    sanitized = sanitized.replace('\u{201D}', "\""); // U+201D right double quote
    sanitized = sanitized.replace('\u{2018}', "'"); // U+2018 left single quote
    sanitized = sanitized.replace('\u{2019}', "'"); // U+2019 right single quote

    // Other problematic characters
    sanitized = sanitized.replace('|', "_"); // Pipe character
    sanitized = sanitized.replace(':', "_"); // Colon (problematic on some systems)
    sanitized = sanitized.replace('?', ""); // Question mark
    sanitized = sanitized.replace('*', ""); // Asterisk
    sanitized = sanitized.replace('<', ""); // Less than
    sanitized = sanitized.replace('>', ""); // Greater than
    sanitized = sanitized.replace('/', "_"); // Forward slash
    sanitized = sanitized.replace('\\', "_"); // Backslash

    // Additional problematic characters from various encodings
    sanitized = sanitized.replace('\u{00A0}', " "); // Non-breaking space
    sanitized = sanitized.replace('\u{202F}', " "); // Narrow no-break space

    // Aggressive ASCII conversion: keep only alphanumeric, dots, dashes, and underscores
    let mut final_sanitized = String::with_capacity(sanitized.len());
    for c in sanitized.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' {
            final_sanitized.push(c);
        } else if c.is_whitespace() {
            final_sanitized.push('_');
        } else {
            // Skip other characters or replace with underscore
            final_sanitized.push('_');
        }
    }
    sanitized = final_sanitized;

    // Replace multiple underscores with a single one
    while sanitized.contains("__") {
        sanitized = sanitized.replace("__", "_");
    }

    // Replace leading/trailing spaces and dots
    sanitized = sanitized
        .trim_matches(|c: char| c == '.' || c == '-' || c == '_')
        .to_string();

    // Ensure filename is not empty after sanitization
    if sanitized.is_empty() {
        sanitized = "document.pdf".to_string();
    } else {
        // Restore extension if it was lost or mangled
        if !sanitized.contains('.') {
            sanitized.push_str(".pdf");
        }
    }

    sanitized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_keywords_from_string() {
        let keywords = "rust, database, vector search".to_string();
        let parsed = parse_keywords_from_string(&keywords);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0], "rust");
        assert_eq!(parsed[1], "database");
    }

    #[test]
    fn test_parse_keywords_filters_long_keywords() {
        let keywords =
            "rust, a_very_long_keyword_that_exceeds_fifty_characters_limit_x".to_string();
        let parsed = parse_keywords_from_string(&keywords);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0], "rust");
    }

    #[test]
    fn test_parse_keywords_limits_to_eight() {
        let keywords = "k1, k2, k3, k4, k5, k6, k7, k8, k9, k10".to_string();
        let parsed = parse_keywords_from_string(&keywords);
        assert_eq!(parsed.len(), 8);
    }

    #[test]
    fn test_collect_context() {
        let sentences = vec![
            "Hello world.".to_string(),
            "This is a test.".to_string(),
            "More content here.".to_string(),
        ];
        let context = collect_context(&sentences, 30);
        assert!(context.contains("Hello"));
        assert!(context.contains("test"));
    }

    #[test]
    fn test_extract_preview() {
        let text = "This is a long text with many characters";
        let preview = extract_preview(text, 10);
        assert_eq!(preview, "This is a ");
    }

    #[test]
    fn test_clean_json_response() {
        let response = "```json\n{\"key\": \"value\"}\n```";
        let cleaned = clean_json_response(response);
        assert_eq!(cleaned, "{\"key\": \"value\"}");
    }

    #[test]
    fn test_clean_json_response_no_markdown() {
        let response = "{\"key\": \"value\"}";
        let cleaned = clean_json_response(response);
        assert_eq!(cleaned, "{\"key\": \"value\"}");
    }

    #[test]
    fn test_extract_author_from_entities() {
        let entities = json!({
            "persons": ["Alice", "Bob"],
            "organizations": []
        });
        let author = extract_author_from_entities(&entities);
        assert_eq!(author, Some("Alice".to_string()));
    }

    #[test]
    fn test_extract_author_empty_persons() {
        let entities = json!({
            "persons": [],
            "organizations": []
        });
        let author = extract_author_from_entities(&entities);
        assert_eq!(author, None);
    }

    #[test]
    fn test_merge_entities() {
        let mut target = json!({
            "persons": ["Alice"],
            "organizations": []
        });
        let source = json!({
            "persons": ["Bob", "Alice"],
            "organizations": ["Acme"]
        });

        merge_entities(&mut target, &source);

        let persons = target["persons"].as_array().unwrap();
        assert_eq!(persons.len(), 2); // Alice and Bob, deduplicated
    }

    #[test]
    fn test_has_tables() {
        assert!(has_tables(1));
        assert!(!has_tables(0));
    }

    #[test]
    fn test_has_figures() {
        assert!(has_figures(5));
        assert!(!has_figures(0));
    }

    #[test]
    fn test_has_formulas() {
        assert!(has_formulas("Some text $$ formula $$"));
        assert!(has_formulas("Math \\[ expression \\]"));
        assert!(!has_formulas("Just plain text"));
    }

    #[test]
    fn test_calculate_confidence_score() {
        assert_eq!(calculate_confidence_score(true, true), 0.9);
        assert_eq!(calculate_confidence_score(true, false), 0.6);
        assert_eq!(calculate_confidence_score(false, true), 0.6);
    }

    #[test]
    fn test_calculate_completeness_score() {
        assert_eq!(calculate_completeness_score(true, true, true), 0.95);
        assert_eq!(calculate_completeness_score(true, true, false), 0.7);
        assert_eq!(calculate_completeness_score(false, true, true), 0.7);
    }

    #[test]
    fn test_generate_category_uuid() {
        let uuid1 = generate_category_uuid("Technology");
        let uuid2 = generate_category_uuid("Technology");
        assert_eq!(uuid1, uuid2);

        let uuid3 = generate_category_uuid("Science");
        assert_ne!(uuid1, uuid3);
    }

    #[test]
    fn test_calculate_word_count() {
        let text = "Hello world this is a test";
        assert_eq!(calculate_word_count(text), 6);
    }

    #[test]
    fn test_batch_text() {
        let sentences = vec![
            "Sentence one.".to_string(),
            "Sentence two.".to_string(),
            "Sentence three.".to_string(),
        ];

        let batches = batch_text(sentences, 20, 2);
        assert!(batches.len() <= 2);
    }

    #[test]
    fn test_batch_text_skips_short_sentences() {
        let sentences = vec![
            "Short.".to_string(),
            "This is a longer sentence.".to_string(),
            "Hi.".to_string(),
        ];

        let batches = batch_text(sentences, 50, 5);
        // Should only include the longer sentence
        assert_eq!(batches.len(), 1);
    }

    #[test]
    fn test_ensure_entity_fields() {
        let input = json!({ "persons": ["Alice"] });
        let result = ensure_entity_fields(&input);

        assert!(result["persons"].is_array());
        assert!(result["organizations"].is_array());
        assert!(result["concepts"].is_array());
    }

    #[test]
    fn test_sanitize_filename_for_docling_em_dash() {
        let input = "Edler achtfacher Pfad – Wikipedia.pdf";
        let result = sanitize_filename_for_docling(input);
        assert_eq!(result, "Edler_achtfacher_Pfad_-_Wikipedia.pdf");
    }

    #[test]
    fn test_sanitize_filename_for_docling_pipe() {
        let input = "The Work You Do | The New Yorker.pdf";
        let result = sanitize_filename_for_docling(input);
        assert_eq!(result, "The_Work_You_Do_The_New_Yorker.pdf");
    }

    #[test]
    fn test_sanitize_filename_for_docling_txt_to_md() {
        let input = "document.txt";
        let result = sanitize_filename_for_docling(input);
        assert_eq!(result, "document.md");
    }

    #[test]
    fn test_sanitize_filename_for_docling_unicode_quotes() {
        let input = "I Love My Wife — Bingqian G.pdf";
        let result = sanitize_filename_for_docling(input);
        assert_eq!(result, "I_Love_My_Wife_-_Bingqian_G.pdf");
    }
}
