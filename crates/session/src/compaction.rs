//! Context compaction: summarizes conversation when approaching token limits.
//!
//! Mirrors `src/session/compaction.ts` from the original OpenCode.

use std::sync::Arc;

use anyhow::Result;
use tracing::{debug, info};

use opencoder_core::config::Config;
use opencoder_provider::provider::{ChatMessage, ChatRequest, LlmProvider, Role};

use crate::message::{CompactionPart, MessageWithParts, Part};
use crate::session::SessionService;

/// Check if the current token usage exceeds the context limit.
pub fn is_overflow(
    input_tokens: u64,
    model_context_limit: u64,
    max_output: u64,
    config: &Config,
) -> bool {
    let reserved = config
        .compaction
        .as_ref()
        .and_then(|c| c.reserved)
        .map(|r| r as u64)
        .unwrap_or_else(|| max_output.min(20_000));
    let usable = model_context_limit.saturating_sub(reserved);
    input_tokens >= usable
}

/// Prune large tool outputs from older messages to free up context.
///
/// Returns the estimated number of tokens saved.
pub fn prune(messages: &[MessageWithParts], session_svc: &SessionService) -> Result<u64> {
    const PRUNE_THRESHOLD_CHARS: usize = 40_000;
    let mut saved = 0u64;

    // Keep the last 2 user turns untouched
    let mut user_turn_count = 0;
    let mut cutoff_idx = messages.len();
    for (i, msg) in messages.iter().enumerate().rev() {
        if matches!(msg.message, crate::message::Message::User(_)) {
            user_turn_count += 1;
            if user_turn_count >= 2 {
                cutoff_idx = i;
                break;
            }
        }
    }

    // Prune tool outputs in older messages
    for msg in messages.iter().take(cutoff_idx) {
        for part_with_id in &msg.parts {
            if let Part::Tool(tool_part) = &part_with_id.part
                && let crate::message::ToolState::Completed { output, .. } = &tool_part.state
                && output.len() > PRUNE_THRESHOLD_CHARS
            {
                // Mark as compacted by replacing with truncated output
                let truncated = format!(
                    "[Output compacted: was {} chars. Use tool again if needed.]",
                    output.len()
                );
                let chars_saved = output.len() - truncated.len();
                saved += (chars_saved / 4) as u64; // rough char-to-token ratio

                let mut compacted_part = tool_part.clone();
                if let crate::message::ToolState::Completed { ref mut output, .. } =
                    compacted_part.state
                {
                    *output = truncated;
                }

                session_svc.update_part(&part_with_id.id, &Part::Tool(compacted_part))?;

                debug!(
                    part_id = %part_with_id.id,
                    chars_saved,
                    "pruned large tool output"
                );
            }
        }
    }

    Ok(saved)
}

/// Run a full compaction: summarize the conversation and add a CompactionPart.
pub async fn process(
    session_id: &str,
    session_svc: &SessionService,
    provider: &Arc<dyn LlmProvider>,
    model: &str,
) -> Result<()> {
    let messages = session_svc.messages(session_id)?;

    if messages.is_empty() {
        return Ok(());
    }

    // First try pruning
    let pruned = prune(&messages, session_svc)?;
    if pruned > 0 {
        info!(session_id, tokens_saved = pruned, "pruned large outputs");
    }

    // Build a summary prompt
    let mut conversation_text = String::new();
    for msg in &messages {
        match &msg.message {
            crate::message::Message::User(u) => {
                conversation_text.push_str(&format!("User: {}\n", u.content));
            }
            crate::message::Message::Assistant(_) => {
                for part in &msg.parts {
                    if let Part::Text(text) = &part.part {
                        conversation_text.push_str(&format!("Assistant: {}\n", text.content));
                    }
                }
            }
        }
    }

    // Truncate if too long
    if conversation_text.len() > 50_000 {
        conversation_text.truncate(50_000);
        conversation_text.push_str("\n[conversation truncated for summarization]");
    }

    let summary_prompt = format!(
        "Summarize this conversation concisely. Focus on:\n\
        - The user's goal\n\
        - Key discoveries and decisions made\n\
        - What has been accomplished\n\
        - Important file paths and code changes\n\
        - What still needs to be done\n\n\
        Conversation:\n{conversation_text}"
    );

    let request = ChatRequest::new(
        model.to_string(),
        vec![
            ChatMessage::text(
                Role::System,
                "You are a conversation summarizer. Produce a concise summary.",
            ),
            ChatMessage::text(Role::User, &summary_prompt),
        ],
    );

    let cancel = tokio_util::sync::CancellationToken::new();
    let response = provider.chat(request, cancel).await?;
    let summary = response.content;

    if summary.is_empty() {
        return Ok(());
    }

    // Find the last assistant message to attach the compaction part
    let last_msg = messages
        .last()
        .ok_or_else(|| anyhow::anyhow!("no messages to compact"))?;

    session_svc.add_part(
        session_id,
        &last_msg.id,
        &Part::Compaction(CompactionPart {
            summary,
            compacted_count: messages.len() as u32,
        }),
    )?;

    info!(session_id, messages = messages.len(), "compaction complete");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_detection() {
        let config = Config::default();
        // 200k context, 8k output, default reserved = min(20k, 8k) = 8k
        // usable = 200k - 8k = 192k
        assert!(!is_overflow(100_000, 200_000, 8_000, &config));
        assert!(is_overflow(195_000, 200_000, 8_000, &config));
    }

    #[test]
    fn overflow_with_custom_reserved() {
        let config = Config {
            compaction: Some(opencoder_core::config::CompactionConfig {
                reserved: Some(50_000),
                ..Default::default()
            }),
            ..Default::default()
        };
        // usable = 200k - 50k = 150k
        assert!(!is_overflow(140_000, 200_000, 8_000, &config));
        assert!(is_overflow(155_000, 200_000, 8_000, &config));
    }
}
