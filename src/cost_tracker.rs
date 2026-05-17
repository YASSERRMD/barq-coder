use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// Per-model USD pricing (prompt + completion per 1k tokens).
// Defaults for common Ollama-served models are free; remote API models carry
// real costs. Values can be overridden via config.
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPricing {
    /// Cost in USD per 1 000 prompt tokens.
    pub prompt_per_1k: f64,
    /// Cost in USD per 1 000 completion tokens.
    pub completion_per_1k: f64,
}

impl ModelPricing {
    pub const fn new(prompt_per_1k: f64, completion_per_1k: f64) -> Self {
        Self { prompt_per_1k, completion_per_1k }
    }

    /// Known models table. Falls back to zero-cost (local Ollama) if unknown.
    pub fn for_model(model: &str) -> Self {
        match model {
            // OpenAI / proxy
            m if m.contains("gpt-4o")            => Self::new(0.005,  0.015),
            m if m.contains("gpt-4-turbo")        => Self::new(0.010,  0.030),
            m if m.contains("gpt-3.5")            => Self::new(0.0005, 0.0015),
            // Anthropic Claude 4.x
            m if m.contains("claude-opus-4")      => Self::new(0.015,  0.075),
            m if m.contains("claude-sonnet-4")    => Self::new(0.003,  0.015),
            m if m.contains("claude-haiku-4")     => Self::new(0.00025, 0.00125),
            // Anthropic Claude 3.x
            m if m.contains("claude-3-5-sonnet")  => Self::new(0.003,  0.015),
            m if m.contains("claude-3-opus")      => Self::new(0.015,  0.075),
            m if m.contains("claude-3-haiku")     => Self::new(0.00025, 0.00125),
            // Google Gemini proxy
            m if m.contains("gemini-2.5-pro")     => Self::new(0.00125, 0.010),
            m if m.contains("gemini-2.0-flash")   => Self::new(0.000075, 0.0003),
            m if m.contains("gemini-1.5-pro")     => Self::new(0.007,  0.021),
            m if m.contains("gemini-1.5-flash")   => Self::new(0.000075, 0.0003),
            // Ollama local — effectively free
            _ => Self::new(0.0, 0.0),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Budget state emitted by the enforcer.
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq)]
pub enum BudgetStatus {
    /// Well within budget.
    Ok,
    /// Approaching the cap — show a warning in the TUI.
    Warning { used_usd: f64, cap_usd: f64, pct: u8 },
    /// Hard limit hit. Agent loop must pause and await user confirmation.
    Paused { used_usd: f64, cap_usd: f64 },
}

// ─────────────────────────────────────────────────────────────────────────────
// CostTracker — upgraded with per-model USD accounting.
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostTracker {
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_tool_calls: usize,
    pub turns: usize,
    /// Accumulated USD cost this session.
    pub total_usd: f64,
    /// Optional hard budget cap in USD. None = unlimited.
    pub budget_cap_usd: Option<f64>,
    /// Warning threshold as a fraction of budget_cap (default 0.80).
    pub warn_threshold: f64,
    /// Pricing for the current model.
    #[serde(skip)]
    pub pricing: Option<ModelPricing>,
}

impl Default for CostTracker {
    fn default() -> Self {
        Self {
            total_prompt_tokens: 0,
            total_completion_tokens: 0,
            total_tool_calls: 0,
            turns: 0,
            total_usd: 0.0,
            budget_cap_usd: None,
            warn_threshold: 0.80,
            pricing: None,
        }
    }
}

impl CostTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Configure USD pricing for the model in use.
    pub fn with_model(mut self, model: &str) -> Self {
        self.pricing = Some(ModelPricing::for_model(model));
        self
    }

    /// Set a hard USD budget cap. Agent loop pauses at this threshold.
    pub fn with_budget_cap(mut self, cap_usd: f64) -> Self {
        self.budget_cap_usd = Some(cap_usd);
        self
    }

    /// Record a turn and accumulate cost. Returns the new BudgetStatus.
    pub fn record_turn(
        &mut self,
        prompt_tokens: usize,
        completion_tokens: usize,
        tool_calls: usize,
    ) -> BudgetStatus {
        self.total_prompt_tokens += prompt_tokens;
        self.total_completion_tokens += completion_tokens;
        self.total_tool_calls += tool_calls;
        self.turns += 1;

        // Compute incremental USD cost.
        if let Some(p) = &self.pricing {
            let cost = (prompt_tokens as f64 / 1000.0) * p.prompt_per_1k
                + (completion_tokens as f64 / 1000.0) * p.completion_per_1k;
            self.total_usd += cost;
        }

        self.budget_status()
    }

    /// Current budget status without recording a new turn.
    pub fn budget_status(&self) -> BudgetStatus {
        let Some(cap) = self.budget_cap_usd else {
            return BudgetStatus::Ok;
        };
        let pct = ((self.total_usd / cap) * 100.0).round() as u8;

        if self.total_usd >= cap {
            BudgetStatus::Paused {
                used_usd: self.total_usd,
                cap_usd: cap,
            }
        } else if (self.total_usd / cap) >= self.warn_threshold {
            BudgetStatus::Warning {
                used_usd: self.total_usd,
                cap_usd: cap,
                pct,
            }
        } else {
            BudgetStatus::Ok
        }
    }

    /// Check if the budget is exhausted and the agent must pause.
    pub fn is_paused(&self) -> bool {
        matches!(self.budget_status(), BudgetStatus::Paused { .. })
    }

    pub fn total_tokens(&self) -> usize {
        self.total_prompt_tokens + self.total_completion_tokens
    }

    /// Rough token estimate from raw text (4 chars ≈ 1 token).
    pub fn estimate_tokens(text: &str) -> usize {
        (text.len() + 3) / 4
    }

    /// Human-readable summary for TUI status bar.
    pub fn summary(&self) -> String {
        if self.total_usd > 0.0 {
            format!(
                "Turns: {}  |  Tokens: {}  |  Cost: ${:.4}{}",
                self.turns,
                self.total_tokens(),
                self.total_usd,
                self.budget_cap_usd
                    .map(|c| format!(" / ${:.2}", c))
                    .unwrap_or_default()
            )
        } else {
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
}
