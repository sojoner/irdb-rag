use crate::domain::query_builder_types::FilterCondition;

/// Compiles FilterCondition trees into SQL WHERE clauses
pub struct QueryCompiler;

impl QueryCompiler {
    /// Compile a FilterCondition into a SQL WHERE clause
    /// Returns the SQL string that can be used in a WHERE clause
    pub fn compile_where_clause(condition: &FilterCondition) -> String {
        Self::compile_condition(condition)
    }

    fn compile_condition(condition: &FilterCondition) -> String {
        match condition {
            FilterCondition::And(conditions) => {
                let parts: Vec<String> = conditions
                    .iter()
                    .map(Self::compile_condition)
                    .collect();
                format!("({})", parts.join(" AND "))
            }
            FilterCondition::Or(conditions) => {
                let parts: Vec<String> = conditions
                    .iter()
                    .map(Self::compile_condition)
                    .collect();
                format!("({})", parts.join(" OR "))
            }
            FilterCondition::Not(inner) => {
                let inner_sql = Self::compile_condition(inner);
                format!("NOT {}", inner_sql)
            }
            FilterCondition::Equals { field, value } => {
                Self::compile_equals(field, value)
            }
            FilterCondition::Contains { field, value } => {
                Self::compile_contains(field, value)
            }
            FilterCondition::Range { field, min, max } => {
                Self::compile_range(field, *min, *max)
            }
            FilterCondition::DateRange { field, min, max } => {
                Self::compile_date_range(field, min, max)
            }
        }
    }

    /// Compile Equals condition
    /// For JSONB fields, generates: metadata->>'field' = 'value'
    fn compile_equals(field: &str, value: &str) -> String {
        let escaped_value = Self::escape_sql_literal(value);
        format!("metadata->>'{}' = '{}'", field, escaped_value)
    }

    /// Compile Contains condition using full-text search
    /// For JSONB fields, generates: metadata->>'field' ILIKE '%value%'
    fn compile_contains(field: &str, value: &str) -> String {
        let escaped_value = Self::escape_sql_literal(value);
        format!("metadata->>'{}' ILIKE '%{}%'", field, escaped_value)
    }

    /// Compile Range condition for numeric values
    /// For JSONB fields, generates: (metadata->>'field')::numeric BETWEEN min AND max
    fn compile_range(field: &str, min: Option<f64>, max: Option<f64>) -> String {
        match (min, max) {
            (Some(min_val), Some(max_val)) => {
                format!(
                    "(metadata->>'{}')::numeric BETWEEN {} AND {}",
                    field, min_val, max_val
                )
            }
            (Some(min_val), None) => {
                format!("(metadata->>'{}')::numeric >= {}", field, min_val)
            }
            (None, Some(max_val)) => {
                format!("(metadata->>'{}')::numeric <= {}", field, max_val)
            }
            (None, None) => "1=1".to_string(), // No constraint
        }
    }

    /// Compile DateRange condition
    /// For JSONB fields, generates: (metadata->>'field')::date BETWEEN min AND max
    fn compile_date_range(field: &str, min: &Option<String>, max: &Option<String>) -> String {
        match (min, max) {
            (Some(min_date), Some(max_date)) => {
                let escaped_min = Self::escape_sql_literal(min_date);
                let escaped_max = Self::escape_sql_literal(max_date);
                format!(
                    "(metadata->>'{}')::date BETWEEN '{}' AND '{}'",
                    field, escaped_min, escaped_max
                )
            }
            (Some(min_date), None) => {
                let escaped_min = Self::escape_sql_literal(min_date);
                format!("(metadata->>'{}')::date >= '{}'", field, escaped_min)
            }
            (None, Some(max_date)) => {
                let escaped_max = Self::escape_sql_literal(max_date);
                format!("(metadata->>'{}')::date <= '{}'", field, escaped_max)
            }
            (None, None) => "1=1".to_string(), // No constraint
        }
    }

    /// Escape SQL literal by doubling single quotes
    fn escape_sql_literal(value: &str) -> String {
        value.replace("'", "''")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compile_equals() {
        let condition = FilterCondition::equals("persons", "John Smith");
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert_eq!(sql, "metadata->>'persons' = 'John Smith'");
    }

    #[test]
    fn test_compile_equals_with_quote() {
        let condition = FilterCondition::equals("persons", "O'Brien");
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert_eq!(sql, "metadata->>'persons' = 'O''Brien'");
    }

    #[test]
    fn test_compile_contains() {
        let condition = FilterCondition::contains("persons", "john");
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert_eq!(sql, "metadata->>'persons' ILIKE '%john%'");
    }

    #[test]
    fn test_compile_range_both_bounds() {
        let condition = FilterCondition::range("age", Some(18.0), Some(65.0));
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert_eq!(sql, "(metadata->>'age')::numeric BETWEEN 18 AND 65");
    }

    #[test]
    fn test_compile_range_min_only() {
        let condition = FilterCondition::range("age", Some(18.0), None);
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert_eq!(sql, "(metadata->>'age')::numeric >= 18");
    }

    #[test]
    fn test_compile_range_max_only() {
        let condition = FilterCondition::range("age", None, Some(65.0));
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert_eq!(sql, "(metadata->>'age')::numeric <= 65");
    }

    #[test]
    fn test_compile_range_no_bounds() {
        let condition = FilterCondition::range("age", None, None);
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert_eq!(sql, "1=1");
    }

    #[test]
    fn test_compile_date_range_both_bounds() {
        let condition = FilterCondition::date_range(
            "date",
            Some("2023-01-01".to_string()),
            Some("2023-12-31".to_string()),
        );
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert!(sql.contains("BETWEEN '2023-01-01' AND '2023-12-31'"));
    }

    #[test]
    fn test_compile_and_conditions() {
        let conditions = vec![
            FilterCondition::equals("persons", "John"),
            FilterCondition::equals("locations", "London"),
        ];
        let and_condition = FilterCondition::and(conditions);
        let sql = QueryCompiler::compile_where_clause(&and_condition);
        assert!(sql.contains("AND"));
        assert!(sql.contains("persons"));
        assert!(sql.contains("locations"));
    }

    #[test]
    fn test_compile_or_conditions() {
        let conditions = vec![
            FilterCondition::equals("persons", "John"),
            FilterCondition::equals("persons", "Jane"),
        ];
        let or_condition = FilterCondition::or(conditions);
        let sql = QueryCompiler::compile_where_clause(&or_condition);
        assert!(sql.contains("OR"));
        assert!(sql.contains("persons"));
    }

    #[test]
    fn test_compile_not_condition() {
        let condition = FilterCondition::not(FilterCondition::equals("persons", "John"));
        let sql = QueryCompiler::compile_where_clause(&condition);
        assert!(sql.contains("NOT"));
        assert!(sql.contains("persons"));
        assert!(sql.contains("John"));
    }

    #[test]
    fn test_compile_complex_nested() {
        let inner_or = FilterCondition::or(vec![
            FilterCondition::equals("persons", "John"),
            FilterCondition::equals("persons", "Jane"),
        ]);
        let outer_and = FilterCondition::and(vec![
            inner_or,
            FilterCondition::equals("locations", "London"),
        ]);
        let sql = QueryCompiler::compile_where_clause(&outer_and);
        assert!(sql.contains("AND"));
        assert!(sql.contains("OR"));
        assert!(sql.contains("persons"));
        assert!(sql.contains("locations"));
    }

    #[test]
    fn test_escape_sql_literal() {
        let input = "O'Brien's House";
        let escaped = QueryCompiler::escape_sql_literal(input);
        assert_eq!(escaped, "O''Brien''s House");
    }
}
