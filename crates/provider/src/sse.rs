//! Server-Sent Events (SSE) parser for streaming LLM responses.
//!
//! Parses the `data: {...}\n\n` format used by OpenAI and Anthropic APIs.

use anyhow::{Context, Result};
use futures::Stream;
use reqwest::Response;
use std::pin::Pin;
use std::task::{Poll, ready};

/// A parsed SSE event.
#[derive(Debug, Clone)]
pub struct SseEvent {
    /// The event type (from `event:` line), if present.
    pub event: Option<String>,
    /// The JSON data payload (from `data:` line).
    pub data: String,
}

/// Parse a line-based byte stream into SSE events.
///
/// Handles:
/// - `event: <type>` lines
/// - `data: <json>` lines
/// - `data: [DONE]` sentinel (terminates the stream)
/// - Empty lines as event delimiters
/// - Comment lines starting with `:`
pub fn parse_sse(response: Response) -> Pin<Box<dyn Stream<Item = Result<SseEvent>> + Send>> {
    Box::pin(SseStream::new(response))
}

struct SseStream {
    bytes_stream: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    buffer: String,
    current_event: Option<String>,
    current_data: Vec<String>,
    done: bool,
}

impl SseStream {
    fn new(response: Response) -> Self {
        let bytes_stream = Box::pin(response.bytes_stream());
        Self {
            bytes_stream,
            buffer: String::new(),
            current_event: None,
            current_data: Vec::new(),
            done: false,
        }
    }
}

impl Stream for SseStream {
    type Item = Result<SseEvent>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        if this.done {
            return Poll::Ready(None);
        }

        loop {
            // Try to extract a complete event from the buffer.
            if let Some(event) = try_extract_event(
                &mut this.buffer,
                &mut this.current_event,
                &mut this.current_data,
                &mut this.done,
            ) {
                return Poll::Ready(Some(event));
            }

            if this.done {
                return Poll::Ready(None);
            }

            // Need more data from the underlying stream.
            match ready!(this.bytes_stream.as_mut().poll_next(cx)) {
                Some(Ok(chunk)) => {
                    let text = String::from_utf8_lossy(&chunk);
                    this.buffer.push_str(&text);
                }
                Some(Err(e)) => {
                    return Poll::Ready(Some(Err(e).context("SSE stream read error")));
                }
                None => {
                    // Stream ended. Flush any pending event.
                    if !this.current_data.is_empty() {
                        let data = this.current_data.join("\n");
                        let event = this.current_event.take();
                        this.current_data.clear();
                        return Poll::Ready(Some(Ok(SseEvent { event, data })));
                    }
                    this.done = true;
                    return Poll::Ready(None);
                }
            }
        }
    }
}

/// Try to parse complete lines from the buffer and produce an event.
/// Returns `Some` if an event is ready, `None` if more data is needed.
fn try_extract_event(
    buffer: &mut String,
    current_event: &mut Option<String>,
    current_data: &mut Vec<String>,
    done: &mut bool,
) -> Option<Result<SseEvent>> {
    loop {
        // Find the next newline.
        let newline_pos = buffer.find('\n')?;
        let line = buffer[..newline_pos].trim_end_matches('\r').to_string();
        buffer.drain(..=newline_pos);

        if line.is_empty() {
            // Empty line = event delimiter.
            if !current_data.is_empty() {
                let data = current_data.join("\n");
                let event = current_event.take();
                current_data.clear();
                return Some(Ok(SseEvent { event, data }));
            }
            continue;
        }

        if line.starts_with(':') {
            // Comment line, skip.
            continue;
        }

        if let Some(event_type) = line.strip_prefix("event:") {
            *current_event = Some(event_type.trim().to_string());
        } else if let Some(data) = line.strip_prefix("data:") {
            let data = data.trim_start();
            if data == "[DONE]" {
                *done = true;
                current_data.clear();
                *current_event = None;
                return None;
            }
            current_data.push(data.to_string());
        }
        // Ignore other field types (id:, retry:, etc.)
    }
}

/// Parse a single SSE data line as JSON.
pub fn parse_sse_json<T: serde::de::DeserializeOwned>(data: &str) -> Result<T> {
    serde_json::from_str(data).with_context(|| format!("Failed to parse SSE JSON: {}", truncate(data, 200)))
}

fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    /// Helper: create a stream of SseEvents from raw SSE text.
    async fn parse_raw_sse(raw: &str) -> Vec<SseEvent> {
        let mut buffer = String::new();
        let mut current_event: Option<String> = None;
        let mut current_data: Vec<String> = Vec::new();
        let mut events = Vec::new();

        for line in raw.split('\n') {
            let line = line.trim_end_matches('\r');

            if line.is_empty() {
                if !current_data.is_empty() {
                    events.push(SseEvent {
                        event: current_event.take(),
                        data: current_data.join("\n"),
                    });
                    current_data.clear();
                }
                continue;
            }

            if line.starts_with(':') {
                continue;
            }

            if let Some(event_type) = line.strip_prefix("event:") {
                current_event = Some(event_type.trim().to_string());
            } else if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim_start();
                if data == "[DONE]" {
                    break;
                }
                current_data.push(data.to_string());
            }
        }

        // Flush remaining.
        if !current_data.is_empty() {
            events.push(SseEvent {
                event: current_event.take(),
                data: current_data.join("\n"),
            });
        }

        events
    }

    #[tokio::test]
    async fn test_parse_simple_data_events() {
        let raw = "data: {\"text\": \"hello\"}\n\ndata: {\"text\": \"world\"}\n\n";
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].data, "{\"text\": \"hello\"}");
        assert_eq!(events[1].data, "{\"text\": \"world\"}");
        assert!(events[0].event.is_none());
    }

    #[tokio::test]
    async fn test_parse_events_with_event_type() {
        let raw = "event: message_start\ndata: {\"type\": \"message_start\"}\n\nevent: content_block_delta\ndata: {\"type\": \"content_block_delta\"}\n\n";
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event.as_deref(), Some("message_start"));
        assert_eq!(events[1].event.as_deref(), Some("content_block_delta"));
    }

    #[tokio::test]
    async fn test_done_sentinel_stops_parsing() {
        let raw = "data: {\"text\": \"hello\"}\n\ndata: [DONE]\n\ndata: {\"text\": \"should not appear\"}\n\n";
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"text\": \"hello\"}");
    }

    #[tokio::test]
    async fn test_comment_lines_are_skipped() {
        let raw = ": this is a comment\ndata: {\"text\": \"hello\"}\n\n";
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "{\"text\": \"hello\"}");
    }

    #[tokio::test]
    async fn test_multiline_data() {
        let raw = "data: line1\ndata: line2\n\n";
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "line1\nline2");
    }

    #[tokio::test]
    async fn test_empty_lines_between_events() {
        let raw = "\n\ndata: {\"a\": 1}\n\n\n\ndata: {\"b\": 2}\n\n";
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 2);
    }

    #[tokio::test]
    async fn test_parse_sse_json() {
        #[derive(Deserialize, Debug)]
        struct Msg {
            text: String,
        }
        let msg: Msg = parse_sse_json("{\"text\": \"hello\"}").unwrap();
        assert_eq!(msg.text, "hello");
    }

    #[tokio::test]
    async fn test_parse_sse_json_error() {
        let result = parse_sse_json::<serde_json::Value>("not json");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_anthropic_sse_sequence() {
        let raw = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_01","type":"message","role":"assistant","content":[],"model":"claude-3-5-sonnet-20241022","usage":{"input_tokens":25,"output_tokens":1}}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}

event: content_block_delta
data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" world"}}

event: content_block_stop
data: {"type":"content_block_stop","index":0}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":12}}

event: message_stop
data: {"type":"message_stop"}

"#;
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 7);
        assert_eq!(events[0].event.as_deref(), Some("message_start"));
        assert_eq!(events[1].event.as_deref(), Some("content_block_start"));
        assert_eq!(events[2].event.as_deref(), Some("content_block_delta"));
        assert_eq!(events[5].event.as_deref(), Some("message_delta"));
        assert_eq!(events[6].event.as_deref(), Some("message_stop"));
    }

    #[tokio::test]
    async fn test_openai_sse_sequence() {
        let raw = r#"data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant","content":""},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":" world"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}

data: [DONE]

"#;
        let events = parse_raw_sse(raw).await;
        assert_eq!(events.len(), 4);

        // Verify we can parse the JSON payloads.
        let first: serde_json::Value = parse_sse_json(&events[0].data).unwrap();
        assert_eq!(first["choices"][0]["delta"]["role"], "assistant");

        let last: serde_json::Value = parse_sse_json(&events[3].data).unwrap();
        assert_eq!(last["choices"][0]["finish_reason"], "stop");
    }

    #[tokio::test]
    async fn test_try_extract_event_incremental() {
        // Simulate incremental data arrival.
        let mut buffer = String::new();
        let mut current_event: Option<String> = None;
        let mut current_data: Vec<String> = Vec::new();
        let mut done = false;

        // First chunk: incomplete line.
        buffer.push_str("data: {\"te");
        let result = try_extract_event(&mut buffer, &mut current_event, &mut current_data, &mut done);
        assert!(result.is_none());
        assert_eq!(buffer, "data: {\"te");

        // Second chunk: completes the line and adds empty delimiter.
        buffer.push_str("xt\": \"hi\"}\n\n");
        let result = try_extract_event(&mut buffer, &mut current_event, &mut current_data, &mut done);
        assert!(result.is_some());
        let event = result.unwrap().unwrap();
        assert_eq!(event.data, "{\"text\": \"hi\"}");
    }

    #[tokio::test]
    async fn test_try_extract_event_done_sentinel() {
        let mut buffer = "data: [DONE]\n\n".to_string();
        let mut current_event: Option<String> = None;
        let mut current_data: Vec<String> = Vec::new();
        let mut done = false;

        let result = try_extract_event(&mut buffer, &mut current_event, &mut current_data, &mut done);
        assert!(result.is_none());
        assert!(done);
    }
}
