//! Chrome bookmark parser
//!
//! Parses Chrome bookmarks exported to JSON format and extracts URLs
//! for indexing in the knowledge base.

use anyhow::{anyhow, Result};
use serde_json::Value;
use std::fs;

/// Parse Chrome bookmarks from a JSON file
///
/// Recursively extracts URLs from bookmark_bar, other, and synced folders.
/// Filters out `chrome://` and `javascript:` URLs.
///
/// # Arguments
/// * `path` - Path to the Chrome bookmarks JSON file
///
/// # Returns
/// Vector of URLs extracted from bookmarks
pub fn parse_chrome_bookmarks(path: &str) -> Result<Vec<String>> {
    // Read the JSON file
    let content = fs::read_to_string(path)
        .map_err(|e| anyhow!("Failed to read Chrome bookmarks file: {}", e))?;

    let json: Value = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse Chrome bookmarks JSON: {}", e))?;

    let mut urls = Vec::new();

    // Extract URLs from roots
    if let Some(roots) = json.get("roots").and_then(|v| v.as_object()) {
        for (_folder_name, folder_data) in roots {
            extract_urls_from_folder(folder_data, &mut urls);
        }
    }

    tracing::info!("Extracted {} URLs from Chrome bookmarks", urls.len());
    Ok(urls)
}

/// Recursively extract URLs from a bookmark folder
fn extract_urls_from_folder(folder: &Value, urls: &mut Vec<String>) {
    // Process children if they exist
    if let Some(children) = folder.get("children").and_then(|v| v.as_array()) {
        for child in children {
            // Check if it's a URL (type === "url")
            if let Some("url") = child.get("type").and_then(|v| v.as_str()) {
                if let Some(url) = child.get("url").and_then(|v| v.as_str()) {
                    // Filter out special URLs
                    if !url.starts_with("chrome://") && !url.starts_with("javascript:") {
                        urls.push(url.to_string());
                    }
                }
            }
            // Recursively process folders
            else if let Some("folder") = child.get("type").and_then(|v| v.as_str()) {
                extract_urls_from_folder(child, urls);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_parse_chrome_bookmarks() {
        let temp_dir = TempDir::new().unwrap();
        let bookmarks_path = temp_dir.path().join("bookmarks.json");

        let bookmarks_content = json!({
            "roots": {
                "bookmark_bar": {
                    "children": [
                        {
                            "type": "url",
                            "url": "https://example.com",
                            "name": "Example"
                        },
                        {
                            "type": "folder",
                            "name": "Tech",
                            "children": [
                                {
                                    "type": "url",
                                    "url": "https://rust-lang.org",
                                    "name": "Rust"
                                }
                            ]
                        }
                    ]
                },
                "other": {
                    "children": [
                        {
                            "type": "url",
                            "url": "https://github.com",
                            "name": "GitHub"
                        }
                    ]
                }
            }
        });

        fs::write(
            &bookmarks_path,
            bookmarks_content.to_string(),
        ).unwrap();

        let urls = parse_chrome_bookmarks(bookmarks_path.to_str().unwrap()).unwrap();

        assert_eq!(urls.len(), 3);
        assert!(urls.contains(&"https://example.com".to_string()));
        assert!(urls.contains(&"https://rust-lang.org".to_string()));
        assert!(urls.contains(&"https://github.com".to_string()));
    }

    #[test]
    fn test_parse_chrome_bookmarks_filters_special_urls() {
        let temp_dir = TempDir::new().unwrap();
        let bookmarks_path = temp_dir.path().join("bookmarks.json");

        let bookmarks_content = json!({
            "roots": {
                "bookmark_bar": {
                    "children": [
                        {
                            "type": "url",
                            "url": "https://example.com",
                            "name": "Example"
                        },
                        {
                            "type": "url",
                            "url": "chrome://bookmarks",
                            "name": "Chrome Bookmarks"
                        },
                        {
                            "type": "url",
                            "url": "javascript:void(0)",
                            "name": "Script"
                        }
                    ]
                }
            }
        });

        fs::write(
            &bookmarks_path,
            bookmarks_content.to_string(),
        ).unwrap();

        let urls = parse_chrome_bookmarks(bookmarks_path.to_str().unwrap()).unwrap();

        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0], "https://example.com");
    }

    #[test]
    fn test_parse_chrome_bookmarks_file_not_found() {
        let result = parse_chrome_bookmarks("/nonexistent/path/bookmarks.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_chrome_bookmarks_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let bookmarks_path = temp_dir.path().join("bookmarks.json");

        fs::write(&bookmarks_path, "invalid json").unwrap();

        let result = parse_chrome_bookmarks(bookmarks_path.to_str().unwrap());
        assert!(result.is_err());
    }
}
