//! Provider error parsing, including context overflow detection.
//!
//! Mirrors `src/provider/error.ts` from the original OpenCode.
//! Contains 23 regex patterns for detecting context overflow across providers.

use once_cell::sync::Lazy;
use regex::Regex;

/// Parsed API error types.
#[derive(Debug, Clone)]
pub enum ParsedError {
    ContextOverflow { message: String },
    ApiError { message: String, status: Option<u16>, retryable: bool },
}

/// Context overflow detection patterns (from all providers).
static OVERFLOW_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    [
        r"maximum context length",
        r"context_length_exceeded",
        r"max_tokens",
        r"too many tokens",
        r"context window",
        r"token limit",
        r"exceeds? the model'?s? (maximum )?context",
        r"Input is too long",
        r"input too long",
        r"prompt is too long",
        r"request too large",
        r"content_too_large",
        r"prompt_too_long",
        r"RESOURCE_EXHAUSTED",
        r"GenerateContentRequest.contents: contents too long",
        r"Please reduce (?:the length of the messages|your prompt)",
        r"Please reduce the length of the messages",
        r"Your input has exceeded the allowed token limit",
        r"input_tokens_limit",
        r"Input token count.*exceeds",
        r"too long input",
        r"maximum number of tokens",
        r"string too long",
    ]
    .iter()
    .filter_map(|p| Regex::new(&format!("(?i){p}")).ok())
    .collect()
});

/// Check if an error message indicates context overflow.
pub fn is_context_overflow(message: &str) -> bool {
    OVERFLOW_PATTERNS.iter().any(|re| re.is_match(message))
}

/// Parse an API error response into a structured error.
pub fn parse_api_error(status: Option<u16>, message: &str) -> ParsedError {
    if is_context_overflow(message) {
        return ParsedError::ContextOverflow {
            message: message.to_string(),
        };
    }

    let retryable = matches!(status, Some(429 | 500 | 502 | 503 | 529));

    ParsedError::ApiError {
        message: message.to_string(),
        status,
        retryable,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_overflow_patterns() {
        assert!(is_context_overflow("maximum context length exceeded"));
        assert!(is_context_overflow("This model's context window is 128k"));
        assert!(is_context_overflow("prompt is too long for this model"));
        assert!(is_context_overflow("RESOURCE_EXHAUSTED: quota limit"));
        assert!(!is_context_overflow("normal error message"));
    }

    #[test]
    fn retryable_errors() {
        match parse_api_error(Some(429), "rate limited") {
            ParsedError::ApiError { retryable, .. } => assert!(retryable),
            _ => panic!("expected api error"),
        }
        match parse_api_error(Some(400), "bad request") {
            ParsedError::ApiError { retryable, .. } => assert!(!retryable),
            _ => panic!("expected api error"),
        }
    }
}
