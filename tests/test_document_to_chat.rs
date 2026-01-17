use uuid::Uuid;

/// Test document formatting for chat context
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
fn test_format_document_for_chat_multiple() {
    let doc_id_1 = Uuid::new_v4();
    let doc_id_2 = Uuid::new_v4();
    let content_1 = "First document.";
    let content_2 = "Second document.";

    let formatted_1 = format_document_for_chat(doc_id_1, content_1);
    let formatted_2 = format_document_for_chat(doc_id_2, content_2);

    let combined = format!("{}{}", formatted_1, formatted_2);

    assert!(combined.contains("Doc: "));
    assert!(combined.contains(&doc_id_1.to_string()));
    assert!(combined.contains(&doc_id_2.to_string()));
    assert!(combined.contains("First document"));
    assert!(combined.contains("Second document"));
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

/// Helper function to format a document for chat
fn format_document_for_chat(doc_id: Uuid, content: &str) -> String {
    format!(
        "---\nDoc: {}\n>>>\n{}\n<<<\n",
        doc_id,
        content
    )
}
