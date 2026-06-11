use serde::{Deserialize, Serialize};

/// What level of tool/function-calling a provider supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolSupportLevel {
    /// Native tool-calling API (e.g. OpenAI function calling, Anthropic tool use).
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

    /// Whether this provider can use tool schemas at all.
    pub fn can_use_tools(&self) -> bool {
        !matches!(self.tool_support, ToolSupportLevel::None)
    }
}
