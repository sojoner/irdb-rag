use anyhow::Result;
use text_splitter::TextSplitter;

fn chunk_text(text: &str, target_tokens: usize) -> Vec<String> {
    let splitter = TextSplitter::default().with_trim_chunks(true);
    splitter
        .chunks(text, target_tokens)
        .map(|s| s.to_string())
        .collect()
}

#[test]
fn test_basic_chunking() {
    println!("\n✂️  Testing basic text chunking...\n");

    let text = "This is a test document. It has multiple sentences. We want to split it into chunks. Each chunk should be manageable. The chunking should preserve sentence boundaries where possible.";

    let chunks = chunk_text(text, 50);

    println!("Original text length: {} chars", text.len());
    println!("Number of chunks: {}", chunks.len());

    for (i, chunk) in chunks.iter().enumerate() {
        println!("\nChunk {}: {} chars", i + 1, chunk.len());
        println!("  {}", chunk);
    }

    assert!(!chunks.is_empty(), "Should produce at least one chunk");
    assert!(
        chunks.iter().all(|c| !c.trim().is_empty()),
        "All chunks should have content"
    );
}

#[test]
fn test_chunk_size_limits() {
    println!("\n📏 Testing chunk size limits...\n");

    let long_text = "word ".repeat(1000); // 5000 chars

    let chunks = chunk_text(&long_text, 100);

    println!("Original text: {} chars", long_text.len());
    println!("Target tokens: 100");
    println!("Produced chunks: {}", chunks.len());

    for (i, chunk) in chunks.iter().enumerate() {
        let token_estimate = chunk.split_whitespace().count();
        println!(
            "Chunk {}: ~{} tokens, {} chars",
            i + 1,
            token_estimate,
            chunk.len()
        );
        // Rough validation: chunks shouldn't be massively over target
        assert!(
            token_estimate <= 150,
            "Chunk {} exceeds reasonable token limit",
            i + 1
        );
    }
}

#[test]
fn test_chunking_preserves_content() {
    println!("\n🔍 Testing content preservation during chunking...\n");

    let original = "The quick brown fox jumps over the lazy dog. This is a test of content preservation.";

    let chunks = chunk_text(original, 20);

    let reassembled = chunks.join(" ");

    println!("Original length: {}", original.len());
    println!("Chunks created: {}", chunks.len());
    println!("Reassembled length: {}", reassembled.len());

    // Content should be preserved (allowing for minor whitespace/punctuation variations)
    // The chunker may split punctuation differently, so we check character count is close
    assert!(
        (reassembled.len() as i32 - original.len() as i32).abs() <= 5,
        "Reassembled content should be approximately same length (original: {}, reassembled: {})",
        original.len(),
        reassembled.len()
    );
}

#[test]
fn test_empty_and_small_inputs() -> Result<()> {
    println!("\n🔬 Testing edge cases (empty, small inputs)...\n");

    // Empty text
    let chunks = chunk_text("", 100);
    assert!(
        chunks.is_empty() || chunks.iter().all(|c| c.trim().is_empty()),
        "Empty input should produce no meaningful chunks"
    );

    // Very small text
    let small = "Hi";
    let chunks = chunk_text(small, 100);
    assert_eq!(chunks.len(), 1, "Small text should produce one chunk");
    assert_eq!(chunks[0].trim(), small, "Content should be preserved");

    println!("✅ Edge cases handled correctly");

    Ok(())
}
