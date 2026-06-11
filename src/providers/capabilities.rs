use serde::{Deserialize, Serialize};

/// What level of tool/function-calling a provider supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ToolSupportLevel {
    /// Native tool-calling API (e.g. OpenAI function calling, Anthropic tool use).
    #[default]
    Native,
    /// Tool calls injected via prompt engineering — arguments may be less reliable.
    Prompted,
    /// Grammar-constrained generation to force a schema (e.g. llama.cpp GBNF).
    Constrained,
    /// No tool-calling support — executor must handle all actions inline.
    None,
}

/// Capability flags for a specific provider + model combination.
///
/// Adapters return this from `capabilities()`. The executor queries it to gate
/// features so tool schemas are never sent to providers that can't use them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    pub tool_support: ToolSupportLevel,
    pub supports_streaming: bool,
    pub supports_reasoning: bool,
    pub supports_vision: bool,
    /// Maximum context window in tokens; 0 means unknown.
    pub max_context_tokens: u32,
    pub supports_system_message: bool,
    pub supports_parallel_tool_calls: bool,
}

impl ProviderCapabilities {
    pub fn minimal() -> Self {
        Self {
            tool_support: ToolSupportLevel::None,
            supports_streaming: false,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 0,
            supports_system_message: true,
            supports_parallel_tool_calls: false,
        }
    }

    // ── Local / self-hosted ───────────────────────────────────────────────────

    pub fn ollama_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 128_000,
            supports_system_message: true,
            supports_parallel_tool_calls: true,
        }
    }

    // ── Cloud providers ───────────────────────────────────────────────────────

    pub fn openai_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: true,
            max_context_tokens: 128_000,
            supports_system_message: true,
            supports_parallel_tool_calls: true,
        }
    }

    /// Anthropic Messages API — claude-3+ all support vision and 200k context.
    pub fn anthropic_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: true,
            max_context_tokens: 200_000,
            supports_system_message: true,
            supports_parallel_tool_calls: true,
        }
    }

    /// Google Gemini via OpenAI-compatible endpoint — 1M context, vision, native tools.
    pub fn gemini_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: true,
            max_context_tokens: 1_000_000,
            supports_system_message: true,
            supports_parallel_tool_calls: true,
        }
    }

    /// Mistral AI — 128k context, native tools; vision only on pixtral models.
    pub fn mistral_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 128_000,
            supports_system_message: true,
            supports_parallel_tool_calls: true,
        }
    }

    /// Groq ultra-fast inference — conservative 32k default; specific models vary.
    pub fn groq_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 32_768,
            supports_system_message: true,
            supports_parallel_tool_calls: false,
        }
    }

    /// Together AI — open-model hosting; 32k conservative default.
    pub fn together_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 32_768,
            supports_system_message: true,
            supports_parallel_tool_calls: false,
        }
    }

    /// DeepSeek API — deepseek-chat (V3) has native tools; deepseek-reasoner does not.
    pub fn deepseek_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 64_000,
            supports_system_message: true,
            supports_parallel_tool_calls: false,
        }
    }

    /// xAI Grok — 131k context, native tools; vision only on grok-2-vision models.
    pub fn xai_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 131_072,
            supports_system_message: true,
            supports_parallel_tool_calls: false,
        }
    }

    /// Perplexity sonar — uses built-in web search instead of function calling.
    pub fn perplexity_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::None,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 127_072,
            supports_system_message: true,
            supports_parallel_tool_calls: false,
        }
    }

    /// Cohere Command — native tool calling, 128k context.
    pub fn cohere_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 128_000,
            supports_system_message: true,
            supports_parallel_tool_calls: true,
        }
    }

    /// Fireworks AI serverless inference — OpenAI-compatible, 131k context.
    pub fn fireworks_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 131_072,
            supports_system_message: true,
            supports_parallel_tool_calls: true,
        }
    }

    /// Cerebras ultra-fast inference — OpenAI-compatible, 131k context.
    pub fn cerebras_default() -> Self {
        Self {
            tool_support: ToolSupportLevel::Native,
            supports_streaming: true,
            supports_reasoning: false,
            supports_vision: false,
            max_context_tokens: 131_072,
            supports_system_message: true,
            supports_parallel_tool_calls: false,
        }
    }

    /// Whether this provider can use tool schemas at all.
    pub fn can_use_tools(&self) -> bool {
        !matches!(self.tool_support, ToolSupportLevel::None)
    }
}

/// Per-model capability override — all fields are optional; `None` means "use provider default".
///
/// Used by `CapabilityRegistry` and serialised in `Config.model_capability_overrides`
/// so users can teach the system about new or unusual models without changing code.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CapabilityOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_support: Option<ToolSupportLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_streaming: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_reasoning: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_vision: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_context_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_system_message: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_parallel_tool_calls: Option<bool>,
}

impl CapabilityOverride {
    /// Apply this override on top of `base`, returning a merged `ProviderCapabilities`.
    pub fn apply_to(&self, base: ProviderCapabilities) -> ProviderCapabilities {
        ProviderCapabilities {
            tool_support: self.tool_support.unwrap_or(base.tool_support),
            supports_streaming: self.supports_streaming.unwrap_or(base.supports_streaming),
            supports_reasoning: self.supports_reasoning.unwrap_or(base.supports_reasoning),
            supports_vision: self.supports_vision.unwrap_or(base.supports_vision),
            max_context_tokens: self.max_context_tokens.unwrap_or(base.max_context_tokens),
            supports_system_message: self.supports_system_message.unwrap_or(base.supports_system_message),
            supports_parallel_tool_calls: self.supports_parallel_tool_calls.unwrap_or(base.supports_parallel_tool_calls),
        }
    }

    /// True if at least one field is set (i.e. this override does something).
    pub fn has_any(&self) -> bool {
        self.tool_support.is_some()
            || self.supports_streaming.is_some()
            || self.supports_reasoning.is_some()
            || self.supports_vision.is_some()
            || self.max_context_tokens.is_some()
            || self.supports_system_message.is_some()
            || self.supports_parallel_tool_calls.is_some()
    }
}
