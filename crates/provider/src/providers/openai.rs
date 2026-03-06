//! OpenAI Chat Completions API provider.
//!
//! Implements the LlmProvider trait for the OpenAI Chat Completions API.
//! Supports streaming via SSE with delta events for content and tool calls.

use std::collections::HashMap;
use std::pin::Pin;

use anyhow::{Context, Result, bail};
use async_trait::async_trait;
use futures::Stream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

use crate::provider::{
    ChatMessage, ChatRequest, ChatResponse, ContentPart, FinishReason, LlmProvider, Role,
    StreamEvent, ToolCall, Usage,
};
use crate::sse::{self, parse_sse_json};

const DEFAULT_API_URL: &str = "https://api.openai.com";

/// OpenAI provider configuration.
#[derive(Debug, Clone)]
pub struct OpenAiProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    /// Provider ID override (for OpenAI-compatible providers like Groq, Together, etc.)
    provider_id: String,
    /// Provider name override.
    provider_name: String,
}

impl OpenAiProvider {
    /// Create a new OpenAI provider with the given API key.
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: DEFAULT_API_URL.to_string(),
            client: reqwest::Client::new(),
            provider_id: "openai".to_string(),
            provider_name: "OpenAI".to_string(),
        }
    }

    /// Create an OpenAI-compatible provider with custom id, name, and base URL.
    pub fn new_compatible(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        provider_id: impl Into<String>,
        provider_name: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            base_url: base_url.into(),
            client: reqwest::Client::new(),
            provider_id: provider_id.into(),
            provider_name: provider_name.into(),
        }
    }

    /// Set a custom base URL.
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Build the request body for the Chat Completions API.
    fn build_request_body(&self, request: &ChatRequest, stream: bool) -> Result<serde_json::Value> {
        let mut body = serde_json::Map::new();

        body.insert(
            "model".into(),
            serde_json::Value::String(request.model.clone()),
        );

        // Convert messages.
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|msg| self.convert_message(msg))
            .collect();
        body.insert("messages".into(), serde_json::Value::Array(messages));

        // Temperature.
        if let Some(temp) = request.temperature
            && let Some(n) = serde_json::Number::from_f64(temp)
        {
            body.insert("temperature".into(), serde_json::Value::Number(n));
        }

        // Top-p.
        if let Some(top_p) = request.top_p
            && let Some(n) = serde_json::Number::from_f64(top_p)
        {
            body.insert("top_p".into(), serde_json::Value::Number(n));
        }

        // Max tokens - OpenAI uses either max_tokens or max_completion_tokens.
        if let Some(max_tokens) = request.max_tokens {
            // Newer models use max_completion_tokens.
            let key = if request
                .provider_options
                .contains_key("use_max_completion_tokens")
            {
                "max_completion_tokens"
            } else {
                "max_tokens"
            };
            body.insert(key.into(), serde_json::Value::Number(max_tokens.into()));
        }

        // Tools.
        if !request.tools.is_empty() {
            let tools: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body.insert("tools".into(), serde_json::Value::Array(tools));
        }

        // Streaming.
        if stream {
            body.insert("stream".into(), serde_json::Value::Bool(true));
            body.insert(
                "stream_options".into(),
                serde_json::json!({"include_usage": true}),
            );
        }

        // Reasoning effort (for o-series models).
        if let Some(reasoning) = request.provider_options.get("reasoning_effort") {
            body.insert("reasoning_effort".into(), reasoning.clone());
        }

        Ok(serde_json::Value::Object(body))
    }

    /// Convert a ChatMessage to OpenAI's message format.
    fn convert_message(&self, msg: &ChatMessage) -> serde_json::Value {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        let mut m = serde_json::Map::new();
        m.insert("role".into(), role.into());

        match msg.role {
            Role::Tool => {
                // Tool results need tool_call_id.
                for part in &msg.content {
                    if let ContentPart::ToolResult {
                        tool_use_id,
                        content,
                        ..
                    } = part
                    {
                        m.insert("tool_call_id".into(), tool_use_id.clone().into());
                        m.insert("content".into(), content.clone().into());
                    }
                }
            }
            Role::Assistant => {
                // Assistant messages may have text content and/or tool_calls.
                let mut text_parts = Vec::new();
                let mut tool_calls = Vec::new();

                for part in &msg.content {
                    match part {
                        ContentPart::Text { text } => {
                            text_parts.push(text.clone());
                        }
                        ContentPart::ToolUse { id, name, input } => {
                            tool_calls.push(serde_json::json!({
                                "id": id,
                                "type": "function",
                                "function": {
                                    "name": name,
                                    "arguments": input.to_string(),
                                }
                            }));
                        }
                        _ => {}
                    }
                }

                if !text_parts.is_empty() {
                    m.insert("content".into(), text_parts.join("").into());
                }
                if !tool_calls.is_empty() {
                    m.insert("tool_calls".into(), serde_json::Value::Array(tool_calls));
                }
            }
            Role::User | Role::System => {
                // Check if we need multimodal content.
                let has_non_text = msg
                    .content
                    .iter()
                    .any(|p| matches!(p, ContentPart::Image { .. }));

                if has_non_text {
                    let parts: Vec<serde_json::Value> = msg
                        .content
                        .iter()
                        .filter_map(|part| match part {
                            ContentPart::Text { text } => {
                                Some(serde_json::json!({"type": "text", "text": text}))
                            }
                            ContentPart::Image { data, media_type } => Some(serde_json::json!({
                                "type": "image_url",
                                "image_url": {
                                    "url": format!("data:{};base64,{}", media_type, data),
                                }
                            })),
                            _ => None,
                        })
                        .collect();
                    m.insert("content".into(), serde_json::Value::Array(parts));
                } else {
                    let text: String = msg
                        .content
                        .iter()
                        .filter_map(|p| {
                            if let ContentPart::Text { text } = p {
                                Some(text.as_str())
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("");
                    m.insert("content".into(), text.into());
                }
            }
        }

        serde_json::Value::Object(m)
    }

    /// Build HTTP headers for the OpenAI API.
    fn build_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.api_key).parse().unwrap(),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );
        headers
    }

    /// Parse a non-streaming OpenAI response.
    fn parse_response(&self, body: serde_json::Value) -> Result<ChatResponse> {
        let choice = body["choices"]
            .as_array()
            .and_then(|c| c.first())
            .context("No choices in OpenAI response")?;

        let message = &choice["message"];
        let content = message["content"].as_str().unwrap_or_default().to_string();

        let mut tool_calls = Vec::new();
        if let Some(tcs) = message["tool_calls"].as_array() {
            for tc in tcs {
                let func = &tc["function"];
                let arguments_str = func["arguments"].as_str().unwrap_or("{}");
                let arguments: serde_json::Value =
                    serde_json::from_str(arguments_str).unwrap_or(serde_json::json!({}));

                tool_calls.push(ToolCall {
                    id: tc["id"].as_str().unwrap_or_default().to_string(),
                    name: func["name"].as_str().unwrap_or_default().to_string(),
                    arguments,
                });
            }
        }

        let usage = parse_openai_usage(&body["usage"]);
        let finish_reason =
            parse_openai_finish_reason(choice["finish_reason"].as_str().unwrap_or("stop"));

        Ok(ChatResponse {
            content,
            tool_calls,
            usage,
            finish_reason,
        })
    }
}

fn parse_openai_usage(usage: &serde_json::Value) -> Usage {
    Usage {
        input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
        output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
        cache_read_tokens: usage["prompt_tokens_details"]["cached_tokens"]
            .as_u64()
            .unwrap_or(0),
        cache_creation_tokens: 0,
    }
}

fn parse_openai_finish_reason(reason: &str) -> FinishReason {
    match reason {
        "stop" => FinishReason::Stop,
        "tool_calls" => FinishReason::ToolUse,
        "length" => FinishReason::MaxTokens,
        "content_filter" => FinishReason::ContentFilter,
        _ => FinishReason::Stop,
    }
}

#[async_trait]
impl LlmProvider for OpenAiProvider {
    fn id(&self) -> &str {
        &self.provider_id
    }

    fn name(&self) -> &str {
        &self.provider_name
    }

    async fn chat(&self, request: ChatRequest, cancel: CancellationToken) -> Result<ChatResponse> {
        let body = self.build_request_body(&request, false)?;
        let headers = self.build_headers();
        let url = format!("{}/v1/chat/completions", self.base_url);

        debug!(url = %url, model = %request.model, "Sending OpenAI chat request");

        let response = tokio::select! {
            result = self.client.post(&url).headers(headers).json(&body).send() => {
                result.context("Failed to send OpenAI request")?
            }
            _ = cancel.cancelled() => {
                bail!("Request cancelled");
            }
        };

        let status = response.status();
        let response_body: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse OpenAI response body")?;

        if !status.is_success() {
            let error_msg = response_body["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            bail!("OpenAI API error ({}): {}", status, error_msg);
        }

        self.parse_response(response_body)
    }

    async fn stream(
        &self,
        request: ChatRequest,
        cancel: CancellationToken,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent>> + Send>>> {
        let body = self.build_request_body(&request, true)?;
        let headers = self.build_headers();
        let url = format!("{}/v1/chat/completions", self.base_url);

        debug!(url = %url, model = %request.model, "Sending OpenAI streaming request");

        let response = tokio::select! {
            result = self.client.post(&url).headers(headers).json(&body).send() => {
                result.context("Failed to send OpenAI streaming request")?
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
            bail!("OpenAI API error ({}): {}", status, error_msg);
        }

        let sse_stream = sse::parse_sse(response);
        let event_stream = OpenAiStreamAdapter::new(sse_stream);

        Ok(Box::pin(event_stream))
    }
}

/// Adapts OpenAI SSE events into our unified StreamEvent format.
struct OpenAiStreamAdapter {
    inner: Pin<Box<dyn Stream<Item = Result<sse::SseEvent>> + Send>>,
    /// Track active tool calls by index for ToolCallStart vs ToolCallDelta.
    active_tool_calls: HashMap<usize, bool>,
}

impl OpenAiStreamAdapter {
    fn new(inner: Pin<Box<dyn Stream<Item = Result<sse::SseEvent>> + Send>>) -> Self {
        Self {
            inner,
            active_tool_calls: HashMap::new(),
        }
    }

    fn process_event(&mut self, sse_event: sse::SseEvent) -> Vec<Result<StreamEvent>> {
        let mut output = Vec::new();

        let data: serde_json::Value = match parse_sse_json(&sse_event.data) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "Failed to parse OpenAI SSE event");
                return output;
            }
        };

        // Check for usage-only chunk (sent at end with stream_options.include_usage).
        if let Some(usage) = data.get("usage")
            && !usage.is_null()
        {
            let parsed_usage = parse_openai_usage(usage);

            // Determine finish reason from the last choice, if present.
            let finish_reason = data["choices"]
                .as_array()
                .and_then(|c| c.first())
                .and_then(|c| c["finish_reason"].as_str())
                .map(parse_openai_finish_reason)
                .unwrap_or(FinishReason::Stop);

            output.push(Ok(StreamEvent::StepFinish {
                finish_reason,
                usage: parsed_usage,
            }));
        }

        // Process choices.
        if let Some(choices) = data["choices"].as_array() {
            for choice in choices {
                let delta = &choice["delta"];

                // Text content delta.
                if let Some(content) = delta["content"].as_str()
                    && !content.is_empty()
                {
                    output.push(Ok(StreamEvent::TextDelta(content.to_string())));
                }

                // Reasoning content (for o-series models).
                if let Some(reasoning) = delta.get("reasoning_content").and_then(|r| r.as_str())
                    && !reasoning.is_empty()
                {
                    output.push(Ok(StreamEvent::ReasoningDelta(reasoning.to_string())));
                }

                // Tool calls.
                if let Some(tool_calls) = delta["tool_calls"].as_array() {
                    for tc in tool_calls {
                        let index = tc["index"].as_u64().unwrap_or(0) as usize;
                        let func = &tc["function"];

                        if let std::collections::hash_map::Entry::Vacant(e) =
                            self.active_tool_calls.entry(index)
                        {
                            // New tool call.
                            e.insert(true);
                            let id = tc["id"].as_str().unwrap_or_default().to_string();
                            let name = func["name"].as_str().unwrap_or_default().to_string();
                            output.push(Ok(StreamEvent::ToolCallStart { index, id, name }));
                        }

                        // Arguments delta.
                        if let Some(args) = func["arguments"].as_str()
                            && !args.is_empty()
                        {
                            output.push(Ok(StreamEvent::ToolCallDelta {
                                index,
                                arguments_delta: args.to_string(),
                            }));
                        }
                    }
                }

                // Finish reason on the choice itself (not the usage chunk).
                if let Some(reason) = choice["finish_reason"].as_str() {
                    // Emit ToolCallEnd for any active tool calls when finishing.
                    if reason == "tool_calls" || reason == "stop" {
                        let mut indices: Vec<usize> =
                            self.active_tool_calls.keys().copied().collect();
                        indices.sort();
                        for idx in indices {
                            output.push(Ok(StreamEvent::ToolCallEnd { index: idx }));
                        }
                        self.active_tool_calls.clear();
                    }
                }
            }
        }

        output
    }
}

impl Stream for OpenAiStreamAdapter {
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
    use crate::provider::ToolDefinition;

    #[test]
    fn test_build_request_body_basic() {
        let provider = OpenAiProvider::new("test-key");
        let request = ChatRequest::new(
            "gpt-4o",
            vec![
                ChatMessage::text(Role::System, "You are helpful."),
                ChatMessage::text(Role::User, "Hello"),
            ],
        );

        let body = provider.build_request_body(&request, true).unwrap();

        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "You are helpful.");
        assert_eq!(body["messages"][1]["role"], "user");
    }

    #[test]
    fn test_build_request_body_with_tools() {
        let provider = OpenAiProvider::new("test-key");
        let mut request = ChatRequest::new(
            "gpt-4o",
            vec![ChatMessage::text(Role::User, "Read file.txt")],
        );
        request.tools = vec![ToolDefinition {
            name: "read_file".into(),
            description: "Read a file".into(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }),
        }];

        let body = provider.build_request_body(&request, false).unwrap();

        assert!(body["tools"].is_array());
        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
    }

    #[test]
    fn test_convert_tool_message() {
        let provider = OpenAiProvider::new("test-key");
        let msg = ChatMessage::tool_result("call_123", "file contents here", false);
        let converted = provider.convert_message(&msg);

        assert_eq!(converted["role"], "tool");
        assert_eq!(converted["tool_call_id"], "call_123");
        assert_eq!(converted["content"], "file contents here");
    }

    #[test]
    fn test_convert_assistant_message_with_tool_calls() {
        let provider = OpenAiProvider::new("test-key");
        let msg = ChatMessage {
            role: Role::Assistant,
            content: vec![
                ContentPart::Text {
                    text: "Let me read that.".into(),
                },
                ContentPart::ToolUse {
                    id: "call_abc".into(),
                    name: "read_file".into(),
                    input: serde_json::json!({"path": "test.txt"}),
                },
            ],
        };

        let converted = provider.convert_message(&msg);
        assert_eq!(converted["role"], "assistant");
        assert_eq!(converted["content"], "Let me read that.");
        assert_eq!(converted["tool_calls"][0]["id"], "call_abc");
        assert_eq!(converted["tool_calls"][0]["function"]["name"], "read_file");
    }

    #[test]
    fn test_parse_response() {
        let provider = OpenAiProvider::new("test-key");
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hello world"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        });

        let response = provider.parse_response(body).unwrap();
        assert_eq!(response.content, "Hello world");
        assert!(response.tool_calls.is_empty());
        assert_eq!(response.finish_reason, FinishReason::Stop);
        assert_eq!(response.usage.input_tokens, 10);
        assert_eq!(response.usage.output_tokens, 5);
    }

    #[test]
    fn test_parse_response_with_tool_calls() {
        let provider = OpenAiProvider::new("test-key");
        let body = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_xyz",
                        "type": "function",
                        "function": {
                            "name": "read_file",
                            "arguments": "{\"path\":\"test.txt\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 20, "total_tokens": 30}
        });

        let response = provider.parse_response(body).unwrap();
        assert_eq!(response.tool_calls.len(), 1);
        assert_eq!(response.tool_calls[0].id, "call_xyz");
        assert_eq!(response.tool_calls[0].name, "read_file");
        assert_eq!(response.tool_calls[0].arguments["path"], "test.txt");
        assert_eq!(response.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn test_stream_adapter_text_delta() {
        let mut adapter = OpenAiStreamAdapter::new(Box::pin(futures::stream::empty()));

        let events = adapter.process_event(sse::SseEvent {
            event: None,
            data: r#"{"choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}"#
                .into(),
        });

        assert_eq!(events.len(), 1);
        match &events[0] {
            Ok(StreamEvent::TextDelta(text)) => assert_eq!(text, "Hello"),
            other => panic!("Expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_stream_adapter_tool_call() {
        let mut adapter = OpenAiStreamAdapter::new(Box::pin(futures::stream::empty()));

        // Tool call start.
        let events = adapter.process_event(sse::SseEvent {
            event: None,
            data: r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"read_file","arguments":""}}]},"finish_reason":null}]}"#.into(),
        });
        assert!(!events.is_empty());
        match &events[0] {
            Ok(StreamEvent::ToolCallStart { index, id, name }) => {
                assert_eq!(*index, 0);
                assert_eq!(id, "call_1");
                assert_eq!(name, "read_file");
            }
            other => panic!("Expected ToolCallStart, got {:?}", other),
        }

        // Tool call delta.
        let events = adapter.process_event(sse::SseEvent {
            event: None,
            data: r#"{"choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"{\"path\":"}}]},"finish_reason":null}]}"#.into(),
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
    }

    #[test]
    fn test_stream_adapter_usage_chunk() {
        let mut adapter = OpenAiStreamAdapter::new(Box::pin(futures::stream::empty()));

        let events = adapter.process_event(sse::SseEvent {
            event: None,
            data: r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":25,"completion_tokens":10,"total_tokens":35}}"#.into(),
        });

        // Should produce a StepFinish event.
        let step_finish = events
            .iter()
            .find(|e| matches!(e, Ok(StreamEvent::StepFinish { .. })));
        assert!(step_finish.is_some());
        if let Ok(StreamEvent::StepFinish {
            usage,
            finish_reason,
        }) = step_finish.unwrap()
        {
            assert_eq!(usage.input_tokens, 25);
            assert_eq!(usage.output_tokens, 10);
            assert_eq!(*finish_reason, FinishReason::Stop);
        }
    }

    #[test]
    fn test_headers() {
        let provider = OpenAiProvider::new("sk-test-key");
        let headers = provider.build_headers();

        assert_eq!(
            headers.get(reqwest::header::AUTHORIZATION).unwrap(),
            "Bearer sk-test-key"
        );
    }

    #[test]
    fn test_openai_compatible_provider() {
        let provider =
            OpenAiProvider::new_compatible("key-123", "https://api.groq.com", "groq", "Groq");

        assert_eq!(provider.id(), "groq");
        assert_eq!(provider.name(), "Groq");
        assert_eq!(provider.base_url, "https://api.groq.com");
    }
}
