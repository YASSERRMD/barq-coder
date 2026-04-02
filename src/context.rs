use crate::agent::Message;

// ─────────────────────────────────────────────────────────────────────────────
// Context window budget tracking.
// Inspired by Claude Code's three-layer context compression strategy.
// ─────────────────────────────────────────────────────────────────────────────

/// Estimate token count for a string (rough: 1 token per 4 chars).
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

/// Calculate total tokens in a conversation.
pub fn total_tokens(messages: &[Message]) -> usize {
    messages.iter().map(|m| estimate_tokens(&m.content)).sum()
}

/// Context budget tracking.
pub struct ContextBudget {
    /// Maximum tokens allowed in the context window.
    pub max_tokens: usize,
    /// Threshold at which auto-compaction triggers (e.g., 80% of max).
    pub compact_threshold: usize,
}

impl ContextBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            compact_threshold: max_tokens * 80 / 100,
        }
    }

    /// Check if compaction is needed.
    pub fn needs_compact(&self, messages: &[Message]) -> bool {
        total_tokens(messages) > self.compact_threshold
    }

    /// Get remaining token budget.
    pub fn remaining(&self, messages: &[Message]) -> usize {
        let used = total_tokens(messages);
        self.max_tokens.saturating_sub(used)
    }

    /// Get usage as a percentage.
    pub fn usage_percent(&self, messages: &[Message]) -> f32 {
        let used = total_tokens(messages) as f32;
        (used / self.max_tokens as f32) * 100.0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Auto-compact: summarize older messages to free context space.
// Inspired by Claude Code's autoCompact strategy.
// ─────────────────────────────────────────────────────────────────────────────

/// Compact boundary marker — everything before this is summarized.
const COMPACT_BOUNDARY_ROLE: &str = "system";
const COMPACT_BOUNDARY_PREFIX: &str = "[COMPACT SUMMARY]";

/// Find the compact boundary in the conversation.
/// Returns the index after the boundary, or 0 if no boundary exists.
pub fn find_compact_boundary(messages: &[Message]) -> usize {
    for (i, msg) in messages.iter().enumerate().rev() {
        if msg.role == COMPACT_BOUNDARY_ROLE && msg.content.starts_with(COMPACT_BOUNDARY_PREFIX) {
            return i + 1;
        }
    }
    0
}

/// Split messages into (older, recent) at the compact boundary.
pub fn split_at_boundary(messages: &[Message]) -> (&[Message], &[Message]) {
    let boundary = find_compact_boundary(messages);
    messages.split_at(boundary)
}

/// Create a compact summary message from older messages.
/// In a production system, this would call the LLM to summarize.
/// For now, we do a simple extraction.
pub fn build_compact_summary(older_messages: &[Message]) -> String {
    let mut summary = String::from(COMPACT_BOUNDARY_PREFIX);
    summary.push_str("\n\nPrevious conversation summary:\n");

    let mut user_count = 0;
    let mut tool_count = 0;
    let mut topics = Vec::new();

    for msg in older_messages {
        match msg.role.as_str() {
            "user" => {
                user_count += 1;
                // Extract first 100 chars as topic
                let topic = if msg.content.len() > 100 {
                    format!("{}...", &msg.content[..100])
                } else {
                    msg.content.clone()
                };
                if topics.len() < 5 {
                    topics.push(topic);
                }
            }
            "tool" => tool_count += 1,
            _ => {}
        }
    }

    summary.push_str(&format!("- {} user messages, {} tool calls\n", user_count, tool_count));
    summary.push_str("- Topics discussed:\n");
    for topic in &topics {
        summary.push_str(&format!("  - {}\n", topic));
    }

    summary
}

/// Perform auto-compaction on the conversation.
/// Returns the compacted conversation with a summary boundary.
pub fn auto_compact(messages: &mut Vec<Message>, keep_recent: usize) {
    if messages.len() <= keep_recent + 1 {
        return; // Not enough to compact
    }

    // Keep the system prompt (first message) and recent messages
    let system_msg = if !messages.is_empty() && messages[0].role == "system" {
        Some(messages[0].clone())
    } else {
        None
    };

    let split_point = messages.len().saturating_sub(keep_recent);
    let older = &messages[1..split_point]; // Skip system prompt

    if older.is_empty() {
        return;
    }

    let summary = build_compact_summary(older);
    let recent: Vec<Message> = messages[split_point..].to_vec();

    messages.clear();

    // Rebuild: system prompt + compact boundary + recent
    if let Some(sys) = system_msg {
        messages.push(sys);
    }

    messages.push(Message {
        role: COMPACT_BOUNDARY_ROLE.to_string(),
        content: summary,
        tool_calls: None,
        tool_call_id: None,
    });

    messages.extend(recent);
}
