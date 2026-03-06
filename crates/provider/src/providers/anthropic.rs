//! Anthropic Messages API provider.
//!
//! Implements the LlmProvider trait for the Anthropic Messages API.
//! Supports streaming via SSE with events: message_start, content_block_start,
//! content_block_delta, content_block_stop, message_delta, message_stop.

use std::collections::HashMap;
use std::pin::Pin;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures::Stream;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::provider::{
    ChatRequest, ChatResponse, ContentPart, FinishReason, LlmProvider, Role, StreamEvent, ToolCall,
    Usage,
};
use crate::sse::{self, parse_sse_json};

const DEFAULT_API_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";

/// Anthropic provider configuration.
#[derive(Debug, Clone)]
pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_API_URL.to_string(),
            client: reqwest::Client::new(),
        }
    }

    /// Set a custom base URL (e.g., for proxying).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Build the request body for the Messages API.
    fn build_request_body(&self, request: &ChatRequest, stream: bool) -> Result<serde_json::Value> {
        let mut body = serde_json::Map::new();

        body.insert(
            "model".into(),
            serde_json::Value::String(request.model.clone()),
        );

        // Extract system messages and convert the rest.
        let mut system_parts: Vec<serde_json::Value> = Vec::new();
        let mut messages: Vec<serde_json::Value> = Vec::new();

        for msg in &request.messages {
            match msg.role {
                Role::System => {
                    for part in &msg.content {
                        if let ContentPart::Text { text } = part {
                            let mut sys_block = serde_json::Map::new();
                            sys_block.insert("type".into(), "text".into());
                            sys_block.insert("text".into(), text.clone().into());

                            // Support cache_control on system messages.
                            if let Some(cache) = request.provider_options.get("cache_control") {
                                sys_block.insert("cache_control".into(), cache.clone());
                            }

                            system_parts.push(serde_json::Value::Object(sys_block));
                        }
                    }
                }
                Role::User | Role::Assistant => {
                    let role_str = match msg.role {
                        Role::User => "user",
                        Role::Assistant => "assistant",
                        _ => unreachable!(),
                    };
                    let content = self.convert_content_parts(&msg.content);
                    let mut m = serde_json::Map::new();
                    m.insert("role".into(), role_str.into());
                    m.insert("content".into(), content);
                    messages.push(serde_json::Value::Object(m));
                }
                Role::Tool => {
                    // Tool results are sent as user messages with tool_result content blocks.
                    let content = self.convert_content_parts(&msg.content);
                    let mut m = serde_json::Map::new();
                    m.insert("role".into(), "user".into());
                    m.insert("content".into(), content);
                    messages.push(serde_json::Value::Object(m));
                }
            }
        }

        if !system_parts.is_empty() {
            body.insert("system".into(), serde_json::Value::Array(system_parts));
        }

        body.insert("messages".into(), serde_json::Value::Array(messages));

        // Max tokens - required by Anthropic.
        let max_tokens = request.max_tokens.unwrap_or(8192);
        body.insert(
            "max_tokens".into(),
            serde_json::Value::Number(max_tokens.into()),
        );

        if let Some(temp) = request.temperature {
            body.insert(
                "temperature".into(),
                serde_json::Number::from_f64(temp)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
            );
        }

        if let Some(top_p) = request.top_p {
            body.insert(
                "top_p".into(),
                serde_json::Number::from_f64(top_p)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null),
            );
        }

        // Tools.
        if !request.tools.is_empty() {
            let tools: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.parameters,
                    })
                })
                .collect();
            body.insert("tools".into(), serde_json::Value::Array(tools));
        }

        if stream {
            body.insert("stream".into(), serde_json::Value::Bool(true));
        }

        // Extended thinking support.
        if let Some(thinking) = request.provider_options.get("thinking") {
            body.insert("thinking".into(), thinking.clone());
        }

        Ok(serde_json::Value::Object(body))
    }

    /// Convert content parts to Anthropic's format.
    fn convert_content_parts(&self, parts: &[ContentPart]) -> serde_json::Value {
        let blocks: Vec<serde_json::Value> = parts
            .iter()
            .map(|part| match part {
                ContentPart::Text { text } => {
                    serde_json::json!({
                        "type": "text",
                        "text": text,
                    })
                }
                ContentPart::Image { data, media_type } => {
                    serde_json::json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": media_type,
                            "data": data,
                        }
                    })
                }
                ContentPart::ToolUse { id, name, input } => {
                    serde_json::json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                        "input": input,
                    })
                }
                ContentPart::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": content,
                        "is_error": is_error,
                    })
                }
            })
            .collect();

        // If there's only one text block, Anthropic also accepts a plain string.
        // But array format is always valid, so we use that.
        serde_json::Value::Array(blocks)
    }

    /// Build HTTP headers for the Anthropic API.
    fn build_headers(&self, request: &ChatRequest) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("x-api-key", self.api_key.parse().unwrap());
        headers.insert("anthropic-version", API_VERSION.parse().unwrap());
        headers.insert("content-type", "application/json".parse().unwrap());

        // Add beta header for extended thinking.
        if request.provider_options.contains_key("thinking")
            && let Ok(val) = "interleaved-thinking-2025-05-14".parse()
        {
            headers.insert("anthropic-beta", val);
        }

        headers
    }

    /// Parse a non-streaming Anthropic response.
    fn parse_response(&self, body: serde_json::Value) -> Result<ChatResponse> {
        let content_blocks = body["content"]
            .as_array()
            .context("Missing content in response")?;

        let mut text_parts = Vec::new();
        let mut tool_calls = Vec::new();

        for block in content_blocks {
            match block["type"].as_str() {
                Some("text") => {
                    if let Some(t) = block["text"].as_str() {
                        text_parts.push(t.to_string());
                    }
                }
                Some("tool_use") => {
                    tool_calls.push(ToolCall {
                        id: block["id"].as_str().unwrap_or_default().to_string(),
                        name: block["name"].as_str().unwrap_or_default().to_string(),
                        arguments: block["input"].clone(),
                    });
                }
                _ => {}
            }
        }

        let usage = parse_anthropic_usage(&body["usage"]);
        let finish_reason = match body["stop_reason"].as_str() {
            Some("end_turn") | Some("stop") => FinishReason::Stop,
            Some("tool_use") => FinishReason::ToolUse,
            Some("max_tokens") => FinishReason::MaxTokens,
            _ => FinishReason::Stop,
        };

        Ok(ChatResponse {
            content: text_parts.join(""),
            tool_calls,
            usage,
            finish_reason,
        })
    }
}

fn parse_anthropic_usage(usage: &serde_json::Value) -> Usage {
    Usage {
        input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
        output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens: usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
        cache_creation_tokens: usage["cache_creation_input_tokens"].as_u64().unwrap_or(0),
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    fn id(&self) -> &str {
        "anthropic"
    }

    fn name(&self) -> &str {
        "Anthropic"
    }

    async fn chat(&self, request: ChatRequest, cancel: CancellationToken) -> Result<ChatResponse> {
        let body = self.build_request_body(&request, false)?;
        let headers = self.build_headers(&request);
        let url = format!("{}/v1/messages", self.base_url);

        debug!(url = %url, model = %request.model, "Sending Anthropic chat request");

        let response = tokio::select! {
            result = self.client.post(&url).headers(headers).json(&body).send() => {
                result.context("Failed to send Anthropic request")?
            }
            _ = cancel.cancelled() => {
                bail!("Request cancelled");
            }
        };

        let status = response.status();
        let response_body: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse Anthropic response body")?;

        if !status.is_success() {
            let error_msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            bail!("Anthropic API error ({}): {}", status, error_msg);
        }

        self.parse_response(response_body)
    }

    async fn stream(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let body = self.build_request_body(&request, true)?;
        let headers = self.build_headers(&request);
        let url = format!("{}/v1/messages", self.base_url);

        debug!(url = %url, model = %request.model, "Sending Anthropic streaming request");

        let response = tokio::select! {
            result = self.client.post(&url).headers(headers).json(&body).send() => {
                result.context("Failed to send Anthropic streaming request")?
            }
            _ = cancel.cancelled() => {
                bail!("Request cancelled");
            }
        };

        let status = response.status();
        if !status.is_success() {
            let error_body: serde_json::Value = response
                .json()
                .await
                .unwrap_or(serde_json::json!({"error": {"message": "Unknown error"}}));
            let error_msg = error_body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            bail!("Anthropic API error ({}): {}", status, error_msg);
        }

        let sse_stream = sse::parse_sse(response);
        let event_stream = AnthropicStreamAdapter::new(sse_stream);

        Ok(Box::pin(event_stream))
    }
}

/// Adapts Anthropic SSE events into our unified StreamEvent format.
struct AnthropicStreamAdapter {
    inner: Pin<Box<dyn Stream<Item = Result<sse::SseEvent>> + Send>>,
    /// Accumulated usage from message_start.
    usage: Usage,
    /// Track current content blocks for tool call mapping.
    current_blocks: HashMap<u64, ContentBlockState>,
    /// Tool call index counter.
    tool_call_index: usize,
}

#[derive(Debug)]
enum ContentBlockState {
    Text,
    ToolUse {
        index: usize,
        _id: String,
        _name: String,
    },
    Thinking,
}

impl AnthropicStreamAdapter {
    fn new(inner: Pin<Box<dyn Stream<Item = Result<sse::SseEvent>> + Send>>) -> Self {
        Self {
            inner,
            usage: Usage::default(),
            current_blocks: HashMap::new(),
            tool_call_index: 0,
        }
    }

    fn process_event(&mut self, sse_event: sse::SseEvent) -> Vec<Result<StreamEvent>> {
        let event_type = sse_event.event.as_deref().unwrap_or("");
        let mut output = Vec::new();

        match event_type {
            "message_start" => {
                if let Ok(data) = parse_sse_json::<serde_json::Value>(&sse_event.data) {
                    self.usage = parse_anthropic_usage(&data["message"]["usage"]);
                }
            }
            "content_block_start" => {
                if let Ok(data) = parse_sse_json::<serde_json::Value>(&sse_event.data) {
                    let index = data["index"].as_u64().unwrap_or(0);
                    let block = &data["content_block"];
                    match block["type"].as_str() {
                        Some("text") => {
                            self.current_blocks.insert(index, ContentBlockState::Text);
                        }
                        Some("tool_use") => {
                            let tool_index = self.tool_call_index;
                            self.tool_call_index += 1;
                            let id = block["id"].as_str().unwrap_or_default().to_string();
                            let name = block["name"].as_str().unwrap_or_default().to_string();
                            self.current_blocks.insert(
                                index,
                                ContentBlockState::ToolUse {
                                    index: tool_index,
                                    _id: id.clone(),
                                    _name: name.clone(),
                                },
                            );
                            output.push(Ok(StreamEvent::ToolCallStart {
                                index: tool_index,
                                id,
                                name,
                            }));
                        }
                        Some("thinking") => {
                            self.current_blocks
                                .insert(index, ContentBlockState::Thinking);
                        }
                        _ => {}
                    }
                }
            }
            "content_block_delta" => {
                if let Ok(data) = parse_sse_json::<serde_json::Value>(&sse_event.data) {
                    let index = data["index"].as_u64().unwrap_or(0);
                    let delta = &data["delta"];

                    if let Some(block_state) = self.current_blocks.get(&index) {
                        match (delta["type"].as_str(), block_state) {
                            (Some("text_delta"), ContentBlockState::Text) => {
                                if let Some(text) = delta["text"].as_str() {
                                    output.push(Ok(StreamEvent::TextDelta(text.to_string())));
                                }
                            }
                            (
                                Some("input_json_delta"),
                                ContentBlockState::ToolUse {
                                    index: tool_idx, ..
                                },
                            ) => {
                                if let Some(json_delta) = delta["partial_json"].as_str() {
                                    output.push(Ok(StreamEvent::ToolCallDelta {
                                        index: *tool_idx,
                                        arguments_delta: json_delta.to_string(),
                                    }));
                                }
                            }
                            (Some("thinking_delta"), ContentBlockState::Thinking) => {
                                if let Some(thinking) = delta["thinking"].as_str() {
                                    output.push(Ok(StreamEvent::ReasoningDelta(
                                        thinking.to_string(),
                                    )));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "content_block_stop" => {
                if let Ok(data) = parse_sse_json::<serde_json::Value>(&sse_event.data) {
                    let index = data["index"].as_u64().unwrap_or(0);
                    if let Some(ContentBlockState::ToolUse {
                        index: tool_idx, ..
                    }) = self.current_blocks.get(&index)
                    {
                        output.push(Ok(StreamEvent::ToolCallEnd { index: *tool_idx }));
                    }
                    self.current_blocks.remove(&index);
                }
            }
            "message_delta" => {
                if let Ok(data) = parse_sse_json::<serde_json::Value>(&sse_event.data) {
                    // Merge usage from message_delta.
                    if let Some(output_tokens) = data["usage"]["output_tokens"].as_u64() {
                        self.usage.output_tokens = output_tokens;
                    }

                    let finish_reason = match data["delta"]["stop_reason"].as_str() {
                        Some("end_turn") | Some("stop") => FinishReason::Stop,
                        Some("tool_use") => FinishReason::ToolUse,
                        Some("max_tokens") => FinishReason::MaxTokens,
                        _ => FinishReason::Stop,
                    };

                    output.push(Ok(StreamEvent::StepFinish {
                        finish_reason,
                        usage: self.usage.clone(),
                    }));
                }
            }
            "message_stop" => {
                // No additional action needed; StepFinish was emitted at message_delta.
            }
            "error" => {
                if let Ok(data) = parse_sse_json::<serde_json::Value>(&sse_event.data) {
                    let msg = data["error"]["message"]
                        .as_str()
                        .or_else(|| data["message"].as_str())
                        .unwrap_or("Unknown streaming error");
                    output.push(Ok(StreamEvent::Error(msg.to_string())));
                }
            }
            "ping" => {
                // Keep-alive, ignore.
            }
            other => {
                debug!(event_type = other, "Unknown Anthropic SSE event type");
            }
        }

        output
    }
}

impl Stream for AnthropicStreamAdapter {
    type Item = Result<StreamEvent>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        use std::task::Poll;

        loop {
            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(sse_event))) => {
                    let events = self.process_event(sse_event);
                    if let Some(first) = events.into_iter().next() {
                        return Poll::Ready(Some(first));
                    }
                    // No events produced, poll again.
                    continue;
                }
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Some(Err(e)));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(None);
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::{ChatMessage, ToolDefinition};

    #[test]
    fn test_build_request_body_basic() {
        let provider = AnthropicProvider::new("test-key");
        let request = ChatRequest::new(
            "claude-3-5-sonnet-20241022",
            vec![
                ChatMessage::text(Role::System, "You are helpful."),
                ChatMessage::text(Role::User, "Hello"),
            ],
        );

        let body = provider.build_request_body(&request, true).unwrap();

        assert_eq!(body["model"], "claude-3-5-sonnet-20241022");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 8192);
        assert!(body["system"].is_array());
        assert_eq!(body["system"][0]["text"], "You are helpful.");
        assert_eq!(body["messages"].as_array().unwrap().len(), 1);
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = AnthropicProvider::new("test-key");
        let mut request = ChatRequest::new(
            "claude-3-5-sonnet-20241022",
            vec![ChatMessage::text(Role::User, "Read file.txt")],
        );
        request.tools = vec![ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                },
                "required": ["path"]
            }),
        }];

        let body = provider.build_request_body(&request, false).unwrap();

        assert!(body["tools"].is_array());
        assert_eq!(body["tools"][0]["name"], "read_file");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn test_parse_response() {
        let provider = AnthropicProvider::new("test-key");
        let body = serde_json::json!({
            "content": [
                {"type": "text", "text": "Hello world"},
            ],
            "stop_reason": "end_turn",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "cache_read_input_tokens": 2,
                "cache_creation_input_tokens": 1,
            }
        });

        let response = provider.parse_response(body).unwrap();
        assert_eq!(response.content, "Hello world");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
        assert_eq!(response.usage.cache_read_tokens, 2);
        assert_eq!(response.usage.cache_creation_tokens, 1);
    }

    #[test]
    fn test_parse_response_with_tool_use() {
        let provider = AnthropicProvider::new("test-key");
        let body = serde_json::json!({
            "content": [
                {"type": "text", "text": "Let me read that file."},
                {
                    "type": "tool_use",
                    "id": "toolu_123",
                    "name": "read_file",
                    "input": {"path": "file.txt"}
                }
            ],
            "stop_reason": "tool_use",
            "usage": {"input_tokens": 10, "output_tokens": 20}
        });

        let response = provider.parse_response(body).unwrap();
        assert_eq!(response.content, "Let me read that file.");
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "toolu_123");
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(response.tool_calls[0].arguments["path"], "file.txt");
        assert_eq!(response.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn test_stream_adapter_text_delta() {
        let mut adapter = AnthropicStreamAdapter::new(Box::pin(futures::stream::empty()));
        // Simulate content_block_start for text.
        adapter.process_event(sse::SseEvent {
            event: Some("content_block_start".into()),
            data: r#"{"index":0,"content_block":{"type":"text","text":""}}"#.into(),
        });

        let events = adapter.process_event(sse::SseEvent {
            event: Some("content_block_delta".into()),
            data: r#"{"index":0,"delta":{"type":"text_delta","text":"Hello"}}"#.into(),
        });

        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::TextDelta(text)) => assert_eq!(text, "Hello"),
            other => panic!("Expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_stream_adapter_tool_use() {
        let mut adapter = AnthropicStreamAdapter::new(Box::pin(futures::stream::empty()));

        // Tool use block start.
        let events = adapter.process_event(sse::SseEvent {
            event: Some("content_block_start".into()),
            data: r#"{"index":1,"content_block":{"type":"tool_use","id":"toolu_abc","name":"read_file"}}"#.into(),
        });
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ToolCallStart { index, id, name }) => {
                assert_eq!(*index, 0);
                assert_eq!(id, "toolu_abc");
                assert_eq!(name, "read_file");
            }
            other => panic!("Expected ToolCallStart, got {:?}", other),
        }

        // Tool use delta.
        let events = adapter.process_event(sse::SseEvent {
            event: Some("content_block_delta".into()),
            data: r#"{"index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}"#
                .into(),
        });
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ToolCallDelta {
                index,
                arguments_delta,
            }) => {
                assert_eq!(*index, 0);
                assert_eq!(arguments_delta, "{\"path\":");
            }
            other => panic!("Expected ToolCallDelta, got {:?}", other),
        }

        // Tool use end.
        let events = adapter.process_event(sse::SseEvent {
            event: Some("content_block_stop".into()),
            data: r#"{"index":1}"#.into(),
        });
        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::ToolCallEnd { index }) => assert_eq!(*index, 0),
            other => panic!("Expected ToolCallEnd, got {:?}", other),
        }
    }

    #[test]
    fn test_stream_adapter_message_delta() {
        let mut adapter = AnthropicStreamAdapter::new(Box::pin(futures::stream::empty()));
        adapter.usage = Usage {
            input_tokens: 25,
            output_tokens: 0,
            ..Default::default()
        };

        let events = adapter.process_event(sse::SseEvent {
            event: Some("message_delta".into()),
            data: r#"{"delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#.into(),
        });

        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::StepFinish {
                finish_reason,
                usage,
            }) => {
                assert_eq!(*finish_reason, FinishReason::Stop);
                assert_eq!(usage.input_tokens, 25);
                assert_eq!(usage.output_tokens, 42);
            }
            other => panic!("Expected StepFinish, got {:?}", other),
        }
    }

    #[test]
    fn test_headers() {
        let provider = AnthropicProvider::new("sk-test-key");
        let request = ChatRequest::new("claude-3-5-sonnet-20241022", vec![]);

        let headers = provider.build_headers(&request);
        assert_eq!(headers.get("x-api-key").unwrap(), "sk-test-key");
        assert_eq!(headers.get("anthropic-version").unwrap(), API_VERSION);
        assert!(headers.get("anthropic-beta").is_none());
    }

    #[test]
    fn test_headers_with_thinking() {
        let provider = AnthropicProvider::new("sk-test-key");
        let mut request = ChatRequest::new("claude-3-5-sonnet-20241022", vec![]);
        request.provider_options.insert(
            "thinking".into(),
            serde_json::json!({"type": "enabled", "budget_tokens": 10000}),
        );

        let headers = provider.build_headers(&request);
        assert!(headers.get("anthropic-beta").is_some());
    }
}
