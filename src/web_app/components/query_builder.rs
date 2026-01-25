use crate::domain::models::{FieldMetadata, FieldType, FieldValueAutocomplete};
use crate::domain::query_builder_types::FilterCondition;
use crate::domain::dtos::FieldValueRequest;
use leptos::prelude::*;
use uuid::Uuid;
use tokio::time::sleep;
use std::time::Duration;

// ============================================
// Server Functions
// ============================================

#[server(GetMetadataFields, "/api")]
pub async fn get_metadata_fields() -> Result<Vec<FieldMetadata>, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::metadata as metadata_service;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let fields: Vec<FieldMetadata> = metadata_service::get_metadata_fields(&state.pool)
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to get metadata fields: {}", e)))?;

    Ok(fields)
}

#[server(GetFieldValueAutocomplete, "/api")]
pub async fn get_field_value_autocomplete(
    request: FieldValueRequest,
) -> Result<FieldValueAutocomplete, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::metadata as metadata_service;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let autocomplete: FieldValueAutocomplete = metadata_service::get_field_value_autocomplete(&state.pool, request)
        .await
        .map_err(|e| ServerFnError::new(format!("Failed to get autocomplete values: {}", e)))?;

    Ok(autocomplete)
}

// ============================================
// Type Definitions
// ============================================

#[derive(Debug, Clone, Copy)]
enum FilterOperator {
    Equals,
    Contains,
    Range,
    DateRange,
}

impl FilterOperator {
    fn label(&self) -> &'static str {
        match self {
            FilterOperator::Equals => "Equals",
            FilterOperator::Contains => "Contains",
            FilterOperator::Range => "Range",
            FilterOperator::DateRange => "Date Range",
        }
    }

    fn all() -> Vec<Self> {
        vec![
            FilterOperator::Equals,
            FilterOperator::Contains,
            FilterOperator::Range,
            FilterOperator::DateRange,
        ]
    }
}

// ============================================
// Main QueryBuilder Component
// ============================================

#[component]
pub fn QueryBuilder(
    #[prop(into)] on_filter_change: Callback<Option<FilterCondition>>,
) -> impl IntoView {
    // State for the query builder
    let (filters, set_filters) = signal(Vec::<FilterSpec>::new());
    let (logical_operator, set_logical_operator) = signal("AND");

    // Load metadata fields
    let fields_resource =
        Resource::new_blocking(|| (), |_| async { get_metadata_fields().await });
    let available_fields = Signal::derive(move || {
        fields_resource
            .get()
            .and_then(|res: Result<_, _>| res.ok())
            .unwrap_or_default()
    });

    // Effect to notify parent of filter changes
    Effect::new(move |_| {
        let filter_specs = filters.get();
        let operator = logical_operator.get();

        let condition = build_filter_condition(&filter_specs, operator);
        on_filter_change.run(condition);
    });

    let add_filter = move |_| {
        set_filters.update(|f| {
            f.push(FilterSpec {
                id: Uuid::new_v4(),
                field: String::new(),
                operator: FilterOperator::Equals,
                value: String::new(),
                value_min: None,
                value_max: None,
                date_min: None,
                date_max: None,
            });
        });
    };

    let remove_filter = move |index: usize| {
        set_filters.update(|f| {
            if index < f.len() {
                f.remove(index);
            }
        });
    };

    let update_filter = move |index: usize, spec: FilterSpec| {
        set_filters.update(|f| {
            if index < f.len() {
                f[index] = spec;
            }
        });
    };

    view! {
        <div class="bg-white border border-gray-200 rounded-lg p-4 space-y-3">
            // Header
            <div class="flex items-center justify-between">
                <h3 class="text-sm font-semibold text-gray-900">"Advanced Query Builder"</h3>
                <button
                    on:click=add_filter
                    class="text-xs bg-blue-600 text-white px-3 py-1.5 rounded hover:bg-blue-700 transition-colors"
                    title="Add filter condition"
                >
                    <svg class="h-3 w-3 inline mr-1" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                    </svg>
                    "Add Filter"
                </button>
            </div>

            // Logical operator selection (only show if we have multiple filters)
            <Show when=move || { filters.get().len() > 1 }>
                <div class="flex items-center gap-2 pb-2 border-b border-gray-100">
                    <span class="text-xs text-gray-600">"Combine filters with:"</span>
                    <div class="flex gap-2">
                        <label class="flex items-center gap-1 cursor-pointer text-sm">
                            <input
                                type="radio"
                                name="logical-op"
                                value="AND"
                                prop:checked=move || logical_operator.get() == "AND"
                                on:change=move |_| set_logical_operator.set("AND")
                                class="rounded"
                            />
                            "AND"
                        </label>
                        <label class="flex items-center gap-1 cursor-pointer text-sm">
                            <input
                                type="radio"
                                name="logical-op"
                                value="OR"
                                prop:checked=move || logical_operator.get() == "OR"
                                on:change=move |_| set_logical_operator.set("OR")
                                class="rounded"
                            />
                            "OR"
                        </label>
                    </div>
                </div>
            </Show>

            // Filter rows
            <div class="space-y-2">
                <Suspense fallback=|| view! { <div class="text-xs text-gray-500">"Loading fields..."</div> }>
                    <For
                        each=move || filters.get().into_iter().enumerate()
                        key=|(_, spec)| spec.id
                        children=move |(idx, spec): (usize, FilterSpec)| {
                            let on_update = move |updated: FilterSpec| {
                                update_filter(idx, updated);
                            };
                            let on_remove = move |_| {
                                remove_filter(idx);
                            };

                            view! {
                                <FilterRow
                                    spec=spec
                                    available_fields=available_fields
                                    on_update=Callback::new(on_update)
                                    on_remove=Callback::new(on_remove)
                                />
                            }
                        }
                    />
                </Suspense>
            </div>

            // Empty state
            <Show when=move || filters.get().is_empty()>
                <div class="text-center py-4">
                    <p class="text-sm text-gray-500">"No filters yet. Add one to get started."</p>
                </div>
            </Show>
        </div>
    }
}

// ============================================
// FilterRow Sub-Component
// ============================================

#[derive(Debug, Clone)]
struct FilterSpec {
    id: Uuid,
    field: String,
    operator: FilterOperator,
    value: String,
    value_min: Option<String>,
    value_max: Option<String>,
    date_min: Option<String>,
    date_max: Option<String>,
}

#[component]
fn FilterRow(
    spec: FilterSpec,
    available_fields: Signal<Vec<FieldMetadata>>,
    #[prop(into)] on_update: Callback<FilterSpec>,
    #[prop(into)] on_remove: Callback<()>,
) -> impl IntoView {
    let (field, set_field) = signal(spec.field.clone());
    let (operator, set_operator) = signal(spec.operator);
    let (value, set_value) = signal(spec.value.clone());
    let (value_min, set_value_min) = signal(spec.value_min.clone());
    let (value_max, set_value_max) = signal(spec.value_max.clone());
    let (date_min, set_date_min) = signal(spec.date_min.clone());
    let (date_max, set_date_max) = signal(spec.date_max.clone());

    // State for autocomplete suggestions
    let (suggestions, set_suggestions) = signal(Vec::<(String, i64)>::new());
    let (show_suggestions, set_show_suggestions) = signal(false);

    // Load autocomplete suggestions when field and value change
    let autocomplete_resource = Resource::new(
        move || (field.get(), value.get()),
        move |(f, v)| async move {
            if f.is_empty() || v.is_empty() {
                Ok::<Vec<(String, i64)>, String>(Vec::new())
            } else {
                match get_field_value_autocomplete(FieldValueRequest {
                    field: f,
                    query: v,
                    limit: 10,
                })
                .await
                {
                    Ok(ac) => Ok(ac.values),
                    Err(_) => Ok(Vec::new()),
                }
            }
        },
    );

    Effect::new(move |_| {
        if let Some(Ok(values)) = autocomplete_resource.get() {
            set_suggestions.set(values);
            set_show_suggestions.set(true);
        }
    });

    let notify_update = move |_| {
        let updated = FilterSpec {
            id: spec.id,
            field: field.get(),
            operator: operator.get(),
            value: value.get(),
            value_min: value_min.get(),
            value_max: value_max.get(),
            date_min: date_min.get(),
            date_max: date_max.get(),
        };
        on_update.run(updated);
    };

    let get_field_type = move || {
        available_fields
            .get()
            .iter()
            .find(|f| f.name == field.get())
            .map(|f| f.field_type.clone())
    };

    // Determine which operators are valid for the selected field
    let valid_operators = move || {
        match get_field_type() {
            Some(FieldType::Text) => vec![FilterOperator::Equals, FilterOperator::Contains],
            Some(FieldType::Number { .. }) => vec![FilterOperator::Equals, FilterOperator::Range],
            Some(FieldType::Date { .. }) => vec![FilterOperator::DateRange],
            None => FilterOperator::all(),
        }
    };

    view! {
        <div class="flex items-start gap-2 p-3 bg-gray-50 rounded border border-gray-200">
            <div class="flex-1 space-y-2">
                // Field selector
                <div class="flex items-center gap-2">
                    <label class="text-xs font-medium text-gray-700 w-16">"Field:"</label>
                    <select
                        class="flex-1 text-sm border border-gray-300 rounded px-2 py-1.5 focus:outline-none focus:ring-1 focus:ring-blue-500"
                        prop:value=move || field.get()
                        on:change=move |ev| {
                            let new_field = event_target_value(&ev);
                            set_field.set(new_field);
                            // Reset operator when field changes
                            set_operator.set(FilterOperator::Equals);
                            notify_update(());
                        }
                    >
                        <option value="">"-- Select a field --"</option>
                        <For
                            each=move || available_fields.get()
                            key=|f| f.name.clone()
                            children=move |field_meta: FieldMetadata| {
                                let type_label = match field_meta.field_type {
                                    FieldType::Text => "text",
                                    FieldType::Number { .. } => "number",
                                    FieldType::Date { .. } => "date",
                                };
                                view! {
                                    <option value=field_meta.name.clone()>
                                        {field_meta.name.clone()}
                                        " ("
                                        {type_label}
                                        ")"
                                    </option>
                                }
                            }
                        />
                    </select>
                </div>

                // Operator selector
                <Show when=move || !field.get().is_empty()>
                    <div class="flex items-center gap-2">
                        <label class="text-xs font-medium text-gray-700 w-16">"Operator:"</label>
                        <select
                            class="flex-1 text-sm border border-gray-300 rounded px-2 py-1.5 focus:outline-none focus:ring-1 focus:ring-blue-500"
                            prop:value=move || format!("{:?}", operator.get())
                            on:change=move |ev| {
                                let value_str = event_target_value(&ev);
                                let new_op = match value_str.as_str() {
                                    "Equals" => FilterOperator::Equals,
                                    "Contains" => FilterOperator::Contains,
                                    "Range" => FilterOperator::Range,
                                    "DateRange" => FilterOperator::DateRange,
                                    _ => FilterOperator::Equals,
                                };
                                set_operator.set(new_op);
                                notify_update(());
                            }
                        >
                            <For
                                each=valid_operators
                                key=|op| format!("{:?}", op)
                                children=move |op: FilterOperator| {
                                    view! {
                                        <option value=format!("{:?}", op)>{op.label()}</option>
                                    }
                                }
                            />
                        </select>
                    </div>
                </Show>

                // Value inputs based on operator
                <Show when=move || !field.get().is_empty()>
                    <div class="flex items-center gap-2">
                        <label class="text-xs font-medium text-gray-700 w-16">"Value:"</label>
                        <div class="flex-1">
                            <div class="space-y-2">
                                <Show when=move || matches!(operator.get(), FilterOperator::Equals | FilterOperator::Contains)>
                                    <div class="relative">
                                        <input
                                            type="text"
                                            class="w-full text-sm border border-gray-300 rounded px-2 py-1.5 focus:outline-none focus:ring-1 focus:ring-blue-500"
                                            placeholder="Enter value..."
                                            prop:value=move || value.get()
                                            on:input=move |ev| {
                                                set_value.set(event_target_value(&ev));
                                                notify_update(());
                                            }
                                            on:focus=move |_| set_show_suggestions.set(true)
                                            on:blur=move |_| {
                                                leptos::task::spawn_local(async move {
                                                    // Delay to allow click on suggestion
                                                    sleep(Duration::from_millis(100)).await;
                                                    set_show_suggestions.set(false);
                                                });
                                            }
                                        />
                                        // Autocomplete suggestions
                                        <Show when=move || show_suggestions.get() && !suggestions.get().is_empty()>
                                            <div class="absolute top-full left-0 right-0 mt-1 bg-white border border-gray-300 rounded shadow-lg z-10 max-h-48 overflow-y-auto">
                                                <For
                                                    each=move || suggestions.get()
                                                    key=|(s, _)| s.clone()
                                                    children=move |(suggestion, count): (String, i64)| {
                                                        let sugg_copy = suggestion.clone();
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class="w-full text-left px-3 py-1.5 text-sm text-gray-700 hover:bg-blue-50 border-b border-gray-100 last:border-b-0 flex items-center justify-between"
                                                                on:click=move |_| {
                                                                    set_value.set(sugg_copy.clone());
                                                                    set_show_suggestions.set(false);
                                                                    notify_update(());
                                                                }
                                                            >
                                                                <span>{suggestion}</span>
                                                                <span class="text-xs text-gray-400 ml-2">"(" {count} ")"</span>
                                                            </button>
                                                        }
                                                    }
                                                />
                                            </div>
                                        </Show>
                                    </div>
                                </Show>

                                <Show when=move || matches!(operator.get(), FilterOperator::Range)>
                                    <div class="flex items-center gap-2">
                                        <input
                                            type="number"
                                            class="flex-1 text-sm border border-gray-300 rounded px-2 py-1.5 focus:outline-none focus:ring-1 focus:ring-blue-500"
                                            placeholder="Min"
                                            prop:value=move || value_min.get().unwrap_or_default()
                                            on:input=move |ev| {
                                                let val = event_target_value(&ev);
                                                set_value_min.set(if val.is_empty() { None } else { Some(val) });
                                                notify_update(());
                                            }
                                        />
                                        <span class="text-xs text-gray-500">"to"</span>
                                        <input
                                            type="number"
                                            class="flex-1 text-sm border border-gray-300 rounded px-2 py-1.5 focus:outline-none focus:ring-1 focus:ring-blue-500"
                                            placeholder="Max"
                                            prop:value=move || value_max.get().unwrap_or_default()
                                            on:input=move |ev| {
                                                let val = event_target_value(&ev);
                                                set_value_max.set(if val.is_empty() { None } else { Some(val) });
                                                notify_update(());
                                            }
                                        />
                                    </div>
                                </Show>

                                <Show when=move || matches!(operator.get(), FilterOperator::DateRange)>
                                    <div class="flex items-center gap-2">
                                        <input
                                            type="date"
                                            class="flex-1 text-sm border border-gray-300 rounded px-2 py-1.5 focus:outline-none focus:ring-1 focus:ring-blue-500"
                                            placeholder="From"
                                            prop:value=move || date_min.get().unwrap_or_default()
                                            on:input=move |ev| {
                                                let val = event_target_value(&ev);
                                                set_date_min.set(if val.is_empty() { None } else { Some(val) });
                                                notify_update(());
                                            }
                                        />
                                        <span class="text-xs text-gray-500">"to"</span>
                                        <input
                                            type="date"
                                            class="flex-1 text-sm border border-gray-300 rounded px-2 py-1.5 focus:outline-none focus:ring-1 focus:ring-blue-500"
                                            placeholder="To"
                                            prop:value=move || date_max.get().unwrap_or_default()
                                            on:input=move |ev| {
                                                let val = event_target_value(&ev);
                                                set_date_max.set(if val.is_empty() { None } else { Some(val) });
                                                notify_update(());
                                            }
                                        />
                                    </div>
                                </Show>
                            </div>
                        </div>
                    </div>
                </Show>
            </div>

            // Remove button
            <button
                on:click=move |_| on_remove.run(())
                class="text-xs text-red-600 hover:text-red-700 p-1 hover:bg-red-50 rounded transition-colors flex-shrink-0 mt-1"
                title="Remove this filter"
            >
                <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
            </button>
        </div>
    }
}

// ============================================
// Helper Functions
// ============================================

fn build_filter_condition(specs: &[FilterSpec], operator: &str) -> Option<FilterCondition> {
    let conditions: Vec<FilterCondition> = specs
        .iter()
        .filter_map(|spec| {
            if spec.field.is_empty() {
                return None;
            }

            match spec.operator {
                FilterOperator::Equals => Some(FilterCondition::Equals {
                    field: spec.field.clone(),
                    value: spec.value.clone(),
                }),
                FilterOperator::Contains => Some(FilterCondition::Contains {
                    field: spec.field.clone(),
                    value: spec.value.clone(),
                }),
                FilterOperator::Range => Some(FilterCondition::Range {
                    field: spec.field.clone(),
                    min: spec.value_min.as_ref().and_then(|v| v.parse().ok()),
                    max: spec.value_max.as_ref().and_then(|v| v.parse().ok()),
                }),
                FilterOperator::DateRange => Some(FilterCondition::DateRange {
                    field: spec.field.clone(),
                    min: spec.date_min.clone(),
                    max: spec.date_max.clone(),
                }),
            }
        })
        .collect();

    match conditions.len() {
        0 => None,
        1 => Some(conditions.into_iter().next().unwrap()),
        _ => {
            if operator == "OR" {
                Some(FilterCondition::Or(conditions))
            } else {
                Some(FilterCondition::And(conditions))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_filter_condition_empty() {
        let specs = vec![];
        let result = build_filter_condition(&specs, "AND");
        assert_eq!(result, None);
    }

    #[test]
    fn test_build_filter_condition_single_equals() {
        let specs = vec![FilterSpec {
            id: Uuid::new_v4(),
            field: "title".to_string(),
            operator: FilterOperator::Equals,
            value: "test".to_string(),
            value_min: None,
            value_max: None,
            date_min: None,
            date_max: None,
        }];

        let result = build_filter_condition(&specs, "AND");
        assert!(matches!(result, Some(FilterCondition::Equals { .. })));
    }

    #[test]
    fn test_build_filter_condition_and() {
        let specs = vec![
            FilterSpec {
                id: Uuid::new_v4(),
                field: "title".to_string(),
                operator: FilterOperator::Equals,
                value: "test".to_string(),
                value_min: None,
                value_max: None,
                date_min: None,
                date_max: None,
            },
            FilterSpec {
                id: Uuid::new_v4(),
                field: "author".to_string(),
                operator: FilterOperator::Equals,
                value: "john".to_string(),
                value_min: None,
                value_max: None,
                date_min: None,
                date_max: None,
            },
        ];

        let result = build_filter_condition(&specs, "AND");
        assert!(matches!(result, Some(FilterCondition::And(_))));
    }

    #[test]
    fn test_build_filter_condition_or() {
        let specs = vec![
            FilterSpec {
                id: Uuid::new_v4(),
                field: "title".to_string(),
                operator: FilterOperator::Equals,
                value: "test".to_string(),
                value_min: None,
                value_max: None,
                date_min: None,
                date_max: None,
            },
            FilterSpec {
                id: Uuid::new_v4(),
                field: "author".to_string(),
                operator: FilterOperator::Equals,
                value: "john".to_string(),
                value_min: None,
                value_max: None,
                date_min: None,
                date_max: None,
            },
        ];

        let result = build_filter_condition(&specs, "OR");
        assert!(matches!(result, Some(FilterCondition::Or(_))));
    }
}
