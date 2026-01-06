//! Pure utility functions for services layer
//!
//! Contains pure, side-effect-free functions for text processing, validation, and transformation.
//! These functions are testable without requiring external dependencies.

use std::path::Path;

// ============================================
// Text Processing
// ============================================

/// Chunk text into smaller pieces using text-splitter
/// Pure function - no side effects
pub fn chunk_text(text: &str, target_tokens: usize) -> Vec<String> {
    use text_splitter::{ChunkConfig, TextSplitter};

    let splitter = TextSplitter::new(ChunkConfig::new(target_tokens).with_trim(true));

    splitter.chunks(text)
        .map(|s: &str| s.to_string())
        .collect()
}

/// Check if a file extension is indexable
/// Pure function - no side effects
pub fn is_indexable_file(extension: &str) -> bool {
    let lower = extension.to_lowercase();

    // Skip hidden/special files
    if lower.is_empty() || matches!(lower.as_str(), "ds_store" | "gitignore" | ".gitignore") {
        return false;
    }

    // Indexable formats
    matches!(
        lower.as_str(),
        "pdf" | "docx" | "doc" | "txt" | "md" | "html" | "htm" | "rtf" | "xlsx" | "csv"
    )
}

/// Get file extension from path
/// Pure function - no side effects
pub fn get_file_extension(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase()
}

/// Format file size for display
/// Pure function - no side effects
pub fn format_file_size(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
    let mut size = bytes as f64;
    let mut unit_idx = 0;

    while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
        size /= 1024.0;
        unit_idx += 1;
    }

    format!("{:.2} {}", size, UNITS[unit_idx])
}

// ============================================
// Validation
// ============================================

/// Validate chunk size parameters
pub fn validate_chunk_config(chunk_size: usize, chunk_overlap: usize) -> (bool, Option<String>) {
    if chunk_size == 0 {
        return (false, Some("chunk_size must be > 0".to_string()));
    }

    if chunk_overlap >= chunk_size {
        return (false, Some("chunk_overlap must be < chunk_size".to_string()));
    }

    if chunk_size > 8192 {
        return (false, Some("chunk_size too large (max 8192)".to_string()));
    }

    (true, None)
}

/// Validate file path safety (prevent directory traversal)
pub fn is_safe_path(path: &str) -> bool {
    // Reject paths with suspicious patterns
    !path.contains("..") && !path.contains("~") && !path.starts_with('/')
}

// ============================================
// Path Processing
// ============================================

/// Filter indexable files from a list of paths
/// Pure function - no side effects
pub fn filter_indexable_files(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .filter(|p| {
            let ext = get_file_extension(Path::new(p));
            is_indexable_file(&ext)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================
    // Chunking Tests
    // ============================================

    #[test]
    fn test_chunk_text_simple() {
        let text = "Hello world this is a test";
        let chunks = chunk_text(text, 5);

        assert!(!chunks.is_empty());
        assert!(chunks.iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn test_chunk_text_empty() {
        let chunks = chunk_text("", 5);
        assert_eq!(chunks.len(), 0);
    }

    #[test]
    fn test_chunk_text_single_word() {
        let chunks = chunk_text("hello", 10);
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_chunk_text_preserves_content() {
        let text = "The quick brown fox jumps over the lazy dog";
        let chunks = chunk_text(text, 5);
        let reconstructed = chunks.join(" ");

        // Should contain original words
        assert!(reconstructed.contains("quick"));
        assert!(reconstructed.contains("brown"));
    }

    #[test]
    fn test_chunk_text_various_sizes() {
        let text = "word ".repeat(100);

        let chunks_small = chunk_text(&text, 5);
        let chunks_large = chunk_text(&text, 50);

        // Larger chunk size should result in fewer chunks
        assert!(chunks_large.len() <= chunks_small.len());
    }

    // ============================================
    // File Extension Tests
    // ============================================

    #[test]
    fn test_is_indexable_pdf() {
        assert!(is_indexable_file("pdf"));
        assert!(is_indexable_file("PDF"));
    }

    #[test]
    fn test_is_indexable_docx() {
        assert!(is_indexable_file("docx"));
        assert!(is_indexable_file("DOCX"));
    }

    #[test]
    fn test_is_indexable_txt() {
        assert!(is_indexable_file("txt"));
    }

    #[test]
    fn test_is_indexable_markdown() {
        assert!(is_indexable_file("md"));
    }

    #[test]
    fn test_is_indexable_html() {
        assert!(is_indexable_file("html"));
        assert!(is_indexable_file("htm"));
    }

    #[test]
    fn test_is_not_indexable_empty() {
        assert!(!is_indexable_file(""));
    }

    #[test]
    fn test_is_not_indexable_hidden() {
        assert!(!is_indexable_file("ds_store"));
        assert!(!is_indexable_file("gitignore"));
    }

    #[test]
    fn test_is_not_indexable_unknown() {
        assert!(!is_indexable_file("xyz"));
        assert!(!is_indexable_file("bin"));
    }

    #[test]
    fn test_is_indexable_case_insensitive() {
        assert!(is_indexable_file("PDF"));
        assert!(is_indexable_file("Pdf"));
        assert!(is_indexable_file("pDf"));
    }

    // ============================================
    // Get File Extension Tests
    // ============================================

    #[test]
    fn test_get_file_extension_simple() {
        let path = Path::new("file.pdf");
        assert_eq!(get_file_extension(path), "pdf");
    }

    #[test]
    fn test_get_file_extension_uppercase() {
        let path = Path::new("file.PDF");
        assert_eq!(get_file_extension(path), "pdf");
    }

    #[test]
    fn test_get_file_extension_with_path() {
        let path = Path::new("/path/to/file.docx");
        assert_eq!(get_file_extension(path), "docx");
    }

    #[test]
    fn test_get_file_extension_no_extension() {
        let path = Path::new("filename");
        assert_eq!(get_file_extension(path), "");
    }

    #[test]
    fn test_get_file_extension_multiple_dots() {
        let path = Path::new("archive.tar.gz");
        assert_eq!(get_file_extension(path), "gz");
    }

    // ============================================
    // Format File Size Tests
    // ============================================

    #[test]
    fn test_format_file_size_bytes() {
        assert_eq!(format_file_size(512), "512.00 B");
        assert_eq!(format_file_size(1), "1.00 B");
    }

    #[test]
    fn test_format_file_size_kb() {
        assert!(format_file_size(2048).contains("KB"));
    }

    #[test]
    fn test_format_file_size_mb() {
        assert!(format_file_size(1024 * 1024 * 5).contains("MB"));
    }

    #[test]
    fn test_format_file_size_gb() {
        assert!(format_file_size(1024 * 1024 * 1024 * 2).contains("GB"));
    }

    #[test]
    fn test_format_file_size_zero() {
        assert_eq!(format_file_size(0), "0.00 B");
    }

    // ============================================
    // Chunk Config Validation Tests
    // ============================================

    #[test]
    fn test_validate_chunk_config_valid() {
        let (valid, error) = validate_chunk_config(512, 64);
        assert!(valid);
        assert!(error.is_none());
    }

    #[test]
    fn test_validate_chunk_config_zero_size() {
        let (valid, error) = validate_chunk_config(0, 64);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chunk_config_overlap_exceeds_size() {
        let (valid, error) = validate_chunk_config(100, 100);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chunk_config_overlap_equals_size() {
        let (valid, error) = validate_chunk_config(100, 100);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chunk_config_size_too_large() {
        let (valid, error) = validate_chunk_config(10000, 64);
        assert!(!valid);
        assert!(error.is_some());
    }

    #[test]
    fn test_validate_chunk_config_valid_boundaries() {
        let (valid, _) = validate_chunk_config(1, 0);
        assert!(valid);

        let (valid, _) = validate_chunk_config(8192, 0);
        assert!(valid);

        let (valid, _) = validate_chunk_config(8192, 8191);
        assert!(valid);
    }

    // ============================================
    // Path Safety Tests
    // ============================================

    #[test]
    fn test_is_safe_path_normal() {
        assert!(is_safe_path("documents/file.pdf"));
    }

    #[test]
    fn test_is_safe_path_directory_traversal() {
        assert!(!is_safe_path("../etc/passwd"));
        assert!(!is_safe_path("../../sensitive"));
    }

    #[test]
    fn test_is_safe_path_home_expansion() {
        assert!(!is_safe_path("~/documents/file"));
    }

    #[test]
    fn test_is_safe_path_absolute() {
        assert!(!is_safe_path("/etc/passwd"));
    }

    #[test]
    fn test_is_safe_path_relative() {
        assert!(is_safe_path("documents/file.pdf"));
    }

    // ============================================
    // Filter Indexable Files Tests
    // ============================================

    #[test]
    fn test_filter_indexable_files_empty() {
        let result = filter_indexable_files(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_indexable_files_mixed() {
        let paths = vec![
            "document.pdf".to_string(),
            "image.png".to_string(),
            "notes.txt".to_string(),
            "script.py".to_string(),
        ];

        let result = filter_indexable_files(&paths);

        assert_eq!(result.len(), 2);
        assert!(result.contains(&"document.pdf".to_string()));
        assert!(result.contains(&"notes.txt".to_string()));
    }

    #[test]
    fn test_filter_indexable_files_all_indexable() {
        let paths = vec![
            "file1.pdf".to_string(),
            "file2.docx".to_string(),
            "file3.md".to_string(),
        ];

        let result = filter_indexable_files(&paths);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_filter_indexable_files_none_indexable() {
        let paths = vec![
            "image.png".to_string(),
            "video.mp4".to_string(),
            "archive.zip".to_string(),
        ];

        let result = filter_indexable_files(&paths);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_indexable_files_with_paths() {
        let paths = vec![
            "/documents/report.pdf".to_string(),
            "/downloads/image.jpg".to_string(),
            "/notes/readme.md".to_string(),
        ];

        let result = filter_indexable_files(&paths);
        assert_eq!(result.len(), 2);
    }
}
