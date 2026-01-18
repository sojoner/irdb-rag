use leptos::prelude::*;
use leptos_router::components::A;

#[component]
pub fn NavBar() -> impl IntoView {
    view! {
        <div class="bg-white border-b border-gray-200 px-6 py-3">
            <div class="flex gap-4">
                <A href="/" attr:class="px-3 py-2 text-sm font-medium text-gray-700 hover:text-blue-600 hover:bg-blue-50 rounded-md">
                    "Search"
                </A>
                <A href="/chat" attr:class="px-3 py-2 text-sm font-medium text-gray-700 hover:text-blue-600 hover:bg-blue-50 rounded-md">
                    "Chat"
                </A>
                <A href="/import" attr:class="px-3 py-2 text-sm font-medium text-gray-700 hover:text-blue-600 hover:bg-blue-50 rounded-md">
                    "Import"
                </A>
            </div>
        </div>
    }
}
