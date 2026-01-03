use leptos::*;
use leptos::prelude::*;
use uuid::Uuid;
use crate::types::Category;

#[component]
pub fn Filters(
    categories: Signal<Vec<Category>>,
    selected_category: Signal<Option<Uuid>>,
    set_selected_category: WriteSignal<Option<Uuid>>,
    #[prop(into)] on_change: Callback<()>,
) -> impl IntoView {
    let (show_filters, set_show_filters) = signal(true);

    view! {
        <div class="bg-white border-b border-gray-200">
            <button on:click=move |_| set_show_filters.update(|v: &mut bool| *v = !*v)
                    class="w-full px-4 py-2 flex justify-between items-center text-xs font-semibold text-gray-600 hover:bg-gray-50">
                <span class="flex items-center gap-2">
                    <svg class="h-3 w-3" fill="none" viewBox="0 0 24 24" stroke="currentColor">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 4a1 1 0 011-1h16a1 1 0 011 1v2.586a1 1 0 01-.293.707l-6.414 6.414a1 1 0 00-.293.707V17l-4 4v-6.586a1 1 0 00-.293-.707L3.293 7.293A1 1 0 013 6.586V4z" />
                    </svg>
                    "Filters"
                    <Show when=move || selected_category.get().is_some()>
                        <span class="ml-1 w-2 h-2 bg-blue-500 rounded-full"></span>
                    </Show>
                </span>
                <svg class="h-3 w-3 transform transition-transform" class:rotate-180=move || show_filters.get() fill="none" viewBox="0 0 24 24" stroke="currentColor">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
                </svg>
            </button>
            
            <Show when=move || show_filters.get()>
                <div class="px-4 pb-4 border-t border-gray-100 bg-gray-50/50">
                    <div class="grid grid-cols-2 gap-4 mt-3">
                        <div>
                            <label class="block text-xs font-medium text-gray-500 mb-1">"Category"</label>
                            <select on:change=move |ev| {
                                let val = event_target_value(&ev);
                                if val.is_empty() {
                                    set_selected_category.set(None);
                                } else if let Ok(id) = Uuid::parse_str(&val) {
                                    set_selected_category.set(Some(id));
                                }
                                on_change.run(());
                            } class="w-full text-xs border-gray-300 rounded focus:ring-blue-500 focus:border-blue-500">
                                <option value="">"All Categories"</option>
                                <For
                                    each=move || categories.get()
                                    key=|cat| cat.id
                                    children=move |cat| {
                                        view! {
                                            <option value=cat.id.to_string() selected=move || selected_category.get() == Some(cat.id)>
                                                {cat.name}
                                            </option>
                                        }
                                    }
                                />
                            </select>
                        </div>
                    </div>
                </div>
            </Show>
        </div>
    }
}
