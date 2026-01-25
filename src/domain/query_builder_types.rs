use serde::{Deserialize, Serialize};

// ============================================
// Filter Conditions
// ============================================

/// Represents a single filter condition or a combination of conditions
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "data")]
pub enum FilterCondition {
    /// Multiple conditions combined with AND logic
    And(Vec<FilterCondition>),
    /// Multiple conditions combined with OR logic
    Or(Vec<FilterCondition>),
    /// Negation of a condition
    Not(Box<FilterCondition>),
    /// Exact match on field value
    Equals { field: String, value: String },
    /// Text search/contains match on field value (uses full-text search)
    Contains { field: String, value: String },
    /// Numeric range query
    Range {
        field: String,
        min: Option<f64>,
        max: Option<f64>,
    },
    /// Date range query
    DateRange {
        field: String,
        min: Option<String>, // ISO 8601 date
        max: Option<String>, // ISO 8601 date
    },
}

// ============================================
// Query Builder State
// ============================================

/// Represents the state of a query builder UI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryBuilderState {
    /// List of conditions at the root level
    pub conditions: Vec<FilterCondition>,
    /// Logical operator to combine root conditions
    pub operator: LogicalOperator,
}

impl QueryBuilderState {
    /// Create a new empty query builder state
    pub fn new() -> Self {
        Self {
            conditions: Vec::new(),
            operator: LogicalOperator::And,
        }
    }

    /// Add a condition to the query builder
    pub fn add_condition(&mut self, condition: FilterCondition) {
        self.conditions.push(condition);
    }

    /// Remove a condition by index
    pub fn remove_condition(&mut self, index: usize) {
        if index < self.conditions.len() {
            self.conditions.remove(index);
        }
    }

    /// Convert to a FilterCondition
    pub fn to_filter_condition(&self) -> Option<FilterCondition> {
        match self.conditions.len() {
            0 => None,
            1 => Some(self.conditions[0].clone()),
            _ => match self.operator {
                LogicalOperator::And => Some(FilterCondition::And(self.conditions.clone())),
                LogicalOperator::Or => Some(FilterCondition::Or(self.conditions.clone())),
            },
        }
    }
}

impl Default for QueryBuilderState {
    fn default() -> Self {
        Self::new()
    }
}

/// Top-level logical operator for combining conditions
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "UPPERCASE")]
pub enum LogicalOperator {
    And,
    Or,
}

// ============================================
// Filter Condition Helpers
// ============================================

impl FilterCondition {
    /// Create an And condition from multiple conditions
    pub fn and(conditions: Vec<FilterCondition>) -> Self {
        FilterCondition::And(conditions)
    }

    /// Create an Or condition from multiple conditions
    pub fn or(conditions: Vec<FilterCondition>) -> Self {
        FilterCondition::Or(conditions)
    }

    /// Create a Not condition
    pub fn not(condition: FilterCondition) -> Self {
        FilterCondition::Not(Box::new(condition))
    }

    /// Create an Equals condition
    pub fn equals(field: impl Into<String>, value: impl Into<String>) -> Self {
        FilterCondition::Equals {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a Contains condition
    pub fn contains(field: impl Into<String>, value: impl Into<String>) -> Self {
        FilterCondition::Contains {
            field: field.into(),
            value: value.into(),
        }
    }

    /// Create a Range condition
    pub fn range(field: impl Into<String>, min: Option<f64>, max: Option<f64>) -> Self {
        FilterCondition::Range {
            field: field.into(),
            min,
            max,
        }
    }

    /// Create a DateRange condition
    pub fn date_range(
        field: impl Into<String>,
        min: Option<String>,
        max: Option<String>,
    ) -> Self {
        FilterCondition::DateRange {
            field: field.into(),
            min,
            max,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_condition_and() {
        let conditions = vec![
            FilterCondition::equals("person", "John"),
            FilterCondition::equals("location", "London"),
        ];
        let and_condition = FilterCondition::and(conditions);

        match and_condition {
            FilterCondition::And(conds) => {
                assert_eq!(conds.len(), 2);
            }
            _ => panic!("Expected And condition"),
        }
    }

    #[test]
    fn test_filter_condition_or() {
        let conditions = vec![
            FilterCondition::equals("person", "John"),
            FilterCondition::equals("person", "Jane"),
        ];
        let or_condition = FilterCondition::or(conditions);

        match or_condition {
            FilterCondition::Or(conds) => {
                assert_eq!(conds.len(), 2);
            }
            _ => panic!("Expected Or condition"),
        }
    }

    #[test]
    fn test_filter_condition_not() {
        let condition = FilterCondition::equals("person", "John");
        let not_condition = FilterCondition::not(condition);

        match not_condition {
            FilterCondition::Not(inner) => {
                match *inner {
                    FilterCondition::Equals { ref field, ref value } => {
                        assert_eq!(field, "person");
                        assert_eq!(value, "John");
                    }
                    _ => panic!("Expected Equals inside Not"),
                }
            }
            _ => panic!("Expected Not condition"),
        }
    }

    #[test]
    fn test_query_builder_state_new() {
        let state = QueryBuilderState::new();
        assert!(state.conditions.is_empty());
        assert_eq!(state.operator, LogicalOperator::And);
    }

    #[test]
    fn test_query_builder_add_condition() {
        let mut state = QueryBuilderState::new();
        state.add_condition(FilterCondition::equals("person", "John"));
        assert_eq!(state.conditions.len(), 1);
    }

    #[test]
    fn test_query_builder_to_filter_condition_single() {
        let mut state = QueryBuilderState::new();
        state.add_condition(FilterCondition::equals("person", "John"));

        let condition = state.to_filter_condition();
        match condition {
            Some(FilterCondition::Equals { field, value }) => {
                assert_eq!(field, "person");
                assert_eq!(value, "John");
            }
            _ => panic!("Expected Equals condition"),
        }
    }

    #[test]
    fn test_query_builder_to_filter_condition_multiple_and() {
        let mut state = QueryBuilderState::new();
        state.add_condition(FilterCondition::equals("person", "John"));
        state.add_condition(FilterCondition::equals("location", "London"));

        let condition = state.to_filter_condition();
        match condition {
            Some(FilterCondition::And(conds)) => {
                assert_eq!(conds.len(), 2);
            }
            _ => panic!("Expected And condition"),
        }
    }

    #[test]
    fn test_query_builder_to_filter_condition_multiple_or() {
        let mut state = QueryBuilderState::new();
        state.operator = LogicalOperator::Or;
        state.add_condition(FilterCondition::equals("person", "John"));
        state.add_condition(FilterCondition::equals("person", "Jane"));

        let condition = state.to_filter_condition();
        match condition {
            Some(FilterCondition::Or(conds)) => {
                assert_eq!(conds.len(), 2);
            }
            _ => panic!("Expected Or condition"),
        }
    }

    #[test]
    fn test_range_condition() {
        let range = FilterCondition::range("age", Some(18.0), Some(65.0));
        match range {
            FilterCondition::Range { field, min, max } => {
                assert_eq!(field, "age");
                assert_eq!(min, Some(18.0));
                assert_eq!(max, Some(65.0));
            }
            _ => panic!("Expected Range condition"),
        }
    }

    #[test]
    fn test_date_range_condition() {
        let date_range =
            FilterCondition::date_range("date", Some("2023-01-01".to_string()), None);
        match date_range {
            FilterCondition::DateRange { field, min, max } => {
                assert_eq!(field, "date");
                assert_eq!(min, Some("2023-01-01".to_string()));
                assert_eq!(max, None);
            }
            _ => panic!("Expected DateRange condition"),
        }
    }

    #[test]
    fn test_filter_condition_serialization() {
        let condition = FilterCondition::equals("person", "John");
        let json = serde_json::to_string(&condition).expect("Serialization failed");
        assert!(json.contains("Equals"));
        assert!(json.contains("person"));
        assert!(json.contains("John"));
    }

    #[test]
    fn test_filter_condition_deserialization() {
        let json = r#"{"type":"Equals","data":{"field":"person","value":"John"}}"#;
        let condition: FilterCondition = serde_json::from_str(json).expect("Deserialization failed");
        match condition {
            FilterCondition::Equals { field, value } => {
                assert_eq!(field, "person");
                assert_eq!(value, "John");
            }
            _ => panic!("Expected Equals condition"),
        }
    }
}
