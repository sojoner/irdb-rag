use leptos::*;
use leptos::prelude::*;
use uuid::Uuid;
use crate::domain::models::Category;
use crate::domain::dtos::AggregationStats;

#[server(GetAggregationStats, "/api")]
pub async fn get_aggregation_stats() -> Result<AggregationStats, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let stats = db::get_aggregation_stats(&state.pool).await
        .map_err(|e| ServerFnError::new(format!("Failed to get aggregation stats: {}", e)))?;

    Ok(stats)
}

#[server(GetCategories, "/api")]
pub async fn get_categories() -> Result<Vec<Category>, ServerFnError> {
    use crate::api::state::AppState;
    use crate::infra::db;

    let state = use_context::<AppState>()
        .ok_or_else(|| ServerFnError::new("AppState not found in context"))?;

    let categories = db::list_categories(&state.pool).await
        .map_err(|e| ServerFnError::new(format!("Failed to get categories: {}", e)))?;

    Ok(categories)
}

#[component]
pub fn FacetedFilters(
    categories: Signal<Vec<Category>>,
    selected_category: Signal<Option<Uuid>>,
    set_selected_category: WriteSignal<Option<Uuid>>,
    selected_keywords: Signal<Vec<String>>,
    set_selected_keywords: WriteSignal<Vec<String>>,
    selected_concepts: Signal<Vec<String>>,
    set_selected_concepts: WriteSignal<Vec<String>>,
    selected_locations: Signal<Vec<String>>,
    set_selected_locations: WriteSignal<Vec<String>>,
    selected_persons: Signal<Vec<String>>,
    set_selected_persons: WriteSignal<Vec<String>>,
    selected_organizations: Signal<Vec<String>>,
    set_selected_organizations: WriteSignal<Vec<String>>,
    selected_authors: Signal<Vec<String>>,
    set_selected_authors: WriteSignal<Vec<String>>,
    #[prop(into)] on_change: Callback<()>,
) -> impl IntoView {
    let (show_more_keywords, set_show_more_keywords) = signal(false);
    let (show_more_concepts, set_show_more_concepts) = signal(false);
    let (show_more_locations, set_show_more_locations) = signal(false);
    let (show_more_persons, set_show_more_persons) = signal(false);
    let (show_more_orgs, set_show_more_orgs) = signal(false);
    let (show_more_authors, set_show_more_authors) = signal(false);

    // Load aggregation stats using server function
    let stats_resource = Resource::new_blocking(|| (), |_| async { get_aggregation_stats().await });
    let stats = move || {
        match stats_resource.get() {
            Some(Ok(data)) => Some(data),
            _ => None,
        }
    };

    let toggle_keyword = move |keyword: String| {
        set_selected_keywords.update(|kw| {
            if kw.contains(&keyword) {
                kw.retain(|k| k != &keyword);
            } else {
                kw.push(keyword);
            }
        });
        on_change.run(());
    };

    let toggle_concept = move |concept: String| {
        set_selected_concepts.update(|c| {
            if c.contains(&concept) {
                c.retain(|x| x != &concept);
            } else {
                c.push(concept);
            }
        });
        on_change.run(());
    };

    let toggle_location = move |location: String| {
        set_selected_locations.update(|loc| {
            if loc.contains(&location) {
                loc.retain(|l| l != &location);
            } else {
                loc.push(location);
            }
        });
        on_change.run(());
    };

    let toggle_person = move |person: String| {
        set_selected_persons.update(|p| {
            if p.contains(&person) {
                p.retain(|x| x != &person);
            } else {
                p.push(person);
            }
        });
        on_change.run(());
    };

    let toggle_organization = move |org: String| {
        set_selected_organizations.update(|o| {
            if o.contains(&org) {
                o.retain(|x| x != &org);
            } else {
                o.push(org);
            }
        });
        on_change.run(());
    };

    let toggle_author = move |author: String| {
        set_selected_authors.update(|a| {
            if a.contains(&author) {
                a.retain(|x| x != &author);
            } else {
                a.push(author);
            }
        });
        on_change.run(());
    };

    view! {
        <Suspense fallback=|| view! { <div class="p-4 text-xs text-gray-500">"Loading filters..."</div> }>
            <div class="bg-white border-b border-gray-200">
                // Categories Section
                <div class="border-b border-gray-100">
                <button on:click=move |_| set_selected_category.set(None)
                        class="w-full px-4 py-2 flex items-center justify-between text-sm font-semibold text-gray-700 hover:bg-gray-50">
                    <span class="flex items-center gap-2">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 7h.01M7 3h5c.512 0 1.024.195 1.414.586l7 7a2 2 0 010 2.828l-7 7a2 2 0 01-2.828 0l-7-7A1.994 1.994 0 013 12V7a4 4 0 014-4z" />
                        </svg>
                        "Categories"
                    </span>
                    <Show when=move || selected_category.get().is_some()>
                        <span class="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded-full">1</span>
                    </Show>
                </button>

                <Show when=move || stats().is_some()>
                    <div class="px-4 py-3 space-y-1 bg-gray-50">
                        <For
                            each=move || stats().map(|s| s.categories).unwrap_or_default()
                            key=|(name, _)| name.clone()
                            children=move |(cat_name, count)| {
                                let cat_id = categories.get().iter()
                                    .find(|c| c.name == cat_name)
                                    .map(|c| c.id);

                                view! {
                                    <Show when=move || cat_id.is_some()>
                                        <label class="flex items-center gap-2 cursor-pointer hover:bg-white px-2 py-1.5 rounded text-sm">
                                            <input type="radio"
                                                   name="category"
                                                   prop:checked=move || selected_category.get() == cat_id
                                                   on:change=move |_| {
                                                       if let Some(id) = cat_id {
                                                           set_selected_category.set(Some(id));
                                                           on_change.run(());
                                                       }
                                                   }
                                                   class="rounded" />
                                            <span class="text-gray-700">{cat_name.clone()}</span>
                                            <span class="ml-auto text-xs text-gray-500">"(" {count} ")"</span>
                                        </label>
                                    </Show>
                                }
                            }
                        />
                    </div>
                </Show>
            </div>

            // Keywords Section
            <div class="border-b border-gray-100">
                <button class="w-full px-4 py-2 flex items-center justify-between text-sm font-semibold text-gray-700 hover:bg-gray-50">
                    <span class="flex items-center gap-2">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 21h10a2 2 0 002-2V9.414a1 1 0 00-.293-.707l-5.414-5.414A1 1 0 0012.586 3H7a2 2 0 00-2 2v14a2 2 0 002 2z" />
                        </svg>
                        "Keywords"
                    </span>
                    <Show when=move || !selected_keywords.get().is_empty()>
                        <span class="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded-full">
                            {move || selected_keywords.get().len()}
                        </span>
                    </Show>
                </button>

                <Show when=move || stats().is_some()>
                    <div class="px-4 py-3 space-y-1 bg-gray-50">
                        <For
                            each=move || {
                                let all = stats().map(|s| s.keywords).unwrap_or_default();
                                if show_more_keywords.get() {
                                    all
                                } else {
                                    all.into_iter().take(5).collect()
                                }
                            }
                            key=|(keyword, _)| keyword.clone()
                            children=move |(keyword, count)| {
                                let keyword_copy = keyword.clone();
                                let keyword_copy2 = keyword.clone();
                                view! {
                                    <label class="flex items-center gap-2 cursor-pointer hover:bg-white px-2 py-1.5 rounded text-sm">
                                        <input type="checkbox"
                                               prop:checked=move || selected_keywords.get().contains(&keyword_copy)
                                               on:change=move |_| toggle_keyword(keyword_copy2.clone())
                                               class="rounded" />
                                        <span class="text-gray-700 truncate">{keyword.clone()}</span>
                                        <span class="ml-auto text-xs text-gray-500 flex-shrink-0">"(" {count} ")"</span>
                                    </label>
                                }
                            }
                        />
                        <Show when=move || {
                            stats().map(|s| s.keywords.len() > 5).unwrap_or(false)
                        }>
                            <button on:click=move |_| set_show_more_keywords.update(|v| *v = !*v)
                                    class="text-xs text-blue-600 hover:text-blue-800 px-2 py-1 mt-1">
                                {move || if show_more_keywords.get() { "Show less" } else { "Show more" }}
                            </button>
                        </Show>
                    </div>
                </Show>
            </div>

            // Concepts Section
            <div class="border-b border-gray-100">
                <button class="w-full px-4 py-2 flex items-center justify-between text-sm font-semibold text-gray-700 hover:bg-gray-50">
                    <span class="flex items-center gap-2">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9.663 17h4.673M12 3v1m6.364 1.636l-.707.707M21 12h-1M4 12H3m3.343-5.657l-.707-.707m2.828 9.9a5 5 0 117.072 0l-.548.547A3.374 3.374 0 0014 18.469V19a2 2 0 11-4 0v-.531c0-.895-.356-1.754-.988-2.386l-.548-.547z" />
                        </svg>
                        "Concepts"
                    </span>
                    <Show when=move || !selected_concepts.get().is_empty()>
                        <span class="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded-full">
                            {move || selected_concepts.get().len()}
                        </span>
                    </Show>
                </button>

                <Show when=move || stats().is_some()>
                    <div class="px-4 py-3 space-y-1 bg-gray-50">
                        <For
                            each=move || {
                                let all = stats().map(|s| s.concepts).unwrap_or_default();
                                if show_more_concepts.get() {
                                    all
                                } else {
                                    all.into_iter().take(5).collect()
                                }
                            }
                            key=|(concept, _)| concept.clone()
                            children=move |(concept, count)| {
                                let concept_copy = concept.clone();
                                let concept_copy2 = concept.clone();
                                view! {
                                    <label class="flex items-center gap-2 cursor-pointer hover:bg-white px-2 py-1.5 rounded text-sm">
                                        <input type="checkbox"
                                               prop:checked=move || selected_concepts.get().contains(&concept_copy)
                                               on:change=move |_| toggle_concept(concept_copy2.clone())
                                               class="rounded" />
                                        <span class="text-gray-700 truncate">{concept.clone()}</span>
                                        <span class="ml-auto text-xs text-gray-500 flex-shrink-0">"(" {count} ")"</span>
                                    </label>
                                }
                            }
                        />
                        <Show when=move || {
                            stats().map(|s| s.concepts.len() > 5).unwrap_or(false)
                        }>
                            <button on:click=move |_| set_show_more_concepts.update(|v| *v = !*v)
                                    class="text-xs text-blue-600 hover:text-blue-800 px-2 py-1 mt-1">
                                {move || if show_more_concepts.get() { "Show less" } else { "Show more" }}
                            </button>
                        </Show>
                    </div>
                </Show>
            </div>

            // Locations Section
            <div class="border-b border-gray-100">
                <button class="w-full px-4 py-2 flex items-center justify-between text-sm font-semibold text-gray-700 hover:bg-gray-50">
                    <span class="flex items-center gap-2">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17.657 16.657L13.414 20.9a1.998 1.998 0 01-2.827 0l-4.244-4.243a8 8 0 1111.314 0z" />
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 11a3 3 0 11-6 0 3 3 0 016 0z" />
                        </svg>
                        "Locations"
                    </span>
                    <Show when=move || !selected_locations.get().is_empty()>
                        <span class="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded-full">
                            {move || selected_locations.get().len()}
                        </span>
                    </Show>
                </button>

                <Show when=move || stats().is_some()>
                    <div class="px-4 py-3 space-y-1 bg-gray-50">
                        <For
                            each=move || {
                                let all = stats().map(|s| s.locations).unwrap_or_default();
                                if show_more_locations.get() {
                                    all
                                } else {
                                    all.into_iter().take(5).collect()
                                }
                            }
                            key=|(location, _)| location.clone()
                            children=move |(location, count)| {
                                let location_copy = location.clone();
                                let location_copy2 = location.clone();
                                view! {
                                    <label class="flex items-center gap-2 cursor-pointer hover:bg-white px-2 py-1.5 rounded text-sm">
                                        <input type="checkbox"
                                               prop:checked=move || selected_locations.get().contains(&location_copy)
                                               on:change=move |_| toggle_location(location_copy2.clone())
                                               class="rounded" />
                                        <span class="text-gray-700 truncate">{location.clone()}</span>
                                        <span class="ml-auto text-xs text-gray-500 flex-shrink-0">"(" {count} ")"</span>
                                    </label>
                                }
                            }
                        />
                        <Show when=move || {
                            stats().map(|s| s.locations.len() > 5).unwrap_or(false)
                        }>
                            <button on:click=move |_| set_show_more_locations.update(|v| *v = !*v)
                                    class="text-xs text-blue-600 hover:text-blue-800 px-2 py-1 mt-1">
                                {move || if show_more_locations.get() { "Show less" } else { "Show more" }}
                            </button>
                        </Show>
                    </div>
                </Show>
            </div>

            // Persons Section
            <div class="border-b border-gray-100">
                <button class="w-full px-4 py-2 flex items-center justify-between text-sm font-semibold text-gray-700 hover:bg-gray-50">
                    <span class="flex items-center gap-2">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4.354a4 4 0 110 8.308 4 4 0 010-8.308M3 20.25V24h18v-3.75a6 6 0 00-6-6H9a6 6 0 00-6 6z" />
                        </svg>
                        "People"
                    </span>
                    <Show when=move || !selected_persons.get().is_empty()>
                        <span class="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded-full">
                            {move || selected_persons.get().len()}
                        </span>
                    </Show>
                </button>

                <Show when=move || stats().is_some()>
                    <div class="px-4 py-3 space-y-1 bg-gray-50">
                        <For
                            each=move || {
                                let all = stats().map(|s| s.persons).unwrap_or_default();
                                if show_more_persons.get() {
                                    all
                                } else {
                                    all.into_iter().take(5).collect()
                                }
                            }
                            key=|(person, _)| person.clone()
                            children=move |(person, count)| {
                                let person_copy = person.clone();
                                let person_copy2 = person.clone();
                                view! {
                                    <label class="flex items-center gap-2 cursor-pointer hover:bg-white px-2 py-1.5 rounded text-sm">
                                        <input type="checkbox"
                                               prop:checked=move || selected_persons.get().contains(&person_copy)
                                               on:change=move |_| toggle_person(person_copy2.clone())
                                               class="rounded" />
                                        <span class="text-gray-700 truncate">{person.clone()}</span>
                                        <span class="ml-auto text-xs text-gray-500 flex-shrink-0">"(" {count} ")"</span>
                                    </label>
                                }
                            }
                        />
                        <Show when=move || {
                            stats().map(|s| s.persons.len() > 5).unwrap_or(false)
                        }>
                            <button on:click=move |_| set_show_more_persons.update(|v| *v = !*v)
                                    class="text-xs text-blue-600 hover:text-blue-800 px-2 py-1 mt-1">
                                {move || if show_more_persons.get() { "Show less" } else { "Show more" }}
                            </button>
                        </Show>
                    </div>
                </Show>
            </div>

            // Organizations Section
            <div class="border-b border-gray-100">
                <button class="w-full px-4 py-2 flex items-center justify-between text-sm font-semibold text-gray-700 hover:bg-gray-50">
                    <span class="flex items-center gap-2">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 21V5a2 2 0 00-2-2H7a2 2 0 00-2 2v16m14 0h2m-2 0h-5m-9 0H3m2 0h5M9 7h1m-1 4h1m4-4h1m-1 4h1m-5 10v-5a1 1 0 011-1h2a1 1 0 011 1v5m-4 0h4" />
                        </svg>
                        "Organizations"
                    </span>
                    <Show when=move || !selected_organizations.get().is_empty()>
                        <span class="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded-full">
                            {move || selected_organizations.get().len()}
                        </span>
                    </Show>
                </button>

                <Show when=move || stats().is_some()>
                    <div class="px-4 py-3 space-y-1 bg-gray-50">
                        <For
                            each=move || {
                                let all = stats().map(|s| s.organizations).unwrap_or_default();
                                if show_more_orgs.get() {
                                    all
                                } else {
                                    all.into_iter().take(5).collect()
                                }
                            }
                            key=|(org, _)| org.clone()
                            children=move |(org, count)| {
                                let org_copy = org.clone();
                                let org_copy2 = org.clone();
                                view! {
                                    <label class="flex items-center gap-2 cursor-pointer hover:bg-white px-2 py-1.5 rounded text-sm">
                                        <input type="checkbox"
                                               prop:checked=move || selected_organizations.get().contains(&org_copy)
                                               on:change=move |_| toggle_organization(org_copy2.clone())
                                               class="rounded" />
                                        <span class="text-gray-700 truncate">{org.clone()}</span>
                                        <span class="ml-auto text-xs text-gray-500 flex-shrink-0">"(" {count} ")"</span>
                                    </label>
                                }
                            }
                        />
                        <Show when=move || {
                            stats().map(|s| s.organizations.len() > 5).unwrap_or(false)
                        }>
                            <button on:click=move |_| set_show_more_orgs.update(|v| *v = !*v)
                                    class="text-xs text-blue-600 hover:text-blue-800 px-2 py-1 mt-1">
                                {move || if show_more_orgs.get() { "Show less" } else { "Show more" }}
                            </button>
                        </Show>
                    </div>
                </Show>
            </div>

            // Authors Section
            <div class="border-b border-gray-100">
                <button class="w-full px-4 py-2 flex items-center justify-between text-sm font-semibold text-gray-700 hover:bg-gray-50">
                    <span class="flex items-center gap-2">
                        <svg class="h-4 w-4" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15.232 5.232l3.536 3.536m-2.036-5.036a2.5 2.5 0 113.536 3.536L6.5 21.036H3v-3.572L16.732 3.732z" />
                        </svg>
                        "Authors"
                    </span>
                    <Show when=move || !selected_authors.get().is_empty()>
                        <span class="text-xs bg-blue-100 text-blue-700 px-2 py-1 rounded-full">
                            {move || selected_authors.get().len()}
                        </span>
                    </Show>
                </button>

                <Show when=move || stats().is_some()>
                    <div class="px-4 py-3 space-y-1 bg-gray-50">
                        <For
                            each=move || {
                                let all = stats().map(|s| s.authors).unwrap_or_default();
                                if show_more_authors.get() {
                                    all
                                } else {
                                    all.into_iter().take(5).collect()
                                }
                            }
                            key=|(author, _)| author.clone()
                            children=move |(author, count)| {
                                let author_copy = author.clone();
                                let author_copy2 = author.clone();
                                view! {
                                    <label class="flex items-center gap-2 cursor-pointer hover:bg-white px-2 py-1.5 rounded text-sm">
                                        <input type="checkbox"
                                               prop:checked=move || selected_authors.get().contains(&author_copy)
                                               on:change=move |_| toggle_author(author_copy2.clone())
                                               class="rounded" />
                                        <span class="text-gray-700 truncate">{author.clone()}</span>
                                        <span class="ml-auto text-xs text-gray-500 flex-shrink-0">"(" {count} ")"</span>
                                    </label>
                                }
                            }
                        />
                        <Show when=move || {
                            stats().map(|s| s.authors.len() > 5).unwrap_or(false)
                        }>
                            <button on:click=move |_| set_show_more_authors.update(|v| *v = !*v)
                                    class="text-xs text-blue-600 hover:text-blue-800 px-2 py-1 mt-1">
                                {move || if show_more_authors.get() { "Show less" } else { "Show more" }}
                            </button>
                        </Show>
                    </div>
                </Show>
            </div>
        </div>
        </Suspense>
    }
}
