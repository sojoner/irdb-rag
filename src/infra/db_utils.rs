//! Pure functions for database query construction and filtering
//!
//! This module extracts deterministic, side-effect-free functions used in database operations.
//! Functions here focus on query building, filter validation, and data transformation.

use crate::infra::db::SearchFilters;
use uuid::Uuid;

/// Sanitize BM25 query to handle empty or problematic inputs
///
/// ParadeDB's pg_search has edge cases with empty/wildcard queries that cause parsing errors.
/// This function ensures only valid queries are sent.
///
/// # Examples
/// ```
/// use rag_chat::infra::db_utils::sanitize_bm25_query;
/// assert_eq!(sanitize_bm25_query(""), "id:__no_match__");
/// assert_eq!(sanitize_bm25_query("*"), "id:__no_match__");
/// assert_eq!(sanitize_bm25_query("id:()"), "id:__no_match__");
/// assert_eq!(sanitize_bm25_query("hello"), "hello");
/// ```
pub fn sanitize_bm25_query(query: &str) -> &str {
    let trimmed = query.trim();

    // Check for empty ID queries like "id:()", "id: ()", "id:(*)"
    let is_empty_id = if let Some(stripped) = trimmed.strip_prefix("id:") {
        let rest = stripped.trim();
        if rest.starts_with('(') && rest.ends_with(')') {
            let inside = rest[1..rest.len() - 1].trim();
            inside.is_empty() || inside == "*" || inside == "**"
        } else {
            false
        }
    } else {
        false
    };

    if trimmed.is_empty() || trimmed == "*" || is_empty_id {
        "id:__no_match__"
    } else {
        query
    }
}

/// Convert embedding vector to PostgreSQL vector format string
///
/// # Examples
/// ```
/// use rag_chat::infra::db_utils::embedding_to_string;
/// let embedding = vec![0.1, 0.2, 0.3];
/// assert_eq!(embedding_to_string(&embedding), "[0.1,0.2,0.3]");
/// ```
pub fn embedding_to_string(embedding: &[f32]) -> String {
    format!(
        "[{}]",
        embedding
            .iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    )
}

/// Check if search filters have entity or word count filters
///
/// These filters require fetching full documents for in-memory filtering,
/// so this function helps determine if that's necessary.
pub fn has_entity_or_wordcount_filters(filters: &SearchFilters) -> bool {
    filters.authors.is_some()
        || filters.concepts.is_some()
        || filters.organizations.is_some()
        || filters.persons.is_some()
        || filters.products.is_some()
        || filters.locations.is_some()
        || filters.keywords.is_some()
        || filters.word_count_min.is_some()
        || filters.word_count_max.is_some()
}

/// Filter documents by author
///
/// Returns true if document should be kept based on author filter
pub fn matches_author_filter(author: Option<&str>, filter_authors: &[String]) -> bool {
    if let Some(doc_author) = author {
        filter_authors.iter().any(|a| a == doc_author)
    } else {
        false
    }
}

/// Filter documents by word count range
///
/// Returns true if word count falls within the specified range
pub fn matches_word_count_filter(
    word_count: Option<i32>,
    min: Option<i32>,
    max: Option<i32>,
) -> bool {
    match word_count {
        Some(wc) => {
            let above_min = min.is_none_or(|m| wc >= m);
            let below_max = max.is_none_or(|m| wc <= m);
            above_min && below_max
        }
        None => false,
    }
}

/// Check if entity array contains any of the filter values
///
/// Used for filtering by persons, organizations, products, concepts
pub fn entity_array_matches(
    entity_array: Option<&[String]>,
    filter_values: &[String],
) -> bool {
    if let Some(entities) = entity_array {
        filter_values.iter().any(|f| entities.contains(f))
    } else {
        false
    }
}

/// Calculate limit adjustment for hybrid search
///
/// Since we apply post-filtering, we need to fetch more results initially
/// to compensate for filtering losses.
///
/// # Arguments
/// * `requested_limit` - The number of results the user wants
/// * `filter_strictness` - How many entity/word count filters are active (0-6)
///
/// # Examples
/// ```
/// use rag_chat::infra::db_utils::calculate_search_limit;
/// assert_eq!(calculate_search_limit(10, 0), 30); // No filters: 3x
/// assert_eq!(calculate_search_limit(10, 1), 30); // 1 filter: 3x
/// assert_eq!(calculate_search_limit(10, 3), 40); // 3 filters: 4x
/// ```
pub fn calculate_search_limit(requested_limit: i32, filter_strictness: usize) -> i32 {
    // Start with base 3x multiplier
    let base_multiplier = 3;

    // For each filter, increase multiplier slightly
    let adjusted_multiplier = if filter_strictness > 0 {
        base_multiplier + (filter_strictness as i32) / 2
    } else {
        base_multiplier
    };

    requested_limit * adjusted_multiplier
}

/// Extract unique document IDs maintaining insertion order
///
/// Used to convert search results into a list of unique document IDs
/// while preserving the relevance order from search results.
///
/// # Examples
/// ```
/// use uuid::Uuid;
/// use rag_chat::infra::db_utils::extract_unique_ids;
/// let ids = vec![Uuid::nil(), Uuid::nil(), Uuid::max()];
/// let unique = extract_unique_ids(&ids);
/// assert_eq!(unique.len(), 2);
/// ```
pub fn extract_unique_ids(ids: &[Uuid]) -> Vec<Uuid> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();

    for id in ids {
        if seen.insert(*id) {
            unique.push(*id);
        }
    }

    unique
}

/// Normalize and deduplicate a keyword list
///
/// Used for keyword extraction results to ensure clean output
pub fn normalize_keywords(raw_keywords: Vec<String>) -> Vec<String> {
    raw_keywords
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.len() < 50)
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
}

/// Count how many filter types are active
///
/// This helps determine filter strictness for search result adjustment
pub fn count_active_filters(filters: &SearchFilters) -> usize {
    let mut count = 0;
    if filters.category_id.is_some() {
        count += 1;
    }
    if filters.date_from.is_some() || filters.date_to.is_some() {
        count += 1;
    }
    if filters.locations.is_some() {
        count += 1;
    }
    if filters.keywords.is_some() {
        count += 1;
    }
    if filters.source_types.is_some() {
        count += 1;
    }
    if filters.authors.is_some() {
        count += 1;
    }
    if filters.concepts.is_some() {
        count += 1;
    }
    if filters.organizations.is_some() {
        count += 1;
    }
    if filters.persons.is_some() {
        count += 1;
    }
    if filters.products.is_some() {
        count += 1;
    }
    if filters.word_count_min.is_some() || filters.word_count_max.is_some() {
        count += 1;
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_empty_query() {
        assert_eq!(sanitize_bm25_query(""), "id:__no_match__");
        assert_eq!(sanitize_bm25_query("   "), "id:__no_match__");
    }

    #[test]
    fn test_sanitize_wildcard_query() {
        assert_eq!(sanitize_bm25_query("*"), "id:__no_match__");
    }

    #[test]
    fn test_sanitize_empty_id_queries() {
        assert_eq!(sanitize_bm25_query("id:()"), "id:__no_match__");
        assert_eq!(sanitize_bm25_query("id: ()"), "id:__no_match__");
        assert_eq!(sanitize_bm25_query("id:(*)"), "id:__no_match__");
        assert_eq!(sanitize_bm25_query("id:(**)"), "id:__no_match__");
    }

    #[test]
    fn test_sanitize_valid_queries() {
        assert_eq!(sanitize_bm25_query("hello"), "hello");
        assert_eq!(sanitize_bm25_query("  hello world  "), "  hello world  ");
        assert_eq!(sanitize_bm25_query("id:(123)"), "id:(123)");
    }

    #[test]
    fn test_embedding_to_string() {
        let embedding = vec![0.1, 0.2, 0.3];
        let result = embedding_to_string(&embedding);
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
        assert!(result.contains("0.1"));
    }

    #[test]
    fn test_embedding_to_string_empty() {
        let embedding: Vec<f32> = vec![];
        assert_eq!(embedding_to_string(&embedding), "[]");
    }

    #[test]
    fn test_has_entity_filters() {
        let filters = SearchFilters {
            category_id: None,
            date_from: None,
            date_to: None,
            locations: None,
            keywords: None,
            source_types: None,
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: None,
            word_count_max: None,
        };

        assert!(!has_entity_or_wordcount_filters(&filters));

        let mut filters_with_authors = filters.clone();
        filters_with_authors.authors = Some(vec!["Alice".to_string()]);
        assert!(has_entity_or_wordcount_filters(&filters_with_authors));
    }

    #[test]
    fn test_has_wordcount_filters() {
        let filters = SearchFilters {
            category_id: None,
            date_from: None,
            date_to: None,
            locations: None,
            keywords: None,
            source_types: None,
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: Some(100),
            word_count_max: None,
        };

        assert!(has_entity_or_wordcount_filters(&filters));
    }

    #[test]
    fn test_matches_author_filter() {
        assert!(matches_author_filter(Some("Alice"), &["Alice".to_string()]));
        assert!(matches_author_filter(
            Some("Alice"),
            &["Alice".to_string(), "Bob".to_string()]
        ));
        assert!(!matches_author_filter(Some("Alice"), &["Bob".to_string()]));
        assert!(!matches_author_filter(None, &["Alice".to_string()]));
    }

    #[test]
    fn test_matches_word_count_filter() {
        assert!(matches_word_count_filter(Some(500), Some(100), Some(1000)));
        assert!(!matches_word_count_filter(Some(50), Some(100), Some(1000)));
        assert!(!matches_word_count_filter(Some(1500), Some(100), Some(1000)));
        assert!(matches_word_count_filter(Some(500), None, Some(1000)));
        assert!(matches_word_count_filter(Some(500), Some(100), None));
        assert!(!matches_word_count_filter(None, Some(100), Some(1000)));
    }

    #[test]
    fn test_entity_array_matches() {
        let entities = vec!["Alice".to_string(), "Bob".to_string()];
        assert!(entity_array_matches(Some(&entities), &["Alice".to_string()]));
        assert!(!entity_array_matches(Some(&entities), &["Charlie".to_string()]));
        assert!(!entity_array_matches(None, &["Alice".to_string()]));
    }

    #[test]
    fn test_calculate_search_limit() {
        // Base multiplier is 3, plus (filter_count / 2) for strictness
        assert_eq!(calculate_search_limit(10, 0), 30); // 10 * 3
        assert_eq!(calculate_search_limit(10, 1), 30); // 10 * (3 + 1/2) = 10 * 3
        assert_eq!(calculate_search_limit(10, 2), 40); // 10 * (3 + 2/2) = 10 * 4
        assert_eq!(calculate_search_limit(10, 3), 40); // 10 * (3 + 3/2) = 10 * 4
        assert_eq!(calculate_search_limit(10, 4), 50); // 10 * (3 + 4/2) = 10 * 5
    }

    #[test]
    fn test_extract_unique_ids() {
        let uuid1 = Uuid::nil();
        let uuid3 = Uuid::max();

        let ids = vec![uuid1, uuid1, uuid3, uuid1];
        let unique = extract_unique_ids(&ids);

        assert_eq!(unique.len(), 2);
        assert_eq!(unique[0], uuid1);
        assert_eq!(unique[1], uuid3);
    }

    #[test]
    fn test_normalize_keywords() {
        let keywords = vec![
            "rust".to_string(),
            "  database  ".to_string(),
            "".to_string(),
            "rust".to_string(), // duplicate
            "a_very_long_keyword_that_exceeds_fifty_characters_limit_x"
                .to_string(),
        ];

        let normalized = normalize_keywords(keywords);
        assert_eq!(normalized.len(), 2); // Only 'rust' and 'database'
    }

    #[test]
    fn test_count_active_filters() {
        let filters = SearchFilters {
            category_id: None,
            date_from: None,
            date_to: None,
            locations: None,
            keywords: None,
            source_types: None,
            authors: None,
            concepts: None,
            organizations: None,
            persons: None,
            products: None,
            word_count_min: None,
            word_count_max: None,
        };

        assert_eq!(count_active_filters(&filters), 0);

        let mut filters_with_authors = filters.clone();
        filters_with_authors.authors = Some(vec!["Alice".to_string()]);
        assert_eq!(count_active_filters(&filters_with_authors), 1);

        let mut filters_with_dates = filters.clone();
        filters_with_dates.date_from = Some(chrono::Utc::now());
        assert_eq!(count_active_filters(&filters_with_dates), 1);

        let mut filters_with_multiple = filters.clone();
        filters_with_multiple.authors = Some(vec!["Alice".to_string()]);
        filters_with_multiple.keywords = Some(vec!["test".to_string()]);
        filters_with_multiple.concepts = Some(vec!["concept".to_string()]);
        assert_eq!(count_active_filters(&filters_with_multiple), 3);
    }
}
