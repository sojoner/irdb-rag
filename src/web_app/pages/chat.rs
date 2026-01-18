use leptos::prelude::*;

use crate::web_app::components::chat::Chat;
use crate::web_app::components::navbar::NavBar;

#[component]
pub fn ChatPage() -> impl IntoView {
    view! {
        <div class="h-screen flex flex-col bg-gray-50">
            <NavBar />
            <div class="flex-1 container mx-auto px-4 py-6 overflow-hidden">
                <div class="h-full max-w-4xl mx-auto">
                    <div class="h-full bg-white rounded-lg shadow-lg">
                        <Chat />
                    </div>
                </div>
            </div>
        </div>
    }
}
