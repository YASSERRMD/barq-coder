use serde::{Deserialize, Serialize};
use crate::parts::{MessagePart, ToolCallPayload};
use crate::version::SchemaVersion;

/// Which provider and model produced this turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderMetadata {
    /// Canonical provider identifier, e.g. "ollama", "openai", "anthropic".
    pub provider_id: String,
    /// The model identifier as the provider knows it, e.g. "gpt-4o".
    pub model_id: String,
    /// Raw provider-specific tag kept for debugging, e.g. "gpt-4o-2024-08-06".
    pub raw_provider_tag: Option<String>,
}

/// Token usage reported by the provider for a single turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
}

impl UsageInfo {
    pub fn total_tokens(&self) -> u32 {
        self.prompt_tokens + self.completion_tokens
    }
}

/// A complete agent turn in the canonical IR.
///
/// The executor builds its conversation history from AgentTurn values rather
/// than from any provider-native message type. This is the output of a
/// ProviderAdapter::chat() call and the accumulation of streaming events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTurn {
    pub schema_version: SchemaVersion,
    /// Ordered parts that make up the assistant's response.
    pub parts: Vec<MessagePart>,
    pub provider: ProviderMetadata,
    pub usage: Option<UsageInfo>,
}

impl AgentTurn {
    pub fn new(parts: Vec<MessagePart>, provider: ProviderMetadata) -> Self {
        Self {
            schema_version: SchemaVersion::CURRENT,
            parts,
            provider,
            usage: None,
        }
    }

    pub fn with_usage(mut self, usage: UsageInfo) -> Self {
        self.usage = Some(usage);
        self
    }

    /// Extract concatenated text content from all Text parts.
    pub fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|p| match p {
                MessagePart::Text { content } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Extract all tool calls from this turn.
    pub fn tool_calls(&self) -> Vec<&ToolCallPayload> {
        self.parts
            .iter()
            .filter_map(|p| match p {
                MessagePart::ToolCall(tc) => Some(tc),
                _ => None,
            })
            .collect()
    }

    /// True if the turn contains at least one tool call.
    pub fn has_tool_calls(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p, MessagePart::ToolCall(_)))
    }
}
