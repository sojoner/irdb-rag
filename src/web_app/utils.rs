use uuid::Uuid;

/// Format a document for inclusion in chat context
///
/// Format:
/// ---
/// Doc: <UUID>
/// >>>
/// <content>
/// <<<
///
pub fn format_document_for_chat(doc_id: Uuid, content: &str) -> String {
    format!(
        "---\nDoc: {}\n>>>\n{}\n<<<\n",
        doc_id,
        content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_document_for_chat_single() {
        let doc_id = Uuid::new_v4();
        let content = "This is a test document content.";

        let formatted = format_document_for_chat(doc_id, content);

        assert!(formatted.starts_with("---\nDoc: "));
        assert!(formatted.contains(&doc_id.to_string()));
        assert!(formatted.contains(">>>\nThis is a test document content.\n<<<"));
        assert!(formatted.ends_with("\n"));
    }

    #[test]
    fn test_format_document_for_chat_empty_content() {
        let doc_id = Uuid::new_v4();
        let content = "";

        let formatted = format_document_for_chat(doc_id, content);

        assert!(formatted.contains(">>>\n\n<<<"));
    }

    #[test]
    fn test_format_document_for_chat_multiline_content() {
        let doc_id = Uuid::new_v4();
        let content = "Line 1\nLine 2\nLine 3";

        let formatted = format_document_for_chat(doc_id, content);

        assert!(formatted.contains(">>>\nLine 1\nLine 2\nLine 3\n<<<"));
    }
}
