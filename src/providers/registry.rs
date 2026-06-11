use std::collections::HashMap;
use super::capabilities::{CapabilityOverride, ProviderCapabilities, ToolSupportLevel};

/// Central registry mapping `(provider_id, model_id)` → `ProviderCapabilities`.
///
/// Resolution priority (highest wins):
///   3. User override from `Config.model_capability_overrides`
///   2. Built-in model knowledge (vision models, reasoning models, etc.)
///   1. Provider default (`ProviderCapabilities::ollama_default()` etc.)
///
/// Keys in the overrides map use the form `"provider_id:model_id"`,
/// matching the same format used in `Config.model_capability_overrides`.
pub struct CapabilityRegistry {
    overrides: HashMap<String, CapabilityOverride>,
}

impl CapabilityRegistry {
    /// Empty registry — only provider defaults are used.
    pub fn new() -> Self {
        Self { overrides: HashMap::new() }
    }

    /// Build a registry pre-loaded from the user's config overrides.
    ///
    /// Keys must be `"provider_id:model_id"` (e.g. `"ollama:llava:13b"`).
    pub fn from_config_overrides(overrides: &HashMap<String, CapabilityOverride>) -> Self {
        Self { overrides: overrides.clone() }
    }

    /// Add or replace a user override at runtime (e.g. from env vars or CLI flags).
    pub fn register(&mut self, provider_id: &str, model_id: &str, cap: CapabilityOverride) {
        self.overrides.insert(Self::key(provider_id, model_id), cap);
    }

    /// Resolve capabilities for `(provider_id, model_id)`.
    ///
    /// Start from `provider_default`, apply built-in knowledge, then apply any user override.
    pub fn lookup(
        &self,
        provider_id: &str,
        model_id: &str,
        provider_default: ProviderCapabilities,
    ) -> ProviderCapabilities {
        let mut result = provider_default;

        if let Some(built_in) = Self::built_in(provider_id, model_id) {
            result = built_in.apply_to(result);
        }

        let key = Self::key(provider_id, model_id);
        if let Some(user_override) = self.overrides.get(&key) {
            result = user_override.apply_to(result);
        }

        result
    }

    fn key(provider_id: &str, model_id: &str) -> String {
        format!("{}:{}", provider_id, model_id)
    }

    /// Built-in model capability knowledge derived from well-known naming conventions
    /// and published API documentation for each provider.
    ///
    /// Returns `None` when no special behaviour is detected for this model.
    fn built_in(provider_id: &str, model_id: &str) -> Option<CapabilityOverride> {
        let m = model_id.to_lowercase();
        let mut cap = CapabilityOverride::default();

        match provider_id {
            // ── Ollama (local) ────────────────────────────────────────────────
            "ollama" => {
                if m.contains("vision") || m.contains("llava") || m.contains("bakllava")
                    || m.contains("minicpm-v") || m.contains("moondream")
                {
                    cap.supports_vision = Some(true);
                }
                if m.contains("deepseek-r1") || m.contains("qwq") || m.contains("marco-o1")
                    || m.contains("-think")
                {
                    cap.supports_reasoning = Some(true);
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                }
                if m.ends_with(":1b") || m.contains("phi-2") || m.contains("gemma:2b") {
                    cap.tool_support = Some(ToolSupportLevel::None);
                }
            }

            // ── OpenAI ────────────────────────────────────────────────────────
            "openai" => {
                // o1/o3/o4 reasoning series: no system message, no parallel tool calls
                if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
                    cap.supports_reasoning = Some(true);
                    cap.supports_system_message = Some(false);
                    cap.supports_parallel_tool_calls = Some(false);
                }
                // GPT-4o variants and vision models
                if m.starts_with("gpt-4o") || m.contains("vision") {
                    cap.supports_vision = Some(true);
                }
                // GPT-4.1 and gpt-4-turbo: 128k context
                if m.starts_with("gpt-4-turbo") || m.starts_with("gpt-4.1") {
                    cap.max_context_tokens = Some(128_000);
                }
                // o3/o4 have very large context
                if m.starts_with("o3") || m.starts_with("o4") {
                    cap.max_context_tokens = Some(200_000);
                }
                // GPT-3.5 family: no vision, 16k context
                if m.starts_with("gpt-3.5") {
                    cap.supports_vision = Some(false);
                    cap.max_context_tokens = Some(16_385);
                    cap.supports_parallel_tool_calls = Some(false);
                }
            }

            // ── Anthropic ─────────────────────────────────────────────────────
            "anthropic" => {
                // Extended thinking: claude-3-7-sonnet and claude-opus-4+
                if m.contains("claude-3-7-sonnet") || m.contains("claude-opus-4")
                    || m.contains("claude-sonnet-4") && m.contains("think")
                {
                    cap.supports_reasoning = Some(true);
                }
                // Haiku models: smaller, no parallel tool calls
                if m.contains("haiku") {
                    cap.supports_parallel_tool_calls = Some(false);
                }
                // claude-2.x: no vision, 100k context
                if m.starts_with("claude-2") {
                    cap.supports_vision = Some(false);
                    cap.max_context_tokens = Some(100_000);
                }
                // claude-instant: very small context, no vision, no tool calling
                if m.contains("instant") {
                    cap.supports_vision = Some(false);
                    cap.max_context_tokens = Some(100_000);
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                }
            }

            // ── Google Gemini ─────────────────────────────────────────────────
            "gemini" => {
                // Reasoning/thinking models
                if m.contains("thinking") || m.contains("2.5") || m.contains("exp")
                    && (m.contains("2.5") || m.contains("2.0"))
                {
                    cap.supports_reasoning = Some(true);
                }
                // Gemini 2.5: 2M context
                if m.contains("2.5") {
                    cap.max_context_tokens = Some(2_000_000);
                }
                // Gemini 1.5 Pro: 2M context
                if m.contains("1.5-pro") {
                    cap.max_context_tokens = Some(2_000_000);
                }
                // Gemini 1.5 Flash: 1M context
                if m.contains("1.5-flash") {
                    cap.max_context_tokens = Some(1_000_000);
                }
                // Flash-Lite / nano: smaller
                if m.contains("flash-lite") || m.contains("nano") {
                    cap.max_context_tokens = Some(32_000);
                    cap.supports_parallel_tool_calls = Some(false);
                }
                // Gemma (Google's smaller models) via Gemini API: limited tool support
                if m.contains("gemma") {
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                    cap.supports_vision = Some(false);
                }
            }

            // ── Mistral AI ────────────────────────────────────────────────────
            "mistral" => {
                // Pixtral: vision model
                if m.contains("pixtral") {
                    cap.supports_vision = Some(true);
                    cap.max_context_tokens = Some(131_072);
                }
                // Codestral: code specialist, very large context, unreliable tool use
                if m.contains("codestral") {
                    cap.max_context_tokens = Some(256_000);
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                }
                // Mistral-Nemo: 128k
                if m.contains("nemo") {
                    cap.max_context_tokens = Some(128_000);
                    cap.supports_parallel_tool_calls = Some(false);
                }
                // Mistral-Small: smaller, no parallel tools
                if m.contains("small") {
                    cap.supports_parallel_tool_calls = Some(false);
                }
                // Mistral-Large or Mistral-Medium: full capabilities
                if m.contains("large") || m.contains("medium") {
                    cap.supports_parallel_tool_calls = Some(true);
                }
            }

            // ── Groq ──────────────────────────────────────────────────────────
            "groq" => {
                // Vision models on Groq
                if m.contains("llama-3.2-11b-vision") || m.contains("llama-3.2-90b-vision")
                    || m.contains("vision")
                {
                    cap.supports_vision = Some(true);
                }
                // DeepSeek-R1 via Groq: reasoning, unreliable tool calling
                if m.contains("deepseek-r1") {
                    cap.supports_reasoning = Some(true);
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                }
                // Llama 3.1/3.3 70b+: 131k context
                if (m.contains("llama-3.1-70b") || m.contains("llama-3.3-70b")
                    || m.contains("llama3-70b") || m.contains("llama-3-70b"))
                {
                    cap.max_context_tokens = Some(131_072);
                }
                // Llama 3.1 8b: 131k
                if m.contains("llama-3.1-8b") || m.contains("llama3-8b") {
                    cap.max_context_tokens = Some(131_072);
                }
                // Whisper: audio model — no text generation
                if m.contains("whisper") {
                    cap.tool_support = Some(ToolSupportLevel::None);
                    cap.supports_streaming = Some(false);
                }
                // Gemma 2: 8k context
                if m.contains("gemma2") || m.contains("gemma-2") {
                    cap.max_context_tokens = Some(8_192);
                }
                // Mixtral: 32k
                if m.contains("mixtral") {
                    cap.max_context_tokens = Some(32_768);
                }
            }

            // ── Together AI ───────────────────────────────────────────────────
            "together" => {
                // Vision / multimodal models
                if m.contains("vision") || m.contains("-vl") || m.contains("llava") {
                    cap.supports_vision = Some(true);
                }
                // Reasoning / chain-of-thought models
                if m.contains("qwq") || m.contains("deepseek-r1") || m.contains("r1") {
                    cap.supports_reasoning = Some(true);
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                }
                // Llama 3.1/3.3 70b+: 131k
                if m.contains("llama-3.1-70b") || m.contains("llama-3.3-70b")
                    || m.contains("llama-3.1-405b")
                {
                    cap.max_context_tokens = Some(131_072);
                }
                // Qwen 2.5 Coder: good for code, 128k
                if m.contains("qwen2.5-coder") || m.contains("qwen2p5-coder") {
                    cap.max_context_tokens = Some(128_000);
                }
                // Mixtral: 32k
                if m.contains("mixtral") {
                    cap.max_context_tokens = Some(32_768);
                }
            }

            // ── DeepSeek ──────────────────────────────────────────────────────
            "deepseek" => {
                // DeepSeek Reasoner (R1): reasoning model, NO native tool calling
                if m.contains("reasoner") || m.contains("r1") {
                    cap.supports_reasoning = Some(true);
                    cap.tool_support = Some(ToolSupportLevel::None);
                    cap.max_context_tokens = Some(64_000);
                }
                // DeepSeek-V3 / deepseek-chat: native tools, 64k
                if m.contains("deepseek-chat") || m.contains("v3") || m.contains("v2.5") {
                    cap.tool_support = Some(ToolSupportLevel::Native);
                    cap.max_context_tokens = Some(64_000);
                }
                // DeepSeek Coder: code specialist
                if m.contains("coder") {
                    cap.max_context_tokens = Some(128_000);
                }
            }

            // ── xAI Grok ──────────────────────────────────────────────────────
            "xai" => {
                // Grok-2-Vision: vision model
                if m.contains("vision") || m.contains("grok-2-vision") {
                    cap.supports_vision = Some(true);
                }
                // Grok-3-Mini: reasoning / thinking model
                if m.contains("grok-3-mini") || m.contains("mini") {
                    cap.supports_reasoning = Some(true);
                }
                // All Grok: 131k context
                cap.max_context_tokens = Some(131_072);
            }

            // ── Perplexity ────────────────────────────────────────────────────
            "perplexity" => {
                // All sonar models use built-in search, no function calling
                cap.tool_support = Some(ToolSupportLevel::None);
                // Reasoning models
                if m.contains("reasoning") || m.contains("r1") {
                    cap.supports_reasoning = Some(true);
                }
                // Sonar Pro: larger context
                if m.contains("pro") {
                    cap.max_context_tokens = Some(200_000);
                }
                // Sonar Huge: 127k
                if m.contains("huge") {
                    cap.max_context_tokens = Some(127_072);
                }
            }

            // ── Cohere ────────────────────────────────────────────────────────
            "cohere" => {
                // Command R+: best tool calling support
                if m.contains("command-r-plus") || m.contains("command-r+") {
                    cap.supports_parallel_tool_calls = Some(true);
                    cap.max_context_tokens = Some(128_000);
                }
                // Command R: standard
                if m.contains("command-r") && !m.contains("plus") {
                    cap.supports_parallel_tool_calls = Some(false);
                    cap.max_context_tokens = Some(128_000);
                }
                // Older Command / Command-Light: limited
                if m == "command" || m.contains("command-light") {
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                    cap.max_context_tokens = Some(4_096);
                    cap.supports_parallel_tool_calls = Some(false);
                }
                // Aya: multilingual, no tool calling
                if m.contains("aya") {
                    cap.tool_support = Some(ToolSupportLevel::None);
                }
            }

            // ── Fireworks AI ──────────────────────────────────────────────────
            "fireworks" => {
                // Vision models
                if m.contains("vision") || m.contains("llava") {
                    cap.supports_vision = Some(true);
                }
                // Reasoning models
                if m.contains("deepseek-r1") || m.contains("qwq") {
                    cap.supports_reasoning = Some(true);
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                }
                // Qwen 2.5 Coder: coding specialist
                if m.contains("qwen2p5-coder") || m.contains("qwen2.5-coder") {
                    cap.max_context_tokens = Some(128_000);
                }
                // Llama 3.1 70b+: 131k
                if m.contains("llama-v3p1-70b") || m.contains("llama-v3p1-405b") {
                    cap.max_context_tokens = Some(131_072);
                }
                // Mixtral: 32k
                if m.contains("mixtral") {
                    cap.max_context_tokens = Some(32_768);
                }
                // Phi-3: limited tool use
                if m.contains("phi-3") {
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                    cap.max_context_tokens = Some(131_072);
                }
            }

            // ── Cerebras ──────────────────────────────────────────────────────
            "cerebras" => {
                // Qwen3: best model on Cerebras
                if m.contains("qwen-3") || m.contains("qwen3") {
                    cap.max_context_tokens = Some(131_072);
                }
                // Llama 3.1: 131k
                if m.contains("llama3.1") || m.contains("llama-3.1") {
                    cap.max_context_tokens = Some(131_072);
                }
                // Scout models: 131k
                if m.contains("scout") {
                    cap.max_context_tokens = Some(131_072);
                }
            }

            _ => {}
        }

        if cap.has_any() { Some(cap) } else { None }
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}
