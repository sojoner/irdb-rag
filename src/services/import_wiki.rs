use anyhow::Result;
use sqlx::PgPool;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use uuid::Uuid;
use crate::services::import::{ImportJobRunner, ImportConfig};
use quick_xml::events::Event;
use quick_xml::Reader;
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct WikiPage {
    pub title: String,
    pub content: String,
    pub categories: Vec<String>,
}

pub async fn import_wikipedia_dump(
    pool: &PgPool,
    job_id: Uuid,
    path: PathBuf,
) -> Result<()> {
    let config = ImportConfig::from_env();
    let runner = ImportJobRunner::new(config);

    tracing::info!("Starting Wikipedia import from {:?}", path);
    runner.update_job_status(pool, job_id, "running").await?;

    match process_wiki_stream(pool, &path, &runner, job_id).await {
        Ok((total, processed, failed)) => {
            runner.complete_job(pool, job_id, "completed", None).await?;
            tracing::info!(
                "Wikipedia import completed: total={}, processed={}, failed={}",
                total,
                processed,
                failed
            );
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Wikipedia import failed: {}", e);
            runner.complete_job(pool, job_id, "failed", Some(&error_msg)).await?;
            Err(e)
        }
    }
}

async fn process_wiki_stream(
    pool: &PgPool,
    path: &PathBuf,
    runner: &ImportJobRunner,
    job_id: Uuid,
) -> Result<(usize, usize, usize)> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let decompressed = bzip2::read::BzDecoder::new(reader);
    let buffered = BufReader::new(decompressed);

    let mut xml_reader = Reader::from_reader(buffered);
    let mut buf = Vec::new();
    let mut pages_batch = Vec::new();
    let mut total = 0;
    let mut processed = 0;
    let mut failed = 0;

    let mut current_page: Option<WikiPage> = None;
    let mut in_title = false;
    let mut in_text = false;
    let mut in_category = false;

    loop {
        match xml_reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                match e.name().as_ref() {
                    b"page" => {
                        current_page = Some(WikiPage {
                            title: String::new(),
                            content: String::new(),
                            categories: Vec::new(),
                        });
                    }
                    b"title" => in_title = true,
                    b"text" => in_text = true,
                    _ => {}
                }
            }
            Ok(Event::Text(e)) => {
                let text = String::from_utf8_lossy(e.as_ref()).to_string();
                
                if let Some(ref mut page) = current_page {
                    if in_title {
                        page.title.push_str(&text);
                    } else if in_text {
                        page.content.push_str(&text);
                    } else if in_category {
                        page.categories.push(text);
                    }
                }
            }
            Ok(Event::End(e)) => {
                match e.name().as_ref() {
                    b"page" => {
                        if let Some(page) = current_page.take() {
                            if !page.title.is_empty() {
                                pages_batch.push(page);
                                total += 1;

                                if pages_batch.len() >= 100 {
                                    match batch_insert_pages(pool, job_id, &pages_batch).await {
                                        Ok(_) => processed += pages_batch.len(),
                                        Err(e) => {
                                            tracing::warn!("Batch insert failed: {}", e);
                                            failed += pages_batch.len();
                                        }
                                    }

                                    if let Err(e) = runner.update_job_progress(
                                        pool,
                                        job_id,
                                        6400000,
                                        processed as i32,
                                        failed as i32,
                                        0,
                                    ).await {
                                        tracing::error!("Failed to update progress: {}", e);
                                    }

                                    pages_batch.clear();
                                }
                            }
                        }
                    }
                    b"title" => in_title = false,
                    b"text" => in_text = false,
                    b"category" => in_category = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => {
                if !pages_batch.is_empty() {
                    match batch_insert_pages(pool, job_id, &pages_batch).await {
                        Ok(_) => processed += pages_batch.len(),
                        Err(e) => {
                            tracing::warn!("Final batch insert failed: {}", e);
                            failed += pages_batch.len();
                        }
                    }
                }
                break;
            }
            Err(e) => {
                tracing::error!("XML parsing error: {}", e);
                return Err(anyhow::anyhow!("XML parsing error: {}", e));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok((total, processed, failed))
}

async fn batch_insert_pages(pool: &PgPool, _job_id: Uuid, pages: &[WikiPage]) -> Result<()> {
     let mut tx = pool.begin().await?;

     for page in pages {
         let doc_id = Uuid::new_v4();

         sqlx::query(
             r#"
             INSERT INTO documents (id, title, content, status, source_type, metadata)
             VALUES ($1, $2, $3, $4, $5, $6)
             "#,
         )
         .bind(doc_id)
         .bind(&page.title)
         .bind(clean_wiki_markup(&page.content))
         .bind("imported")
         .bind("wikipedia")
         .bind(serde_json::json!({
             "categories": page.categories,
             "import_timestamp": Utc::now().to_rfc3339(),
         }))
         .execute(&mut *tx)
         .await?;
     }

     tx.commit().await?;
     Ok(())
}

fn clean_wiki_markup(content: &str) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();
    let mut in_link = false;
    let mut in_template = false;
    let mut brace_depth = 0;

    while let Some(ch) = chars.next() {
        match ch {
            '[' => {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    in_link = true;
                } else {
                    result.push(ch);
                }
            }
            ']' => {
                if in_link && chars.peek() == Some(&']') {
                    chars.next();
                    in_link = false;
                } else {
                    result.push(ch);
                }
            }
            '{' => {
                if !in_link {
                    brace_depth += 1;
                    in_template = brace_depth > 0;
                }
            }
            '}' => {
                if !in_link && brace_depth > 0 {
                    brace_depth -= 1;
                    in_template = brace_depth > 0;
                } else {
                    result.push(ch);
                }
            }
            _ => {
                if !in_link && !in_template {
                    result.push(ch);
                }
            }
        }
    }

    result
        .lines()
        .filter(|line| !line.trim().starts_with("==") && !line.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_wiki_markup_links() {
        let input = "This is a [[link]] text";
        let output = clean_wiki_markup(input);
        assert!(!output.contains("[["));
        assert!(output.contains("This is a") && output.contains("text"));
    }

    #[test]
    fn test_clean_wiki_markup_templates() {
        let input = "Text {template} more";
        let output = clean_wiki_markup(input);
        assert!(!output.contains("{"));
        assert!(output.contains("Text") && output.contains("more"));
    }

    #[test]
    fn test_clean_wiki_markup_headers() {
        let input = "== Header ==\nContent here";
        let output = clean_wiki_markup(input);
        assert!(!output.contains("=="));
        assert!(output.contains("Content here"));
    }

    #[test]
    fn test_wiki_page_creation() {
        let page = WikiPage {
            title: "Test".to_string(),
            content: "Content".to_string(),
            categories: vec!["Category1".to_string()],
        };
        assert_eq!(page.title, "Test");
        assert_eq!(page.content, "Content");
        assert_eq!(page.categories.len(), 1);
    }
}
