# Independent Chat Implementation

## Summary

Implemented a standalone chat interface with conversation history, similar to Claude Code, that can optionally include search results as context. The chat features a witty, wise, and agentic personality called "Sage" that acts as a creative research companion.

## Personality: Meet Sage 🚀

The chat uses a custom system prompt that creates an engaging, intellectually playful assistant that:
- **Thinks out loud** and shares its reasoning
- **Asks clarifying questions** instead of making assumptions
- **Admits uncertainty** when appropriate
- **Connects ideas creatively** across domains
- **Challenges gently** when it spots flawed reasoning
- **Celebrates insights** when users have breakthroughs

This makes conversations feel more like brainstorming with a brilliant, curious friend rather than just querying a knowledge base.

## Changes Made

### 1. Domain Models (DTOs)
**File**: `src/domain/dtos.rs`

Added new conversation-based chat types:
- `ChatMessage`: Represents a single message with role ("user" or "assistant") and content
- `ChatConversationRequest`: Accepts a full message history instead of just a single message
- `ChatConversationResponse`: Returns assistant's message in the same format

Kept legacy types for backward compatibility:
- `ChatRequest` (single message)
- `ChatResponse` (single message)

### 2. API Handlers
**File**: `src/api/handlers.rs`

Added two new endpoint handlers:

#### `chat_conversation`
- Non-streaming endpoint for conversation-based chat
- Accepts full conversation history
- Optionally retrieves document chunks if `document_ids` provided
- Injects document context into the last user message
- Returns assistant response as a `ChatMessage`

#### `chat_conversation_stream`
- Streaming (SSE) version of conversation chat
- Same features as above but streams the response
- Compatible with existing SSE client code

### 3. API Routes
**File**: `src/api/routes.rs`

Added routes:
- `POST /api/chat/conversation` → `chat_conversation`
- `POST /api/chat/conversation/stream` → `chat_conversation_stream`

### 4. UI Components

#### New Chat Component
**File**: `src/web_app/components/chat.rs`

Features:
- ✅ Full conversation history display
- ✅ User/assistant message bubbles
- ✅ Text area input with Enter to send (Shift+Enter for newline)
- ✅ Submit button (disabled while streaming)
- ✅ Streaming response updates
- ✅ Error handling and display
- ✅ Optional document context (can inject search results)
- ✅ Clean, modern UI with Tailwind CSS

#### Navigation Bar
**File**: `src/web_app/components/navbar.rs`

Simple navigation between:
- Search page
- Chat page
- Import page

### 5. Chat Page
**File**: `src/web_app/pages/chat.rs`

- Standalone page at `/chat` route
- Full-height chat interface
- Navigation bar at top
- Responsive container layout

### 6. Tests

#### Unit Tests
**File**: `tests/chat_conversation_test.rs`

Tests for:
- ChatMessage creation
- ChatConversationRequest with history
- ChatConversationRequest with search context
- ChatConversationResponse structure
- Empty conversation initialization

All tests pass ✅

#### Integration Tests
**File**: `tests/chat_conversation_api_test.rs`

Tests for:
- Request serialization/deserialization
- Conversation with document context
- Message history formatting

## How It Works

### Chat Flow

1. **User types message** → Added to conversation history
2. **Frontend sends request** to `/api/chat/conversation/stream` with:
   - Full message history: `[{role: "user", content: "..."}, ...]`
   - Optional `document_ids` to include as context
   - Optional `conversation_id` for tracking
3. **Backend**:
   - Extracts last user message for context retrieval
   - If `document_ids` provided, fetches and reranks relevant chunks
   - Injects document context into the last user message
   - Formats entire conversation for LLM
   - Streams LLM response back
4. **Frontend** updates assistant message in real-time
5. **Conversation continues** with full history preserved

### Adding Search Context

The chat can optionally receive document IDs to use as context:

```rust
ChatConversationRequest {
    messages: [...],
    document_ids: Some(vec![doc_id_1, doc_id_2]),  // Optional
    context_chunks: 5,
}
```

When provided, the backend:
1. Embeds the last user message
2. Retrieves relevant chunks from those documents
3. Optionally reranks with qwen3-reranker
4. Injects context into the conversation

## UI Features

### Chat Interface
- Clean message bubbles (blue for user, gray for assistant)
- Auto-scrolling message area
- Multi-line input with keyboard shortcuts
- Loading indicator while streaming
- Error messages displayed inline
- Context status indicator

### Navigation
- Easy switching between Search, Chat, and Import
- Consistent navigation bar across pages

## Backward Compatibility

✅ Old `/api/chat/stream` endpoint still works
✅ Old `ChatRequest`/`ChatResponse` types preserved
✅ Existing chat_panel component untouched

## Testing

Run tests:
```bash
# Unit tests
cargo test --test chat_conversation_test

# Integration tests (requires DB)
cargo test --test chat_conversation_api_test
```

## Usage

### Access the Chat
1. Start the server: `cargo leptos serve` or `make gpu-up`
2. Navigate to `http://localhost:3000/chat`
3. Start chatting!

### Add Search Context (Future Enhancement)
The component supports `initial_context_docs` prop:

```rust
<Chat initial_context_docs=Some(selected_document_ids_signal) />
```

This can be used to create a "Chat with selected documents" feature from the search page.

## Next Steps

Potential improvements:
1. Add "Add to context" button on search results
2. Persist conversation history to database
3. Show source citations in chat responses
4. Export conversation as markdown
5. Conversation management (list, delete, rename)
6. Streaming markdown rendering

## System Prompts

The implementation includes two distinct system prompts:

### RAG Mode (with document context)
Used when `document_ids` are provided - focused on accurately citing sources and answering from provided context.

**Config key**: `rag.system_prompt`

### Standalone Chat Mode (no context)
Used for general conversation - the creative "Sage" personality that's witty, wise, and agentic.

**Config key**: `rag.chat_system_prompt`

See [config/default.toml](../config/default.toml) for the full "Sage" prompt definition.

## Files Modified

- `src/domain/dtos.rs` - Added conversation DTOs
- `src/api/handlers.rs` - Added conversation endpoints with dual system prompt support
- `src/api/routes.rs` - Added conversation routes
- `src/config.rs` - Added `chat_system_prompt` field to `RagConfig`
- `config/default.toml` - Added creative "Sage" system prompt
- `src/web_app/components/chat.rs` - New chat component
- `src/web_app/components/navbar.rs` - New navigation
- `src/web_app/components/mod.rs` - Export new components
- `src/web_app/pages/chat.rs` - New chat page
- `src/web_app/pages/mod.rs` - Export chat page
- `src/web_app/app.rs` - Added `/chat` route
- `tests/chat_conversation_test.rs` - Unit tests
- `tests/chat_conversation_api_test.rs` - Integration tests
