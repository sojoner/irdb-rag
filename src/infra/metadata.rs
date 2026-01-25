use sqlx::PgPool;
use crate::domain::models::{FieldMetadata, FieldType, FieldValueAutocomplete};
use crate::domain::dtos::FieldValueRequest;

/// Discover all metadata fields in the documents table
pub async fn discover_fields(pool: &PgPool) -> Result<Vec<FieldMetadata>, sqlx::Error> {
    // Query JSONB metadata to find all unique keys
    let rows: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT
            jsonb_object_keys(metadata) AS field_name,
            COUNT(DISTINCT metadata->field_name) AS unique_count
        FROM documents
        WHERE metadata IS NOT NULL
        GROUP BY field_name
        ORDER BY field_name
        "#
    )
    .fetch_all(pool)
    .await?;

    let fields = rows
        .into_iter()
        .map(|(field_name, unique_count)| {
            let display_name = field_name
                .split('_')
                .map(|word| {
                    let mut chars = word.chars();
                    match chars.next() {
                        None => String::new(),
                        Some(first) => {
                            let mut result = first.to_uppercase().to_string();
                            result.push_str(chars.as_str());
                            result
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join(" ");

            FieldMetadata {
                name: field_name,
                display_name,
                field_type: FieldType::Text,
                total_unique_values: unique_count,
            }
        })
        .collect();

    Ok(fields)
}

/// Autocomplete field values using BM25 full-text search
pub async fn discover_field_values(
    pool: &PgPool,
    field: &str,
    query: &str,
    limit: usize,
) -> Result<Vec<(String, i64)>, sqlx::Error> {
    // Use ILIKE search on JSONB field values for autocomplete
    let search_query = format!("%{}%", query.replace("%", "\\%"));

    let rows: Vec<(Option<String>, i64)> = sqlx::query_as(
        &format!(
            r#"
            SELECT
                metadata->>'{}' AS value,
                COUNT(*) AS doc_count
            FROM documents
            WHERE metadata IS NOT NULL
                AND metadata->>'{}'  IS NOT NULL
                AND (metadata->>'{}' ILIKE $1 OR $1 = '')
            GROUP BY metadata->>'{}'
            ORDER BY doc_count DESC, metadata->>'{}' ASC
            LIMIT $2
            "#,
            field, field, field, field, field
        )
    )
    .bind(search_query)
    .bind(limit as i64)
    .fetch_all(pool)
    .await?;

    let values = rows
        .into_iter()
        .filter_map(|(value, doc_count)| {
            value.map(|val| (val, doc_count))
        })
        .collect();

    Ok(values)
}

/// Wrapper for QueryBuilder component: Get all metadata fields
pub async fn get_metadata_fields(pool: &PgPool) -> Result<Vec<FieldMetadata>, sqlx::Error> {
    discover_fields(pool).await
}

/// Wrapper for QueryBuilder component: Autocomplete field values
pub async fn get_field_value_autocomplete(
    pool: &PgPool,
    request: FieldValueRequest,
) -> Result<FieldValueAutocomplete, sqlx::Error> {
    let values = discover_field_values(pool, &request.field, &request.query, request.limit).await?;

    Ok(FieldValueAutocomplete {
        field: request.field,
        query: request.query,
        total_matching: values.len() as i64,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn test_discover_fields_returns_metadata_keys() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect("postgres://rag_user:rag_pass@localhost:5432/rag_chat")
            .await
            .expect("Failed to connect to test database");

        let fields = discover_fields(&pool).await.expect("Failed to discover fields");
        assert!(!fields.is_empty(), "Should discover at least one field");
        assert!(fields.iter().any(|f| f.name == "persons" || f.name == "locations"),
            "Should discover common metadata fields");
    }

    #[tokio::test]
    #[ignore]
    async fn test_discover_field_values_filters_correctly() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect("postgres://rag_user:rag_pass@localhost:5432/rag_chat")
            .await
            .expect("Failed to connect to test database");

        let values = discover_field_values(&pool, "persons", "john", 10)
            .await
            .expect("Failed to discover field values");

        // Values should be filtered to those containing "john"
        for (value, _count) in values {
            assert!(value.to_lowercase().contains("john"),
                "Value should contain query: {}", value);
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_discover_field_values_limits_results() {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .connect("postgres://rag_user:rag_pass@localhost:5432/rag_chat")
            .await
            .expect("Failed to connect to test database");

        let values = discover_field_values(&pool, "persons", "", 5)
            .await
            .expect("Failed to discover field values");

        assert!(values.len() <= 5, "Should limit results to specified count");
    }
}
