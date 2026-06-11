use std::collections::HashMap;
use super::capabilities::{CapabilityOverride, ProviderCapabilities, ToolSupportLevel};

/// Central registry that maps `(provider_id, model_id)` → `ProviderCapabilities`.
///
/// Resolution priority (highest wins):
///   3. User override from `Config.model_capability_overrides`
///   2. Built-in model knowledge baked into this file
///   1. Provider-level default (passed by the adapter as the `provider_default`)
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

    /// Built-in model knowledge derived from well-known naming conventions.
    /// Returns `None` when no special behaviour is detected for this model.
    fn built_in(provider_id: &str, model_id: &str) -> Option<CapabilityOverride> {
        let m = model_id.to_lowercase();
        let mut cap = CapabilityOverride::default();

        match provider_id {
            "ollama" => {
                // Vision models
                if m.contains("vision")
                    || m.contains("llava")
                    || m.contains("bakllava")
                    || m.contains("minicpm-v")
                    || m.contains("moondream")
                {
                    cap.supports_vision = Some(true);
                }
                // Chain-of-thought / reasoning models — emit <think> blocks; tool calling is
                // unreliable with the native API, so fall back to prompt injection.
                if m.contains("deepseek-r1")
                    || m.contains("qwq")
                    || m.contains("marco-o1")
                    || m.contains("-think")
                {
                    cap.supports_reasoning = Some(true);
                    cap.tool_support = Some(ToolSupportLevel::Prompted);
                }
                // Very small models that rarely parse tool schemas reliably
                if m.ends_with(":1b") || m.contains("phi-2") || m.contains("gemma:2b") {
                    cap.tool_support = Some(ToolSupportLevel::None);
                }
            }
            "openai" => {
                // o1 / o3 / o4 reasoning series: no system message, no parallel tool calls
                if m.starts_with("o1") || m.starts_with("o3") || m.starts_with("o4") {
                    cap.supports_reasoning = Some(true);
                    cap.supports_system_message = Some(false);
                    cap.supports_parallel_tool_calls = Some(false);
                }
                // GPT-4o variants and explicit vision models support image input
                if m.starts_with("gpt-4o") || m.contains("vision") {
                    cap.supports_vision = Some(true);
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
