//! Context Management for Chat
//!
//! Handles token budgets, conversation history trimming, and context compression
//! to ensure LLM requests stay within token limits.

use crate::domain::dtos::ChatMessage;

/// Token budget manager for conversation context
///
/// Uses heuristic-based token counting: ~4 chars = 1 token (typical for OpenAI models)
/// This is a conservative estimate; actual token counts vary by model.
pub struct ContextBudget {
    /// Maximum total tokens for the entire request (input + output)
    pub max_total_tokens: usize,
    /// Reserved tokens for system prompt
    pub system_tokens: usize,
    /// Maximum tokens to reserve for model output
    pub output_reserve_tokens: usize,
}

impl ContextBudget {
    /// Create a new context budget with typical limits
    ///
    /// # Arguments
    /// * `max_total_tokens` - Model's maximum token limit (e.g., 4096 for smaller models)
    /// * `system_tokens` - Approximate tokens in system prompt (e.g., 100-200)
    /// * `output_reserve_tokens` - Tokens reserved for model output (e.g., 1024)
    pub fn new(max_total_tokens: usize, system_tokens: usize, output_reserve_tokens: usize) -> Self {
        Self {
            max_total_tokens,
            system_tokens,
            output_reserve_tokens,
        }
    }

    /// Get the available budget for conversation history and context
    pub fn available_budget(&self) -> usize {
        self.max_total_tokens
            .saturating_sub(self.system_tokens)
            .saturating_sub(self.output_reserve_tokens)
    }

    /// Estimate token count using heuristic: ~4 chars = 1 token
    fn estimate_tokens(text: &str) -> usize {
        // Split on whitespace and punctuation for more accurate estimate
        let word_count = text.split_whitespace().count();
        // Heuristic: average word is 5 chars + 1 space = 6 chars = ~1.5 tokens
        // Be conservative: use (word_count * 1.3) as estimate
        (word_count as f64 * 1.3).ceil() as usize
    }

    /// Count tokens in a ChatMessage
    pub fn count_message_tokens(message: &ChatMessage) -> usize {
        // Add 4 tokens for role + formatting overhead
        Self::estimate_tokens(&message.content) + 4
    }

    /// Count total tokens in a conversation history
    pub fn count_conversation_tokens(messages: &[ChatMessage]) -> usize {
        messages.iter().map(Self::count_message_tokens).sum()
    }

    /// Trim conversation history to fit within available budget
    ///
    /// Keeps the most recent messages within the token budget.
    /// Always keeps at least the last message (user's current query).
    ///
    /// # Strategy
    /// 1. Keep system message (always) - handled by caller
    /// 2. Remove oldest messages until conversation fits within budget
    /// 3. Minimum: keep at least 1 message (current user query)
    pub fn trim_conversation_history(&self, messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
        if messages.is_empty() {
            return messages;
        }

        let available = self.available_budget();
        let current_tokens = Self::count_conversation_tokens(&messages);

        if current_tokens <= available {
            tracing::debug!(
                "Conversation already fits within budget: {} tokens <= {} available",
                current_tokens,
                available
            );
            return messages;
        }

        tracing::info!(
            "Trimming conversation: {} tokens > {} available",
            current_tokens,
            available
        );

        // Keep recent messages, remove oldest first
        let mut trimmed = messages.clone();
        while trimmed.len() > 1 && Self::count_conversation_tokens(&trimmed) > available {
            trimmed.remove(0); // Remove oldest message
        }

        let new_tokens = Self::count_conversation_tokens(&trimmed);
        tracing::info!(
            "Trimmed conversation from {} to {} messages ({} tokens)",
            messages.len(),
            trimmed.len(),
            new_tokens
        );

        trimmed
    }

    /// Compress context chunks by truncating and joining
    ///
    /// Simple truncation strategy: limit total context to fit within remaining budget
    pub fn compress_context(&self, chunks: &[String]) -> String {
        if chunks.is_empty() {
            return String::new();
        }

        let available = self.available_budget();
        let context_budget = (available / 3).max(500); // Use ~1/3 of budget for context

        let mut context = String::new();
        let separator = "\n---\n";

        for chunk in chunks {
            if Self::estimate_tokens(&context) + Self::estimate_tokens(chunk) > context_budget {
                tracing::debug!(
                    "Context budget reached at {} tokens, stopping chunk inclusion",
                    Self::estimate_tokens(&context)
                );
                break;
            }
            if !context.is_empty() {
                context.push_str(separator);
            }
            context.push_str(chunk);
        }

        tracing::debug!(
            "Compressed context to {} tokens (budget: {})",
            Self::estimate_tokens(&context),
            context_budget
        );

        context
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_estimation() {
        // "Hello world" = 2 words * 1.3 = ~2.6 tokens, rounded up to 3
        let text = "Hello world";
        let tokens = ContextBudget::estimate_tokens(text);
        assert!(tokens >= 2 && tokens <= 4, "Unexpected token count: {}", tokens);
    }

    #[test]
    fn test_message_token_counting() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "Hello world".to_string(),
        };
        let tokens = ContextBudget::count_message_tokens(&msg);
        // Content tokens + 4 overhead
        assert!(tokens >= 6, "Should include overhead");
    }

    #[test]
    fn test_trim_conversation() {
        let budget = ContextBudget::new(100, 20, 20); // 60 tokens available

        let messages = vec![
            ChatMessage {
                role: "user".to_string(),
                content: "First message with some content".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Response to first message here".to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: "Second message".to_string(),
            },
            ChatMessage {
                role: "assistant".to_string(),
                content: "Final response".to_string(),
            },
        ];

        let trimmed = budget.trim_conversation_history(messages.clone());

        // Should trim to recent messages
        assert!(trimmed.len() <= messages.len());
        assert!(
            ContextBudget::count_conversation_tokens(&trimmed) <= budget.available_budget(),
            "Trimmed conversation still exceeds budget"
        );
        // Should keep at least the last message
        assert!(!trimmed.is_empty());
    }

    #[test]
    fn test_never_trim_to_empty() {
        let budget = ContextBudget::new(50, 30, 10); // Very tight budget (10 tokens)

        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "This is a long message with many words that will definitely exceed the tiny budget available"
                .to_string(),
        }];

        let trimmed = budget.trim_conversation_history(messages);

        // Should always keep at least 1 message
        assert_eq!(trimmed.len(), 1);
        assert_eq!(trimmed[0].role, "user");
    }

    #[test]
    fn test_compress_context() {
        let budget = ContextBudget::new(400, 50, 100); // 250 tokens available

        let chunks = vec![
            "This is the first chunk of context that is quite long".to_string(),
            "This is the second chunk of context information".to_string(),
            "This is the third chunk".to_string(),
        ];

        let compressed = budget.compress_context(&chunks);

        let compressed_tokens = ContextBudget::estimate_tokens(&compressed);
        assert!(
            compressed_tokens < 150,
            "Compressed context should fit within ~1/3 of budget"
        );
    }
}
