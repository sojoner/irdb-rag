use uuid::Uuid;
use crate::domain::dtos::ChatMessage;

#[derive(Debug, Clone)]
pub struct StreamResult {
    pub conversation_id: Option<Uuid>,
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_stream(
    url: &str,
    body: &str,
    on_chunk: impl Fn(String, Option<String>) + 'static,
) -> Result<StreamResult, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use leptos::web_sys;
    use futures::stream::StreamExt;
    use wasm_streams::ReadableStream;

    leptos::logging::log!("fetch_stream: Starting request to {}", url);
    leptos::logging::log!("fetch_stream: Body: {}", body);

    let window = web_sys::window().ok_or("No window")?;

    let init = web_sys::RequestInit::new();
    init.set_method("POST");
    init.set_body(&JsValue::from_str(body));

    let request = web_sys::Request::new_with_str_and_init(url, &init).map_err(|e| {
        leptos::logging::error!("Failed to create request: {:?}", e);
        "Failed to create request".to_string()
    })?;

    request
        .headers()
        .set("Content-Type", "application/json")
        .map_err(|_| "Failed to set header".to_string())?;

    leptos::logging::log!("fetch_stream: Calling window.fetch_with_request");
    let promise = window.fetch_with_request(&request);
    let resp_promise: JsFuture = promise.into();
    leptos::logging::log!("fetch_stream: Awaiting fetch response");
    let resp = resp_promise.await.map_err(|e| {
        leptos::logging::error!("fetch_stream: Fetch promise failed: {:?}", e);
        "Fetch failed".to_string()
    })?;
    leptos::logging::log!("fetch_stream: Got fetch response");

    let resp = web_sys::Response::from(resp);

    leptos::logging::log!("fetch_stream: Response status: {}", resp.status());

    if !resp.ok() {
        let error_msg = format!("HTTP {}", resp.status());
        leptos::logging::error!("fetch_stream: {}", error_msg);
        return Err(error_msg);
    }

    let body = resp.body().ok_or("No response body")?;
    let stream = ReadableStream::from_raw(body.unchecked_into()).into_stream();
    let mut stream = stream.map(|chunk| {
        let chunk = chunk.map_err(|_| "Stream error")?;
        let chunk = chunk.unchecked_into::<js_sys::Uint8Array>();
        let vec = chunk.to_vec();
        String::from_utf8(vec).map_err(|_| "Invalid UTF-8")
    });

    let mut buffer = String::new();
    let mut conversation_id: Option<Uuid> = None;

    while let Some(result) = stream.next().await {
        match result {
            Ok(text) => {
                buffer.push_str(&text);
                leptos::logging::log!("fetch_stream: Buffer size: {}", buffer.len());
                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].to_string();
                    buffer.drain(..=pos);

                    leptos::logging::log!("fetch_stream: Processing line: {}", line);

                    if let Some(data) = line.strip_prefix("data: ") {
                        leptos::logging::log!("fetch_stream: Found data line: {}", data);
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            leptos::logging::log!("fetch_stream: Parsed JSON: {}", json);
                            let agent = json.get("agent").and_then(|a| a.as_str()).map(|s| s.to_string());
                            let event_type = json.get("type").and_then(|t| t.as_str());
                            
                            if let Some("chunk") = event_type {
                                if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
                                    leptos::logging::log!(
                                        "fetch_stream: Got chunk content from {}: {}",
                                        agent.as_deref().unwrap_or("unknown"),
                                        content
                                    );
                                    on_chunk(content.to_string(), agent);
                                }
                            } else if let Some("complete") = event_type {
                                if let Some(conv_id_str) = json.get("conversation_id").and_then(|id| id.as_str()) {
                                    if let Ok(conv_id) = Uuid::parse_str(conv_id_str) {
                                        leptos::logging::log!("fetch_stream: Captured conversation_id: {}", conv_id);
                                        conversation_id = Some(conv_id);
                                    }
                                }
                            } else if let Some("error") = event_type {
                                if let Some(msg) = json.get("message").and_then(|m| m.as_str()) {
                                    let error_msg = format!("Server error: {}", msg);
                                    leptos::logging::error!("fetch_stream: {}", error_msg);
                                    return Err(error_msg);
                                }
                            }
                        } else {
                            leptos::logging::warn!("fetch_stream: Failed to parse JSON: {}", data);
                        }
                    }
                }
            }
            Err(e) => {
                leptos::logging::error!("fetch_stream: Stream error: {:?}", e);
                return Err("Stream error".to_string());
            }
        }
    }

    if !buffer.is_empty() {
        leptos::logging::log!("fetch_stream: Processing final buffer: {}", buffer);
        if let Some(data) = buffer.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                let event_type = json.get("type").and_then(|t| t.as_str());
                match event_type {
                    Some("chunk") => {
                        let agent = json.get("agent").and_then(|a| a.as_str()).map(|s| s.to_string());
                        if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
                            on_chunk(content.to_string(), agent);
                        }
                    }
                    Some("complete") => {
                        if let Some(conv_id_str) = json.get("conversation_id").and_then(|id| id.as_str()) {
                            if let Ok(conv_id) = Uuid::parse_str(conv_id_str) {
                                leptos::logging::log!("fetch_stream: Captured conversation_id from final buffer: {}", conv_id);
                                conversation_id = Some(conv_id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    Ok(StreamResult { conversation_id })
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub async fn fetch_stream(
    _url: &str,
    _body: &str,
    _on_chunk: impl Fn(String, Option<String>) + 'static,
) -> Result<StreamResult, String> {
    Err("Client-side only".to_string())
}

#[cfg(target_arch = "wasm32")]
pub async fn fetch_conversation_messages(conversation_id: Uuid) -> Result<Vec<ChatMessage>, String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen_futures::JsFuture;
    use leptos::web_sys;

    let window = web_sys::window().ok_or("No window")?;
    let url = format!("/api/conversations/{}", conversation_id);

    leptos::logging::log!("Fetching conversation from: {}", url);

    let resp_promise = window.fetch_with_str(&url);
    let resp = JsFuture::from(resp_promise)
        .await
        .map_err(|e| format!("Fetch failed: {:?}", e))?;

    let resp: web_sys::Response = resp.dyn_into().map_err(|_| "Invalid response")?;

    if !resp.ok() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let json_promise = resp
        .json()
        .map_err(|_| "Failed to parse response")?;
    let json = JsFuture::from(json_promise)
        .await
        .map_err(|_| "Failed to read JSON")?;

    let json_str = js_sys::JSON::stringify(&json)
        .map_err(|_| "Failed to stringify")?
        .as_string()
        .ok_or("Failed to convert to string")?;

    #[derive(serde::Deserialize)]
    struct ConversationResponse {
        messages: Vec<MessageData>,
    }

    #[derive(serde::Deserialize)]
    struct MessageData {
        role: String,
        content: String,
    }

    let response: ConversationResponse = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    Ok(response
        .messages
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: m.content,
        })
        .collect())
}

#[cfg(not(target_arch = "wasm32"))]
pub async fn fetch_conversation_messages(_conversation_id: Uuid) -> Result<Vec<ChatMessage>, String> {
    Err("Client-side only".to_string())
}
