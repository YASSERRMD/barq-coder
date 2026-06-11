use async_trait::async_trait;
use barq_ir::{AgentMessage, AgentTurn, StreamEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::mpsc;

pub mod capabilities;
pub mod ollama;
pub mod openai;
pub mod trust_tiers;

pub use capabilities::ProviderCapabilities;
pub use trust_tiers::TrustTier;

/// Discriminant used in Config to select which adapter to build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderKind {
    Ollama,
    OpenAi,
}

impl Default for ProviderKind {
    fn default() -> Self {
        Self::Ollama
    }
}

impl std::fmt::Display for ProviderKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ollama => write!(f, "ollama"),
            Self::OpenAi => write!(f, "openai"),
        }
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
/// This is the single place that knows about all provider kinds.
pub fn build_provider(config: &crate::config::Config) -> Arc<dyn ProviderAdapter> {
    match config.provider {
        ProviderKind::Ollama => Arc::new(ollama::OllamaAdapter::new(
            &config.ollama_base_url,
            &config.ollama_model,
        )),
        ProviderKind::OpenAi => Arc::new(openai::OpenAiAdapter::new(
            config
                .openai_base_url
                .as_deref()
                .unwrap_or("https://api.openai.com"),
            config
                .openai_model
                .as_deref()
                .unwrap_or("gpt-4o"),
            config.openai_api_key.as_deref().unwrap_or(""),
        )),
    }
}
