use async_trait::async_trait;
use barq_ir::{AgentMessage, AgentTurn, StreamEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

pub mod anthropic;
pub mod capabilities;
pub mod ollama;
pub mod openai;
pub mod registry;
pub mod trust_tiers;

pub use capabilities::{CapabilityOverride, ProviderCapabilities};
pub use registry::CapabilityRegistry;
pub use trust_tiers::TrustTier;

/// Discriminant used in Config to select which adapter to build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    // ── Local / self-hosted ───────────────────────────────────────────────────
    Ollama,

    // ── OpenAI ────────────────────────────────────────────────────────────────
    OpenAi,

    // ── Anthropic ─────────────────────────────────────────────────────────────
    Anthropic,

    // ── Google Gemini (via OpenAI-compatible endpoint) ─────────────────────────
    Gemini,

    // ── Mistral AI ────────────────────────────────────────────────────────────
    Mistral,

    // ── Groq ultra-fast inference ─────────────────────────────────────────────
    Groq,

    // ── Together AI open-model hosting ───────────────────────────────────────
    Together,

    // ── DeepSeek ─────────────────────────────────────────────────────────────
    DeepSeek,

    // ── xAI Grok ─────────────────────────────────────────────────────────────
    Xai,

    // ── Perplexity sonar ─────────────────────────────────────────────────────
    Perplexity,

    // ── Cohere Command ────────────────────────────────────────────────────────
    Cohere,

    // ── Fireworks AI serverless ───────────────────────────────────────────────
    Fireworks,

    // ── Cerebras ultra-fast inference ────────────────────────────────────────
    Cerebras,
}

impl Default for ProviderKind {
    fn default() -> Self {
        Self::Ollama
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Ollama => "ollama",
            Self::OpenAi => "openai",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::Mistral => "mistral",
            Self::Groq => "groq",
            Self::Together => "together",
            Self::DeepSeek => "deepseek",
            Self::Xai => "xai",
            Self::Perplexity => "perplexity",
            Self::Cohere => "cohere",
            Self::Fireworks => "fireworks",
            Self::Cerebras => "cerebras",
        };
        write!(f, "{}", s)
    }
}

/// The canonical adapter contract.
///
/// Every model-specific behaviour (request format, response parsing, streaming
/// protocol, tool-call serialisation) lives inside an implementation of this
/// trait. The executor (Orchestrator + all agents) depends ONLY on this trait
/// and never on any provider-native type. This is the boundary the PDF calls
/// the "Adapter / Provider Layer".
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Short stable identifier, e.g. "ollama", "openai".
    fn provider_id(&self) -> &str;

    /// The model identifier this adapter is configured with.
    fn model_id(&self) -> &str;

    /// Capability profile for this provider + model pair.
    /// Resolved through the CapabilityRegistry: provider default → built-in
    /// model knowledge → user Config overrides.
    fn capabilities(&self) -> ProviderCapabilities;

    /// Trust tier that gates which tools this provider may invoke.
    fn trust_tier(&self) -> TrustTier;

    /// Start a streaming chat request.
    /// Returns a channel that emits canonical `StreamEvent`s.
    /// The executor must NEVER receive raw provider output — only events from here.
    fn chat_stream(
        &self,
        messages: Vec<AgentMessage>,
        tools: Option<Vec<Value>>,
    ) -> mpsc::Receiver<StreamEvent>;

    /// Non-streaming chat — returns a complete canonical `AgentTurn`.
    async fn chat(
        &self,
        messages: Vec<AgentMessage>,
        tools: Option<Vec<Value>>,
    ) -> Result<AgentTurn, String>;
}

/// Build a boxed `ProviderAdapter` from the runtime configuration.
///
/// Constructs a `CapabilityRegistry` pre-loaded from `config.model_capability_overrides`
/// and shares it (via `Arc`) across all adapter instances so every capability
/// lookup reflects both built-in knowledge and user overrides.
pub fn build_provider(config: &crate::config::Config) -> Arc<dyn ProviderAdapter> {
    let registry = Arc::new(CapabilityRegistry::from_config_overrides(
        &config.model_capability_overrides,
    ));

    match config.provider {
        ProviderKind::Ollama => Arc::new(ollama::OllamaAdapter::new(
            &config.ollama_base_url,
            &config.ollama_model,
            Arc::clone(&registry),
        )),

        ProviderKind::OpenAi => Arc::new(openai::OpenAiAdapter::new(
            config.openai_base_url.as_deref().unwrap_or("https://api.openai.com"),
            config.openai_model.as_deref().unwrap_or("gpt-4o"),
            config.openai_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Anthropic => Arc::new(anthropic::AnthropicAdapter::new(
            config.anthropic_model.as_deref().unwrap_or("claude-sonnet-4-6"),
            config.anthropic_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Gemini => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "gemini",
            config.gemini_base_url.as_deref()
                .unwrap_or("https://generativelanguage.googleapis.com/v1beta/openai"),
            "/chat/completions",
            config.gemini_model.as_deref().unwrap_or("gemini-2.5-pro"),
            config.gemini_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Mistral => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "mistral",
            config.mistral_base_url.as_deref().unwrap_or("https://api.mistral.ai"),
            "/v1/chat/completions",
            config.mistral_model.as_deref().unwrap_or("mistral-large-latest"),
            config.mistral_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Groq => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "groq",
            config.groq_base_url.as_deref().unwrap_or("https://api.groq.com/openai"),
            "/v1/chat/completions",
            config.groq_model.as_deref().unwrap_or("llama-3.3-70b-versatile"),
            config.groq_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Together => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "together",
            config.together_base_url.as_deref().unwrap_or("https://api.together.xyz"),
            "/v1/chat/completions",
            config.together_model.as_deref()
                .unwrap_or("meta-llama/Llama-3.3-70B-Instruct-Turbo"),
            config.together_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::DeepSeek => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "deepseek",
            config.deepseek_base_url.as_deref().unwrap_or("https://api.deepseek.com"),
            "/v1/chat/completions",
            config.deepseek_model.as_deref().unwrap_or("deepseek-chat"),
            config.deepseek_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Xai => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "xai",
            config.xai_base_url.as_deref().unwrap_or("https://api.x.ai"),
            "/v1/chat/completions",
            config.xai_model.as_deref().unwrap_or("grok-3-latest"),
            config.xai_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Perplexity => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "perplexity",
            config.perplexity_base_url.as_deref().unwrap_or("https://api.perplexity.ai"),
            "/chat/completions",
            config.perplexity_model.as_deref().unwrap_or("sonar-pro"),
            config.perplexity_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Cohere => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "cohere",
            config.cohere_base_url.as_deref().unwrap_or("https://api.cohere.com/compatibility"),
            "/v1/chat/completions",
            config.cohere_model.as_deref().unwrap_or("command-r-plus"),
            config.cohere_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Fireworks => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "fireworks",
            config.fireworks_base_url.as_deref().unwrap_or("https://api.fireworks.ai/inference"),
            "/v1/chat/completions",
            config.fireworks_model.as_deref()
                .unwrap_or("accounts/fireworks/models/llama-v3p3-70b-instruct"),
            config.fireworks_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),

        ProviderKind::Cerebras => Arc::new(openai::OpenAiAdapter::new_for_provider(
            "cerebras",
            config.cerebras_base_url.as_deref().unwrap_or("https://api.cerebras.ai"),
            "/v1/chat/completions",
            config.cerebras_model.as_deref().unwrap_or("llama3.1-70b"),
            config.cerebras_api_key.as_deref().unwrap_or(""),
            Arc::clone(&registry),
        )),
    }
}
