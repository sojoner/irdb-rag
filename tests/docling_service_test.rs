use anyhow::Result;
use std::time::Instant;
use rag_chat::config::Settings;

#[tokio::test]
async fn test_docling_parsing_speed() -> Result<()> {
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new()?;
    let docling_url = settings.docling.url.clone();

    println!("\n📋 Testing Docling parsing speed and capabilities...\n");

    // Test with a small document for speed verification
    let test_file = "tests/test_data/HumanPrincipals.pdf";

    let start = Instant::now();
    let client = reqwest::Client::new();

    let form = reqwest::multipart::Form::new()
        .file("files", test_file)
        .await?;

    let response = client
        .post(format!("{}/v1/convert/file", docling_url))
        .multipart(form)
        .send()
        .await?;

    let elapsed = start.elapsed();

    assert!(response.status().is_success(), "Docling conversion failed");

    let result: serde_json::Value = response.json().await?;
    let content = result["document"]["md_content"].as_str()
        .ok_or_else(|| anyhow::anyhow!("No content in response"))?;

    println!("✅ Docling parsing completed in {:.2}s", elapsed.as_secs_f64());
    println!("📄 Content length: {} chars", content.len());
    println!("📊 Content preview (first 200 chars):");
    println!("{}", &content.chars().take(200).collect::<String>());

    // Verify parsing was reasonably fast (allow up to 30 seconds for CI/slower systems)
    assert!(elapsed.as_secs() < 30, "Docling parsing too slow: {:?}", elapsed);
    assert!(!content.is_empty(), "Content should not be empty");

    Ok(())
}

#[tokio::test]
async fn test_docling_table_detection() -> Result<()> {
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new()?;
    let docling_url = settings.docling.url.clone();

    println!("\n🔍 Testing Docling table detection...\n");

    // Use a document that likely contains tables
    let test_file = "tests/test_data/HumanPrincipals.pdf";

    let client = reqwest::Client::new();
    let form = reqwest::multipart::Form::new()
        .file("files", test_file)
        .await?;

    let response = client
        .post(format!("{}/v1/convert/file", docling_url))
        .multipart(form)
        .send()
        .await?;

    let result: serde_json::Value = response.json().await?;

    // Check if tables were detected
    let has_tables = result.get("tables")
        .and_then(|t| t.as_array())
        .map(|arr| !arr.is_empty())
        .unwrap_or(false);

    println!("📊 Tables detected: {}", has_tables);

    if let Some(tables) = result.get("tables").and_then(|t| t.as_array()) {
        println!("   Found {} table(s)", tables.len());
        for (i, table) in tables.iter().enumerate() {
            if let Some(rows) = table.get("rows").and_then(|r| r.as_u64()) {
                if let Some(cols) = table.get("cols").and_then(|c| c.as_u64()) {
                    println!("   Table {}: {}x{} cells", i + 1, rows, cols);
                }
            }
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_docling_metadata_extraction() -> Result<()> {
    std::env::set_var("RUN_ENV", "test");

    let settings = Settings::new()?;
    let docling_url = settings.docling.url.clone();

    println!("\n📝 Testing Docling metadata extraction...\n");

    let test_file = "tests/test_data/HumanPrincipals.pdf";

    let client = reqwest::Client::new();
    let form = reqwest::multipart::Form::new()
        .file("files", test_file)
        .await?;

    let response = client
        .post(format!("{}/v1/convert/file", docling_url))
        .multipart(form)
        .send()
        .await?;

    let result: serde_json::Value = response.json().await?;

    println!("📋 Metadata fields present:");
    if let Some(obj) = result.as_object() {
        for key in obj.keys() {
            if key != "content" {
                println!("   - {}: {:?}", key, obj.get(key));
            }
        }
    }

    Ok(())
}
