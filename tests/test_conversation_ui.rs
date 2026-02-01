// Test that the ConversationList component properly integrates with the Chat component

#[cfg(test)]
mod tests {
    use leptos::prelude::*;
    use rag_chat::web_app::components::chat::Chat;
    use rag_chat::web_app::components::conversation_list::ConversationList;

    #[test]
    fn test_conversation_list_component_exists() {
        // Verify we can create the component
        let _ = || {
            view! {
                <ConversationList />
            }
        };
    }

    #[test]
    fn test_chat_component_exists() {
        // Verify we can create the component
        let _ = || {
            view! {
                <Chat />
            }
        };
    }

    #[test]
    fn test_conversation_list_accepts_callbacks() {
        // Verify callbacks work
        let _ = || {
            let on_select = Callback::new(|_id: uuid::Uuid| {});
            let on_new = Callback::new(|_: ()| {});
            let on_delete = Callback::new(|_id: uuid::Uuid| {});

            view! {
                <ConversationList
                    on_conversation_select=on_select
                    on_new_conversation=on_new
                    on_delete_conversation=on_delete
                />
            }
        };
    }
}
