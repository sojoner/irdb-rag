use crate::api::state::AppState;
use crate::domain::dtos::ChatMessage;
use crate::infra::llm;
use futures::stream::BoxStream;
use uuid::Uuid;

pub struct Moderator;
pub struct RagReporter;
pub struct DualAgentOrchestrator;

pub enum AgentResponse {
    Moderator {
        content: String,
        conversation_id: Uuid,
    },
    RagReporter {
        content: String,
        sources: Vec<crate::domain::dtos::SourceReference>,
        conversation_id: Uuid,
    },
}

pub struct DualAgentResponse {
    pub rag_reporter: Option<Result<AgentResponse, AgentError>>,
    pub moderator: Option<Result<AgentResponse, AgentError>>,
}

#[derive(Debug, Clone)]
pub enum AgentError {
    EmbeddingFailed(String),
    ChunkRetrievalFailed(String),
    LlmFailed(String),
    NoUserMessage,
    NoRelevantDocuments,
}

impl Moderator {
    pub async fn respond(
        state: &AppState,
        messages: &[ChatMessage],
        conversation_id: Uuid,
    ) -> Result<AgentResponse, AgentError> {
        let config = state.llm_config.read().await.clone();
        
        let default_prompt =
            "You are a helpful, friendly AI assistant. Be conversational, thoughtful, and engaging.";
        let system_prompt = state
            .settings
            .rag
            .chat_system_prompt
            .as_deref()
            .unwrap_or(default_prompt)
            .to_string();

        let user_prompt = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        match llm::call_llm(&config, &system_prompt, &user_prompt).await {
            Ok(response) => Ok(AgentResponse::Moderator {
                content: response,
                conversation_id,
            }),
            Err(e) => Err(AgentError::LlmFailed(e.to_string())),
        }
    }

    pub async fn stream_respond(
        state: &AppState,
        messages: &[ChatMessage],
    ) -> Result<BoxStream<'static, anyhow::Result<String>>, AgentError> {
        let config = state.llm_config.read().await.clone();
        
        let default_prompt =
            "You are a helpful, friendly AI assistant. Be conversational, thoughtful, and engaging.";
        let system_prompt = state
            .settings
            .rag
            .chat_system_prompt
            .as_deref()
            .unwrap_or(default_prompt)
            .to_string();

        let user_prompt = messages
            .iter()
            .map(|m| format!("{}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        llm::stream_llm(&config, &system_prompt, &user_prompt)
            .await
            .map_err(|e| AgentError::LlmFailed(e.to_string()))
    }
}

impl RagReporter {
    pub async fn respond(
        state: &AppState,
        messages: &[ChatMessage],
        context_chunks: i32,
        document_ids: Option<&[Uuid]>,
        conversation_id: Uuid,
    ) -> Result<AgentResponse, AgentError> {
        let last_user_message = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if last_user_message.is_empty() {
            return Err(AgentError::NoUserMessage);
        }

        let embedding = state
            .embedder
            .embed(&last_user_message)
            .await
            .map_err(|e| AgentError::EmbeddingFailed(e.to_string()))?;

        let chunks = crate::infra::db::get_relevant_chunks(
            &state.pool,
            &embedding,
            context_chunks,
            document_ids,
        )
        .await
        .map_err(|e| AgentError::ChunkRetrievalFailed(e.to_string()))?;

        if chunks.is_empty() {
            return Ok(AgentResponse::RagReporter {
                content: "No relevant documents found to answer this question.".to_string(),
                sources: vec![],
                conversation_id,
            });
        }

        let context: String = chunks
            .iter()
            .map(|c| format!("---\n{}\n", c.content))
            .collect();

        let default_system_prompt = "You are a knowledge search specialist. Analyze the provided document context and summarize the key findings relevant to the user's question. Be concise and cite specific parts of the context when relevant.";
        let system_prompt = state
            .settings
            .rag
            .system_prompt
            .as_deref()
            .unwrap_or(default_system_prompt)
            .to_string();

        let user_prompt = format!(
            "CONTEXT FROM DOCUMENTS:\n{}\n\nQUESTION:\n{}",
            context, last_user_message
        );

        let config = state.llm_config.read().await.clone();

        let sources: Vec<crate::domain::dtos::SourceReference> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| crate::domain::dtos::SourceReference {
                document_id: c.document_id,
                title: c
                    .section_title
                    .clone()
                    .unwrap_or_else(|| format!("Chunk {}", i + 1)),
                chunk: c.content.chars().take(200).collect::<String>() + "...",
                relevance: 1.0 - (i as f64 * 0.1),
            })
            .collect();

        match llm::call_llm(&config, &system_prompt, &user_prompt).await {
            Ok(response) => Ok(AgentResponse::RagReporter {
                content: response,
                sources,
                conversation_id,
            }),
            Err(e) => Err(AgentError::LlmFailed(e.to_string())),
        }
    }

    pub async fn stream_respond(
        state: &AppState,
        messages: &[ChatMessage],
        context_chunks: i32,
        document_ids: Option<&[Uuid]>,
    ) -> Result<(BoxStream<'static, anyhow::Result<String>>, Vec<crate::domain::dtos::SourceReference>), AgentError> {
        let last_user_message = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone())
            .unwrap_or_default();

        if last_user_message.is_empty() {
            return Err(AgentError::NoUserMessage);
        }

        let embedding = state
            .embedder
            .embed(&last_user_message)
            .await
            .map_err(|e| AgentError::EmbeddingFailed(e.to_string()))?;

        let mut chunks = crate::infra::db::get_relevant_chunks(
            &state.pool,
            &embedding,
            context_chunks,
            document_ids,
        )
        .await
        .map_err(|e| AgentError::ChunkRetrievalFailed(e.to_string()))?;

        if let Some(reranker) = state.reranker.as_ref() {
            let chunk_contents: Vec<&str> = chunks.iter().map(|c| c.content.as_str()).collect();
            if let Ok(ranked) = reranker
                .rerank_and_sort(&last_user_message, &chunk_contents)
                .await
            {
                let mut reranked = Vec::new();
                for doc in ranked {
                    if let Some(chunk) = chunks.get(doc.index) {
                        reranked.push(chunk.clone());
                    }
                }
                chunks = reranked;
            }
        }

        let context: String = chunks
            .iter()
            .map(|c| format!("---\n{}\n", c.content))
            .collect();

        let sources: Vec<crate::domain::dtos::SourceReference> = chunks
            .iter()
            .enumerate()
            .map(|(i, c)| crate::domain::dtos::SourceReference {
                document_id: c.document_id,
                title: c
                    .section_title
                    .clone()
                    .unwrap_or_else(|| format!("Chunk {}", i + 1)),
                chunk: c.content.chars().take(200).collect::<String>() + "...",
                relevance: 1.0 - (i as f64 * 0.1),
            })
            .collect();

        let default_system_prompt = "You are a knowledge search specialist. Analyze the provided document context and summarize the key findings relevant to the user's question. Be concise and cite specific parts of the context when relevant.";
        let system_prompt = state
            .settings
            .rag
            .system_prompt
            .as_deref()
            .unwrap_or(default_system_prompt)
            .to_string();

        let user_prompt = format!(
            "CONTEXT FROM DOCUMENTS:\n{}\n\nQUESTION:\n{}",
            context, last_user_message
        );

        let config = state.llm_config.read().await.clone();

        let stream = llm::stream_llm(&config, &system_prompt, &user_prompt)
            .await
            .map_err(|e| AgentError::LlmFailed(e.to_string()))?;

        Ok((stream, sources))
    }
}

impl DualAgentOrchestrator {
    pub async fn orchestrate_streaming(
        state: &AppState,
        messages: &[ChatMessage],
        context_chunks: i32,
        document_ids: Option<&[Uuid]>,
    ) -> (
        Result<BoxStream<'static, anyhow::Result<String>>, AgentError>,
        Result<(BoxStream<'static, anyhow::Result<String>>, Vec<crate::domain::dtos::SourceReference>), AgentError>,
    ) {
        let messages_clone = messages.to_vec();
        let messages_clone2 = messages.to_vec();

        let moderator_result = Moderator::stream_respond(state, &messages_clone).await;
        let rag_result = RagReporter::stream_respond(state, &messages_clone2, context_chunks, document_ids).await;

        (moderator_result, rag_result)
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::dtos::ChatMessage;
    use crate::services::agents::{AgentError, AgentResponse};
    use uuid::Uuid;

    #[test]
    fn test_agent_error_display() {
        let err = AgentError::NoUserMessage;
        assert!(format!("{:?}", err).contains("NoUserMessage"));

        let err = AgentError::EmbeddingFailed("API timeout".to_string());
        assert!(format!("{:?}", err).contains("EmbeddingFailed"));

        let err = AgentError::LlmFailed("Model error".to_string());
        assert!(format!("{:?}", err).contains("LlmFailed"));
    }

    #[test]
    fn test_valid_conversation_message() {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "What is AI?".to_string(),
        }];

        let user_msg = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone());

        assert!(user_msg.is_some());
        assert_eq!(user_msg.unwrap(), "What is AI?");
    }

    #[test]
    fn test_mixed_conversation_history() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Hello".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Hi there!".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "What is ML?".to_string(),
            },
        ];

        let last_user = messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.clone());

        assert_eq!(last_user.unwrap(), "What is ML?");
    }

    #[test]
    fn test_agent_response_moderator_structure() {
        let conversation_id = Uuid::new_v4();
        let content = "This is a test response".to_string();

        let response = AgentResponse::Moderator {
            content: content.clone(),
            conversation_id,
        };

        match response {
            AgentResponse::Moderator {
                content: resp_content,
                conversation_id: resp_id,
            } => {
                assert_eq!(resp_content, content);
                assert_eq!(resp_id, conversation_id);
            }
            _ => panic!("Expected Moderator response"),
        }
    }

    #[test]
    fn test_agent_response_rag_reporter_structure() {
        let conversation_id = Uuid::new_v4();
        let content = "Based on documents...".to_string();
        let sources = vec![];

        let response = AgentResponse::RagReporter {
            content: content.clone(),
            sources: sources.clone(),
            conversation_id,
        };

        match response {
            AgentResponse::RagReporter {
                content: resp_content,
                sources: resp_sources,
                conversation_id: resp_id,
            } => {
                assert_eq!(resp_content, content);
                assert_eq!(resp_sources.len(), 0);
                assert_eq!(resp_id, conversation_id);
            }
            _ => panic!("Expected RagReporter response"),
        }
    }

    #[test]
    fn test_message_role_detection() {
        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: "Q1".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "A1".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Q2".to_string(),
            },
        ];

        let user_count = messages.iter().filter(|m| m.role == "user").count();
        let assistant_count = messages.iter().filter(|m| m.role == "assistant").count();

        assert_eq!(user_count, 2);
        assert_eq!(assistant_count, 1);
    }

    #[test]
    fn test_error_variants_cloneable() {
        let err1 = AgentError::LlmFailed("test".to_string());
        let err2 = err1.clone();

        assert_eq!(format!("{:?}", err1), format!("{:?}", err2));
    }
}
