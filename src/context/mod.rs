use barq_ir::{AgentMessage, MessageRole};

pub type Message = AgentMessage;

pub mod symbolic_injector;

// ─────────────────────────────────────────────────────────────────────────────
// Context window budget tracking.
// ─────────────────────────────────────────────────────────────────────────────

pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

pub fn total_tokens(messages: &[Message]) -> usize {
    messages.iter().map(|m| estimate_tokens(&m.content)).sum()
}

pub struct ContextBudget {
    pub max_tokens: usize,
    pub compact_threshold: usize,
}

impl ContextBudget {
    pub fn new(max_tokens: usize) -> Self {
        Self {
            max_tokens,
            compact_threshold: max_tokens * 80 / 100,
        }
    }

    pub fn needs_compact(&self, messages: &[Message]) -> bool {
        total_tokens(messages) > self.compact_threshold
    }

    pub fn remaining(&self, messages: &[Message]) -> usize {
        self.max_tokens.saturating_sub(total_tokens(messages))
    }

    pub fn usage_percent(&self, messages: &[Message]) -> f32 {
        let used = total_tokens(messages) as f32;
        (used / self.max_tokens as f32) * 100.0
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Auto-compact: summarize older messages to free context space.
// ─────────────────────────────────────────────────────────────────────────────

const COMPACT_BOUNDARY_PREFIX: &str = "[COMPACT SUMMARY]";

pub fn find_compact_boundary(messages: &[Message]) -> usize {
    for (i, msg) in messages.iter().enumerate().rev() {
        if matches!(msg.role, MessageRole::System)
            && msg.content.starts_with(COMPACT_BOUNDARY_PREFIX)
        {
            return i + 1;
        }
    }
    0
}

pub fn split_at_boundary(messages: &[Message]) -> (&[Message], &[Message]) {
    messages.split_at(find_compact_boundary(messages))
}

pub fn build_compact_summary(older_messages: &[Message]) -> String {
    let mut summary = String::from(COMPACT_BOUNDARY_PREFIX);
    summary.push_str("\n\nPrevious conversation summary:\n");

    let mut user_count = 0usize;
    let mut tool_count = 0usize;
    let mut topics: Vec<String> = Vec::new();

    for msg in older_messages {
        match msg.role {
            MessageRole::User => {
                user_count += 1;
                let topic = if msg.content.len() > 100 {
                    format!("{}...", &msg.content[..100])
                } else {
                    msg.content.clone()
                };
                if topics.len() < 5 {
                    topics.push(topic);
                }
            }
            MessageRole::Tool => tool_count += 1,
            _ => {}
        }
    }

    summary.push_str(&format!(
        "- {} user messages, {} tool calls\n- Topics discussed:\n",
        user_count, tool_count
    ));
    for topic in &topics {
        summary.push_str(&format!("  - {}\n", topic));
    }
    summary
}

pub fn auto_compact(messages: &mut Vec<Message>, keep_recent: usize) {
    if messages.len() <= keep_recent + 1 {
        return;
    }

    let system_msg = messages
        .first()
        .filter(|m| matches!(m.role, MessageRole::System))
        .cloned();

    let split_point = messages.len().saturating_sub(keep_recent);
    let older = messages[1..split_point].to_vec();

    if older.is_empty() {
        return;
    }

    let summary = build_compact_summary(&older);
    let recent: Vec<Message> = messages[split_point..].to_vec();

    messages.clear();
    if let Some(sys) = system_msg {
        messages.push(sys);
    }
    messages.push(Message::system(summary));
    messages.extend(recent);
}

pub fn snip_compact(messages: &mut Vec<Message>) {
    let msg_count = messages.len();
    let save_recent = 5;
    if msg_count <= save_recent {
        return;
    }
    for i in 1..(msg_count - save_recent) {
        let msg = &mut messages[i];
        if matches!(msg.role, MessageRole::Tool) && msg.content.len() > 2000 {
            msg.content = format!(
                "[SNIPPED_OUTPUT: original length {} chars]\n{}...",
                msg.content.len(),
                &msg.content[..500]
            );
        }
    }
}
