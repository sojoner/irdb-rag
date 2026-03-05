use uuid::Uuid;
use crate::domain::dtos::ChatMessage;

#[cfg(target_arch = "wasm32")]
pub async fn fetch_stream(
    url: &str,
    body: &str,
    on_chunk: impl Fn(String, Option<String>) + 'static,
) -> Result<(), String> {
    use wasm_bindgen::JsCast;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_futures::JsFuture;
    use leptos::web_sys;

    leptos::logging::log!("fetch_stream: Starting request to {}", url);
    leptos::logging::log!("fetch_stream: Body: {}", body);

    let window = web_sys::window().ok_or("No window")?;

    // Create request options
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

    // Fetch and convert Promise to Future
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

    let text_promise = resp.text().map_err(|_| "Failed to get response text")?;
    let text_future = wasm_bindgen_futures::JsFuture::from(text_promise);
    let text_value: JsValue = text_future.await.map_err(|_| "Failed to read response text")?;
    let response_text = text_value
        .as_string()
        .ok_or("Response text is not a string")?;

    leptos::logging::log!("fetch_stream: Got response text of length: {}", response_text.len());

    let mut buffer = response_text.clone();

    while let Some(pos) = buffer.find('\n') {
        let line = buffer[..pos].to_string();
        buffer.drain(..=pos); // Remove line and newline

        leptos::logging::log!("fetch_stream: Processing line: {}", line);

        if let Some(data) = line.strip_prefix("data: ") {
            leptos::logging::log!("fetch_stream: Found data line: {}", data);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                leptos::logging::log!("fetch_stream: Parsed JSON: {}", json);
                let agent = json.get("agent").and_then(|a| a.as_str()).map(|s| s.to_string());
                if let Some("chunk") = json.get("type").and_then(|t| t.as_str()) {
                    if let Some(content) = json.get("content").and_then(|c| c.as_str()) {
                        leptos::logging::log!(
                            "fetch_stream: Got chunk content from {}: {}",
                            agent.as_deref().unwrap_or("unknown"),
                            content
                        );
                        on_chunk(content.to_string(), agent);
                    }
                } else if let Some("error") = json.get("type").and_then(|t| t.as_str()) {
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
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub async fn fetch_stream(
    _url: &str,
    _body: &str,
    _on_chunk: impl Fn(String) + 'static,
) -> Result<(), String> {
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
