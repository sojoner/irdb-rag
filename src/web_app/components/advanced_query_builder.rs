use chrono::{Duration, Local};
use leptos::prelude::*;
use serde::{Deserialize, Serialize};

/// Represents a single filter in the advanced query builder
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryFilter {
    pub filter_type: FilterType,
    pub value: FilterValue,
}

/// The type of filter (determines which UI component is rendered)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilterType {
    DateRange,
    TextField(String),  // field name: title, content, summary, author
    ArrayField(String), // field name: keywords, locations, persons, organizations, products, concepts
}

/// The actual filter value (depends on filter type)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FilterValue {
    DateRange {
        from: Option<String>,
        to: Option<String>,
    },
    Text {
        field: String,
        value: String,
    },
    Array {
        field: String,
        values: Vec<String>,
    },
}

#[component]
pub fn AdvancedQueryBuilder(on_filter_change: Callback<Vec<QueryFilter>>) -> impl IntoView {
    let (filters, set_filters) = signal(Vec::<QueryFilter>::new());

    let add_date_filter = move |_| {
        let mut new_filters = filters.get();
        new_filters.push(QueryFilter {
            filter_type: FilterType::DateRange,
            value: FilterValue::DateRange {
                from: None,
                to: None,
            },
        });
        set_filters.set(new_filters.clone());
        on_filter_change.run(new_filters);
    };

    let add_text_filter = move |_| {
        let mut new_filters = filters.get();
        new_filters.push(QueryFilter {
            filter_type: FilterType::TextField("title".to_string()),
            value: FilterValue::Text {
                field: "title".to_string(),
                value: String::new(),
            },
        });
        set_filters.set(new_filters.clone());
        on_filter_change.run(new_filters);
    };

    let add_array_filter = move |_| {
        let mut new_filters = filters.get();
        new_filters.push(QueryFilter {
            filter_type: FilterType::ArrayField("keywords".to_string()),
            value: FilterValue::Array {
                field: "keywords".to_string(),
                values: Vec::new(),
            },
        });
        set_filters.set(new_filters.clone());
        on_filter_change.run(new_filters);
    };

    let remove_filter = move |idx: usize| {
        let mut new_filters = filters.get();
        if idx < new_filters.len() {
            new_filters.remove(idx);
            set_filters.set(new_filters.clone());
            on_filter_change.run(new_filters);
        }
    };

    view! {
        <div class="bg-white border border-gray-200 rounded-lg shadow-sm">
            <div class="px-4 py-3 border-b border-gray-200">
                <h3 class="text-sm font-semibold text-gray-900">Advanced Query Builder</h3>
            </div>

            <div class="px-4 py-4 space-y-4">
                {move || {
                    filters
                        .get()
                        .into_iter()
                        .enumerate()
                        .map(|(idx, filter)| -> AnyView {
                            match filter.filter_type {
                                FilterType::DateRange => {
                                    let date_filter_idx = idx;
                                    let date_range = if let FilterValue::DateRange { from, to } = &filter.value {
                                        (from.clone(), to.clone())
                                    } else {
                                        (None, None)
                                    };

                                    view! {
                                        <DateRangeFilter
                                            initial_from=date_range.0
                                            initial_to=date_range.1
                                            on_change=Callback::new(move |(from, to)| {
                                                let mut new_filters = filters.get();
                                                if date_filter_idx < new_filters.len() {
                                                    new_filters[date_filter_idx].value =
                                                        FilterValue::DateRange { from, to };
                                                    set_filters.set(new_filters.clone());
                                                    on_filter_change.run(new_filters);
                                                }
                                            })
                                            on_remove=Callback::new(move |_| remove_filter(date_filter_idx))
                                        />
                                    }.into_any()
                                }
                                FilterType::TextField(ref field) => {
                                    let text_filter_idx = idx;
                                    let text_value = if let FilterValue::Text { value, .. } = &filter.value {
                                        value.clone()
                                    } else {
                                        String::new()
                                    };

                                    view! {
                                        <TextFieldFilter
                                            initial_field=field.clone()
                                            initial_value=text_value
                                            on_change=Callback::new(move |(field, value)| {
                                                let mut new_filters = filters.get();
                                                if text_filter_idx < new_filters.len() {
                                                    new_filters[text_filter_idx].value =
                                                        FilterValue::Text { field, value };
                                                    set_filters.set(new_filters.clone());
                                                    on_filter_change.run(new_filters);
                                                }
                                            })
                                            on_remove=Callback::new(move |_| remove_filter(text_filter_idx))
                                        />
                                    }.into_any()
                                }
                                FilterType::ArrayField(ref field) => {
                                    let array_filter_idx = idx;
                                    let array_values = if let FilterValue::Array { values, .. } = &filter.value {
                                        values.clone()
                                    } else {
                                        Vec::new()
                                    };

                                    view! {
                                        <ArrayFieldFilter
                                            initial_field=field.clone()
                                            initial_values=array_values
                                            on_change=Callback::new(move |(field, values)| {
                                                let mut new_filters = filters.get();
                                                if array_filter_idx < new_filters.len() {
                                                    new_filters[array_filter_idx].value =
                                                        FilterValue::Array { field, values };
                                                    set_filters.set(new_filters.clone());
                                                    on_filter_change.run(new_filters);
                                                }
                                            })
                                            on_remove=Callback::new(move |_| remove_filter(array_filter_idx))
                                        />
                                    }.into_any()
                                }
                            }
                        })
                        .collect::<Vec<_>>()
                }}

                <div class="flex gap-2 pt-2">
                    <button on:click=add_date_filter
                        class="px-3 py-2 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded border border-blue-200">
                        "+ Date Range"
                    </button>
                    <button on:click=add_text_filter
                        class="px-3 py-2 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded border border-blue-200">
                        "+ Text Field"
                    </button>
                    <button on:click=add_array_filter
                        class="px-3 py-2 text-xs font-medium text-blue-600 bg-blue-50 hover:bg-blue-100 rounded border border-blue-200">
                        "+ Array Field"
                    </button>
                </div>
            </div>
        </div>
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum DatePreset {
    None,
    LastDay,
    LastWeek,
    LastMonth,
    LastYear,
}

#[component]
fn DateRangeFilter(
    initial_from: Option<String>,
    initial_to: Option<String>,
    on_change: Callback<(Option<String>, Option<String>)>,
    on_remove: Callback<()>,
) -> impl IntoView {
    let (date_preset, set_date_preset) = signal(DatePreset::None);
    let (from_date, set_from_date) = signal(initial_from.clone());
    let (to_date, set_to_date) = signal(initial_to.clone());

    let apply_preset = move |preset: DatePreset| {
        set_date_preset.set(preset);
        let (from, to) = calculate_date_range(preset);
        set_from_date.set(from.clone());
        set_to_date.set(to.clone());
        on_change.run((from, to));
    };

    let handle_from_change = move |ev| {
        let value = event_target_value(&ev);
        set_from_date.set(if value.is_empty() { None } else { Some(value) });
        on_change.run((from_date.get(), to_date.get()));
    };

    let handle_to_change = move |ev| {
        let value = event_target_value(&ev);
        set_to_date.set(if value.is_empty() { None } else { Some(value) });
        on_change.run((from_date.get(), to_date.get()));
    };

    view! {
        <div class="flex items-end gap-3 p-3 bg-gray-50 rounded border border-gray-200">
            <div class="flex-1">
                <label class="block text-xs font-medium text-gray-700 mb-1">
                    "Quick Select"
                </label>
                <div class="flex gap-1 flex-wrap">
                    <button on:click=move |_| apply_preset(DatePreset::LastDay)
                        class=format!(
                            "px-2 py-1 text-xs rounded {}",
                            if date_preset.get() == DatePreset::LastDay {
                                "bg-blue-500 text-white"
                            } else {
                                "bg-white border border-gray-300 text-gray-700 hover:bg-gray-100"
                            }
                        )>
                        "1 Day"
                    </button>
                    <button on:click=move |_| apply_preset(DatePreset::LastWeek)
                        class=format!(
                            "px-2 py-1 text-xs rounded {}",
                            if date_preset.get() == DatePreset::LastWeek {
                                "bg-blue-500 text-white"
                            } else {
                                "bg-white border border-gray-300 text-gray-700 hover:bg-gray-100"
                            }
                        )>
                        "Last Week"
                    </button>
                    <button on:click=move |_| apply_preset(DatePreset::LastMonth)
                        class=format!(
                            "px-2 py-1 text-xs rounded {}",
                            if date_preset.get() == DatePreset::LastMonth {
                                "bg-blue-500 text-white"
                            } else {
                                "bg-white border border-gray-300 text-gray-700 hover:bg-gray-100"
                            }
                        )>
                        "Last Month"
                    </button>
                    <button on:click=move |_| apply_preset(DatePreset::LastYear)
                        class=format!(
                            "px-2 py-1 text-xs rounded {}",
                            if date_preset.get() == DatePreset::LastYear {
                                "bg-blue-500 text-white"
                            } else {
                                "bg-white border border-gray-300 text-gray-700 hover:bg-gray-100"
                            }
                        )>
                        "Last Year"
                    </button>
                </div>
            </div>

            <div class="flex-1">
                <label class="block text-xs font-medium text-gray-700 mb-1">
                    "From (YYYY-MM-DD)"
                </label>
                <input
                    type="date"
                    value=move || from_date.get().unwrap_or_default()
                    on:change=handle_from_change
                    class="w-full px-2 py-1 text-xs border border-gray-300 rounded focus:ring-blue-500 focus:border-blue-500"
                />
            </div>

            <div class="flex-1">
                <label class="block text-xs font-medium text-gray-700 mb-1">
                    "To (YYYY-MM-DD)"
                </label>
                <input
                    type="date"
                    value=move || to_date.get().unwrap_or_default()
                    on:change=handle_to_change
                    class="w-full px-2 py-1 text-xs border border-gray-300 rounded focus:ring-blue-500 focus:border-blue-500"
                />
            </div>

            <button on:click=move |_| on_remove.run(())
                class="p-1 text-red-600 hover:bg-red-50 rounded">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
            </button>
        </div>
    }
}

fn calculate_date_range(preset: DatePreset) -> (Option<String>, Option<String>) {
    let now = Local::now();
    let to = now.format("%Y-%m-%d").to_string();

    let from = match preset {
        DatePreset::None => return (None, None),
        DatePreset::LastDay => now.checked_sub_signed(Duration::days(1)),
        DatePreset::LastWeek => now.checked_sub_signed(Duration::days(7)),
        DatePreset::LastMonth => now.checked_sub_signed(Duration::days(30)),
        DatePreset::LastYear => now.checked_sub_signed(Duration::days(365)),
    };

    (from.map(|d| d.format("%Y-%m-%d").to_string()), Some(to))
}

#[component]
fn TextFieldFilter(
    initial_field: String,
    initial_value: String,
    on_change: Callback<(String, String)>,
    on_remove: Callback<()>,
) -> impl IntoView {
    let (field, set_field) = signal(initial_field);
    let (value, set_value) = signal(initial_value);
    let (show_suggestions, set_show_suggestions) = signal(false);

    let text_fields = vec!["title", "content", "summary", "author"];
    let text_fields_for_render: Vec<String> = text_fields.iter().map(|s| s.to_string()).collect();

    let handle_field_change = move |ev| {
        let new_field = event_target_value(&ev);
        set_field.set(new_field.clone());
        on_change.run((new_field, value.get()));
    };

    let handle_value_change = move |ev| {
        let new_value = event_target_value(&ev);
        set_value.set(new_value.clone());
        set_show_suggestions.set(!new_value.is_empty());
        on_change.run((field.get(), new_value));
    };

    view! {
        <div class="flex items-end gap-3 p-3 bg-gray-50 rounded border border-gray-200">
            <div class="flex-1">
                <label class="block text-xs font-medium text-gray-700 mb-1">
                    "Field"
                </label>
                <select on:change=handle_field_change
                    class="w-full px-2 py-1 text-xs border border-gray-300 rounded focus:ring-blue-500 focus:border-blue-500">
                    {text_fields_for_render
                        .iter()
                        .map(|f| {
                            let f_clone = f.clone();
                            view! {
                                <option value=f.clone() selected=move || field.get() == f_clone>
                                    {f.clone()}
                                </option>
                            }
                        })
                        .collect::<Vec<_>>()}
                </select>
            </div>

            <div class="flex-1 relative">
                <label class="block text-xs font-medium text-gray-700 mb-1">
                    "Search"
                </label>
                <input
                    type="text"
                    value=move || value.get()
                    on:change=handle_value_change
                    on:focus=move |_| set_show_suggestions.set(!value.get().is_empty())
                    placeholder="Type to search..."
                    class="w-full px-2 py-1 text-xs border border-gray-300 rounded focus:ring-blue-500 focus:border-blue-500"
                />
                <Show when=move || show_suggestions.get()>
                    <div class="absolute top-full left-0 right-0 mt-1 bg-white border border-gray-300 rounded shadow-lg z-10">
                        <div class="p-2">
                            <div class="text-xs text-gray-500 px-2 py-1">
                                "Type-ahead suggestions (connected to document data)"
                            </div>
                        </div>
                    </div>
                </Show>
            </div>

            <button on:click=move |_| on_remove.run(())
                class="p-1 text-red-600 hover:bg-red-50 rounded">
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                </svg>
            </button>
        </div>
    }
}

#[component]
fn ArrayFieldFilter(
    initial_field: String,
    initial_values: Vec<String>,
    on_change: Callback<(String, Vec<String>)>,
    on_remove: Callback<()>,
) -> impl IntoView {
    let (field, set_field) = signal(initial_field);
    let (values, set_values) = signal(initial_values);
    let (search_term, set_search_term) = signal(String::new());
    let (show_dropdown, set_show_dropdown) = signal(false);

    let array_fields = vec![
        "keywords",
        "locations",
        "persons",
        "organizations",
        "products",
        "concepts",
    ];
    let array_fields_for_render: Vec<String> = array_fields.iter().map(|s| s.to_string()).collect();

    // Mock suggestions - in real app, fetch from server
    let get_suggestions = |field: &str, search: &str| {
        let mock_data = match field {
            "keywords" => vec![
                "Python",
                "Machine Learning",
                "Data Science",
                "AI",
                "Deep Learning",
            ],
            "locations" => vec!["New York", "San Francisco", "London", "Tokyo", "Berlin"],
            "persons" => vec!["John Doe", "Jane Smith", "Bob Johnson", "Alice Williams"],
            "organizations" => vec!["OpenAI", "Google", "Meta", "Microsoft", "Apple"],
            "products" => vec!["GPT-4", "BERT", "Claude", "Gemini", "Llama"],
            "concepts" => vec![
                "Classification",
                "Regression",
                "NLP",
                "Computer Vision",
                "Clustering",
            ],
            _ => vec![],
        };

        if search.is_empty() {
            mock_data
        } else {
            mock_data
                .into_iter()
                .filter(|s| s.to_lowercase().contains(&search.to_lowercase()))
                .collect()
        }
    };

    let handle_field_change = move |ev| {
        let new_field = event_target_value(&ev);
        set_field.set(new_field.clone());
        set_values.set(Vec::new());
        on_change.run((new_field, Vec::new()));
    };

    let handle_search_change = move |ev| {
        let search = event_target_value(&ev);
        set_search_term.set(search);
        set_show_dropdown.set(true);
    };

    let add_value = move |new_value: String| {
        let mut new_values = values.get();
        if !new_values.contains(&new_value) {
            new_values.push(new_value);
            set_values.set(new_values.clone());
            on_change.run((field.get(), new_values));
        }
        set_search_term.set(String::new());
        set_show_dropdown.set(false);
    };

    let remove_value = move |idx: usize| {
        let mut new_values = values.get();
        if idx < new_values.len() {
            new_values.remove(idx);
            set_values.set(new_values.clone());
            on_change.run((field.get(), new_values));
        }
    };

    let suggestions = move || {
        get_suggestions(&field.get(), &search_term.get())
            .into_iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };

    view! {
        <div class="flex flex-col gap-3 p-3 bg-gray-50 rounded border border-gray-200">
            <div class="flex items-end gap-3">
                <div class="flex-1">
                    <label class="block text-xs font-medium text-gray-700 mb-1">
                        "Field"
                    </label>
                    <select on:change=handle_field_change
                        class="w-full px-2 py-1 text-xs border border-gray-300 rounded focus:ring-blue-500 focus:border-blue-500">
                        {array_fields_for_render
                            .iter()
                            .map(|f| {
                                let f_clone = f.clone();
                                view! {
                                    <option value=f.clone() selected=move || field.get() == f_clone>
                                        {f.clone()}
                                    </option>
                                }
                            })
                            .collect::<Vec<_>>()}
                    </select>
                </div>

                <div class="flex-1 relative">
                    <label class="block text-xs font-medium text-gray-700 mb-1">
                        "Search & Select"
                    </label>
                    <input
                        type="text"
                        value=move || search_term.get()
                        on:change=handle_search_change
                        on:focus=move |_| set_show_dropdown.set(true)
                        placeholder="Search values..."
                        class="w-full px-2 py-1 text-xs border border-gray-300 rounded focus:ring-blue-500 focus:border-blue-500"
                    />

                    <Show when=move || show_dropdown.get() && !suggestions().is_empty()>
                        <div class="absolute top-full left-0 right-0 mt-1 bg-white border border-gray-300 rounded shadow-lg z-10 max-h-40 overflow-y-auto">
                            {suggestions()
                                .into_iter()
                                .map(|suggestion| {
                                    view! {
                                        <button
                                            type="button"
                                            on:click=move |_| add_value(suggestion.clone())
                                            class="w-full text-left px-3 py-2 text-xs hover:bg-blue-50 border-b border-gray-100 last:border-b-0">
                                            {suggestion.clone()}
                                        </button>
                                    }
                                })
                                .collect::<Vec<_>>()}
                        </div>
                    </Show>
                </div>

                <button on:click=move |_| on_remove.run(())
                    class="p-1 text-red-600 hover:bg-red-50 rounded">
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                    </svg>
                </button>
            </div>

            <Show when=move || !values.get().is_empty()>
                <div class="flex flex-wrap gap-2">
                    {move || {
                        values
                            .get()
                            .into_iter()
                            .enumerate()
                            .map(|(idx, val)| {
                                view! {
                                    <span class="inline-flex items-center gap-2 px-2 py-1 bg-blue-100 text-blue-800 rounded-full text-xs">
                                        {val}
                                        <button
                                            type="button"
                                            on:click=move |_| remove_value(idx)
                                            class="text-blue-600 hover:text-blue-800">
                                            <svg class="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
                                            </svg>
                                        </button>
                                    </span>
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </div>
            </Show>
        </div>
    }
}
