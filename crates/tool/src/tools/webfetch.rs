//! Web fetch tool: retrieve content from URLs.

use anyhow::Result;

use crate::tool::{Tool, ToolContext, ToolOutput};
use crate::truncation;

const MAX_RESPONSE_BYTES: usize = 5 * 1024 * 1024; // 5MB
const DEFAULT_TIMEOUT_SECS: u64 = 30;

pub struct WebFetchTool;

#[async_trait::async_trait]
impl Tool for WebFetchTool {
    fn id(&self) -> &str {
        "webfetch"
    }

    fn description(&self) -> &str {
        include_str!("../../descriptions/webfetch.txt")
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {
                    "type": "string",
                    "description": "The URL to fetch (must start with http:// or https://)"
                },
                "format": {
                    "type": "string",
                    "enum": ["text", "markdown", "html"],
                    "description": "Output format: text (strip HTML), markdown (convert HTML), html (raw). Default: text",
                    "default": "text"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30)"
                }
            }
        })
    }

    async fn execute(&self, params: serde_json::Value, ctx: &ToolContext) -> Result<ToolOutput> {
        let url = params["url"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing 'url' parameter"))?;
        let format = params["format"].as_str().unwrap_or("text");
        let timeout_secs = params["timeout"].as_u64().unwrap_or(DEFAULT_TIMEOUT_SECS);

        // Validate URL
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return Ok(ToolOutput {
                title: "WebFetch".to_string(),
                output: "Error: URL must start with http:// or https://".to_string(),
                metadata: serde_json::json!({ "error": "invalid_url" }),
            });
        }

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(timeout_secs))
            .user_agent("opencoder/0.1")
            .build()?;

        let response = tokio::select! {
            result = client.get(url).send() => {
                match result {
                    Ok(r) => r,
                    Err(e) => {
                        return Ok(ToolOutput {
                            title: "WebFetch".to_string(),
                            output: format!("Error fetching URL: {e}"),
                            metadata: serde_json::json!({ "error": e.to_string() }),
                        });
                    }
                }
            }
            _ = ctx.cancel.cancelled() => {
                return Ok(ToolOutput {
                    title: "WebFetch".to_string(),
                    output: "Fetch cancelled.".to_string(),
                    metadata: serde_json::json!({ "cancelled": true }),
                });
            }
        };

        let status = response.status();
        if !status.is_success() {
            return Ok(ToolOutput {
                title: format!("WebFetch ({status})"),
                output: format!("HTTP error: {status}"),
                metadata: serde_json::json!({ "status": status.as_u16() }),
            });
        }

        // Read body with size limit
        let bytes = response.bytes().await?;
        if bytes.len() > MAX_RESPONSE_BYTES {
            return Ok(ToolOutput {
                title: "WebFetch".to_string(),
                output: format!(
                    "Response too large: {} bytes (max {})",
                    bytes.len(),
                    MAX_RESPONSE_BYTES
                ),
                metadata: serde_json::json!({ "error": "too_large", "size": bytes.len() }),
            });
        }

        let body = String::from_utf8_lossy(&bytes).to_string();

        let content = match format {
            "html" => body,
            "text" => strip_html_tags(&body),
            "markdown" => html_to_markdown(&body),
            _ => strip_html_tags(&body),
        };

        let truncated = truncation::truncate_default(&content);

        Ok(ToolOutput {
            title: format!("WebFetch {url}"),
            output: truncated.content,
            metadata: serde_json::json!({
                "url": url,
                "format": format,
                "status": status.as_u16(),
                "bytes": bytes.len(),
                "truncated": truncated.truncated,
            }),
        })
    }
}

/// Simple HTML tag stripping (no dependency needed).
fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let _lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if in_script {
            if lower[i..].starts_with("</script") {
                in_script = false;
                // Skip to end of tag
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }
        if in_style {
            if lower[i..].starts_with("</style") {
                in_style = false;
                while i < chars.len() && chars[i] != '>' {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }

        if chars[i] == '<' {
            if lower[i..].starts_with("<script") {
                in_script = true;
            } else if lower[i..].starts_with("<style") {
                in_style = true;
            }
            in_tag = true;
            i += 1;
            continue;
        }

        if chars[i] == '>' && in_tag {
            in_tag = false;
            i += 1;
            continue;
        }

        if !in_tag {
            result.push(chars[i]);
        }
        i += 1;
    }

    // Decode basic HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

/// Simple HTML to markdown conversion.
fn html_to_markdown(html: &str) -> String {
    // For a production implementation, we'd use htmd crate.
    // This is a simplified version.
    let text = strip_html_tags(html);
    // Clean up whitespace
    let mut result = String::new();
    let mut prev_blank = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !prev_blank {
                result.push('\n');
                prev_blank = true;
            }
        } else {
            result.push_str(trimmed);
            result.push('\n');
            prev_blank = false;
        }
    }
    result
}
