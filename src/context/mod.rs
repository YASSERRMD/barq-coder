use crate::agent::Message;

pub mod symbolic_injector;

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

// ─────────────────────────────────────────────────────────────────────────────
// Snip Compact: Truncate large tool outputs in history
// ─────────────────────────────────────────────────────────────────────────────



/// Replace extremely long tool outputs with a snipped marker to save tokens,
/// while keeping the record that the tool was called.
pub fn snip_compact(messages: &mut Vec<Message>) {
    let msg_count = messages.len();
    // Don't snip the very last few messages
    let save_recent = 5;
    if msg_count <= save_recent {
        return;
    }

    for i in 1..(msg_count - save_recent) {
        let msg = &mut messages[i];
        if msg.role == "tool" && msg.content.len() > 2000 {
            msg.content = format!(
                "[SNIPPED_OUTPUT: original length {} chars]\n{}...",
                msg.content.len(),
                &msg.content[..500]
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Message;

    fn make_msg(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    // ── ContextBudget ────────────────────────────────────────────────────────

    #[test]
    fn context_budget_needs_compact_when_over_threshold() {
        let budget = ContextBudget::new(100); // 80-token compact threshold
        // 4 chars ≈ 1 token → 320 chars ≈ 80 tokens (at the threshold)
        let messages = vec![make_msg("user", &"a".repeat(324))]; // just over 80 tokens
        assert!(budget.needs_compact(&messages));
    }

    #[test]
    fn context_budget_no_compact_when_under_threshold() {
        let budget = ContextBudget::new(100);
        let messages = vec![make_msg("user", "hello")]; // ~1 token
        assert!(!budget.needs_compact(&messages));
    }

    #[test]
    fn context_budget_remaining_is_saturating() {
        let budget = ContextBudget::new(10);
        // 1000 chars ≈ 250 tokens >> 10 token budget → remaining saturates to 0
        let messages = vec![make_msg("user", &"x".repeat(1000))];
        assert_eq!(budget.remaining(&messages), 0);
    }

    #[test]
    fn context_budget_usage_percent_is_reasonable() {
        let budget = ContextBudget::new(100);
        // 200 chars ≈ 50 tokens → 50% of 100-token budget
        let messages = vec![make_msg("user", &"a".repeat(200))];
        let pct = budget.usage_percent(&messages);
        assert!((45.0..=55.0).contains(&pct), "expected ~50%, got {pct}");
    }

    // ── auto_compact ─────────────────────────────────────────────────────────

    #[test]
    fn auto_compact_preserves_system_and_recent_messages() {
        let mut messages = vec![
            make_msg("system", "You are a coder."),
            make_msg("user", "step 1"),
            make_msg("assistant", "done 1"),
            make_msg("user", "step 2"),
            make_msg("assistant", "done 2"),
            make_msg("user", "step 3"),   // recent
            make_msg("assistant", "done 3"), // recent
        ];
        auto_compact(&mut messages, 2); // keep last 2
        // System message must still be first
        assert_eq!(messages[0].role, "system");
        // Last 2 messages must be retained verbatim
        assert_eq!(messages.last().unwrap().content, "done 3");
    }

    #[test]
    fn auto_compact_inserts_compact_boundary_marker() {
        let mut messages = vec![
            make_msg("system", "sys"),
            make_msg("user", "q1"),
            make_msg("assistant", "a1"),
            make_msg("user", "q2"),      // recent
            make_msg("assistant", "a2"), // recent
        ];
        auto_compact(&mut messages, 2);
        // A compact boundary message must appear between system and recent
        let boundary = messages
            .iter()
            .find(|m| m.content.starts_with(COMPACT_BOUNDARY_PREFIX));
        assert!(boundary.is_some(), "compact boundary marker missing");
    }

    #[test]
    fn auto_compact_is_noop_when_not_enough_messages() {
        let mut messages = vec![
            make_msg("system", "sys"),
            make_msg("user", "q"),
        ];
        let original_len = messages.len();
        auto_compact(&mut messages, 10); // keep_recent > available
        assert_eq!(messages.len(), original_len);
    }

    // ── snip_compact ─────────────────────────────────────────────────────────

    #[test]
    fn snip_compact_truncates_long_tool_output() {
        let long_output = "x".repeat(3000);
        let mut messages = vec![
            make_msg("system", "sys"),
            make_msg("tool", &long_output),
            make_msg("user", "q1"),
            make_msg("user", "q2"),
            make_msg("user", "q3"),
            make_msg("user", "q4"), // recent (save_recent = 5)
            make_msg("user", "q5"),
        ];
        snip_compact(&mut messages);
        assert!(messages[1].content.contains("[SNIPPED_OUTPUT:"));
        assert!(messages[1].content.len() < long_output.len());
    }

    #[test]
    fn snip_compact_does_not_touch_recent_messages() {
        let long_output = "y".repeat(3000);
        // All messages are "recent" (≤ save_recent = 5)
        let mut messages = vec![
            make_msg("tool", &long_output),
            make_msg("user", "q1"),
            make_msg("user", "q2"),
            make_msg("user", "q3"),
            make_msg("user", "q4"),
        ];
        snip_compact(&mut messages);
        // Should not have been snipped — all are within save_recent window
        assert!(!messages[0].content.contains("[SNIPPED_OUTPUT:"));
    }

    #[test]
    fn snip_compact_ignores_short_tool_outputs() {
        let short_output = "small output";
        let mut messages = vec![
            make_msg("system", "sys"),
            make_msg("tool", short_output),
            make_msg("user", "q1"),
            make_msg("user", "q2"),
            make_msg("user", "q3"),
            make_msg("user", "q4"),
            make_msg("user", "q5"),
        ];
        snip_compact(&mut messages);
        assert_eq!(messages[1].content, short_output);
    }
}
