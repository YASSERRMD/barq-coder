/// Lightweight token-cost tracker for a session.
/// Estimates token counts and accumulates totals per turn.
#[derive(Debug, Default, Clone)]
pub struct CostTracker {
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_tool_calls: usize,
    pub turns: usize,
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a turn: prompt tokens sent + completion tokens received.
    pub fn record_turn(&mut self, prompt_tokens: usize, completion_tokens: usize, tool_calls: usize) {
        self.total_prompt_tokens += prompt_tokens;
        self.total_completion_tokens += completion_tokens;
        self.total_tool_calls += tool_calls;
        self.turns += 1;
    }

    pub fn total_tokens(&self) -> usize {
        self.total_prompt_tokens + self.total_completion_tokens
    }

    /// Rough estimate — assumes 4 chars per token on average.
    pub fn estimate_tokens(text: &str) -> usize {
        (text.len() + 3) / 4
    }

    /// Format a human-readable summary.
    pub fn summary(&self) -> String {
        format!(
            "Turns: {}  |  Prompt: {}t  |  Completion: {}t  |  Total: {}t  |  Tool calls: {}",
            self.turns,
            self.total_prompt_tokens,
            self.total_completion_tokens,
            self.total_tokens(),
            self.total_tool_calls,
        )
    }
}
