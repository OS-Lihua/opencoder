//! Output truncation for tool results.
//!
//! Mirrors `src/tool/truncation.ts` from the original OpenCode.
//! Limits tool output to MAX_LINES/MAX_BYTES to avoid flooding the LLM context.

/// Default maximum lines in tool output.
pub const MAX_LINES: usize = 2000;
/// Default maximum bytes in tool output.
pub const MAX_BYTES: usize = 50 * 1024;

/// Result of truncation.
#[derive(Debug, Clone)]
pub struct TruncationResult {
    pub content: String,
    pub truncated: bool,
}

/// Truncate output if it exceeds line or byte limits.
///
/// When truncating from "head" (default), keeps the first N lines.
/// When truncating from "tail", keeps the last N lines.
pub fn truncate(
    output: &str,
    max_lines: usize,
    max_bytes: usize,
    from_tail: bool,
) -> TruncationResult {
    let lines: Vec<&str> = output.lines().collect();
    let total_lines = lines.len();
    let total_bytes = output.len();

    if total_lines <= max_lines && total_bytes <= max_bytes {
        return TruncationResult {
            content: output.to_string(),
            truncated: false,
        };
    }

    // Truncate by lines first
    let selected: Vec<&str> = if from_tail {
        let start = total_lines.saturating_sub(max_lines);
        lines[start..].to_vec()
    } else {
        lines[..max_lines.min(total_lines)].to_vec()
    };

    // Then truncate by bytes
    let mut result = String::new();
    let mut bytes_used = 0;
    for (i, line) in selected.iter().enumerate() {
        let line_bytes = line.len() + 1; // +1 for newline
        if bytes_used + line_bytes > max_bytes && i > 0 {
            break;
        }
        if i > 0 {
            result.push('\n');
        }
        result.push_str(line);
        bytes_used += line_bytes;
    }

    let direction = if from_tail { "beginning" } else { "end" };
    let hint = format!(
        "\n\n[Output truncated: showing {}/{} lines, {}/{} bytes. Content from the {} was omitted.]",
        result.lines().count(),
        total_lines,
        result.len(),
        total_bytes,
        direction,
    );
    result.push_str(&hint);

    TruncationResult {
        content: result,
        truncated: true,
    }
}

/// Convenience wrapper with default limits and head truncation.
pub fn truncate_default(output: &str) -> TruncationResult {
    truncate(output, MAX_LINES, MAX_BYTES, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_truncation_for_short_output() {
        let result = truncate("hello\nworld", MAX_LINES, MAX_BYTES, false);
        assert!(!result.truncated);
        assert_eq!(result.content, "hello\nworld");
    }

    #[test]
    fn truncate_by_lines() {
        let input: String = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate(&input, 10, MAX_BYTES, false);
        assert!(result.truncated);
        assert!(result.content.contains("line 0"));
        assert!(result.content.contains("line 9"));
        assert!(!result.content.contains("line 50"));
        assert!(result.content.contains("[Output truncated"));
    }

    #[test]
    fn truncate_by_bytes() {
        let input = "x".repeat(100_000);
        let result = truncate(&input, MAX_LINES, 1024, false);
        assert!(result.truncated);
        // Content is the single long line (kept because i==0) + hint
        assert!(result.content.contains("[Output truncated"));
    }

    #[test]
    fn truncate_from_tail() {
        let input: String = (0..100)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = truncate(&input, 10, MAX_BYTES, true);
        assert!(result.truncated);
        assert!(result.content.contains("line 99"));
        assert!(result.content.contains("line 90"));
        assert!(!result.content.contains("line 0"));
    }
}
