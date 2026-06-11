use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single part of an agent turn.
///
/// The discriminated union ensures every adapter must produce structured data —
/// never raw text with embedded JSON that the executor would need to parse.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessagePart {
    Text { content: String },
    Reasoning { content: String },
    ToolCall(ToolCallPayload),
    ToolResult(ToolResultPayload),
    File { path: String, content: String },
    Error { message: String, code: Option<String> },
}

/// Canonical tool call emitted by a model.
///
/// `id` is stable and first-class: it correlates tool results back to their
/// calls, which is especially important during streaming where results arrive
/// out of order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallPayload {
    /// Stable UUID assigned at call time by the adapter.
    pub id: String,
    /// Tool name as registered in the ToolRegistry.
    pub name: String,
    /// Arguments parsed from the model output.
    pub arguments: Value,
    /// Raw argument string exactly as the model produced it, kept for debugging
    /// and for adapters that need to forward the original string.
    pub raw_arguments: String,
}

impl ToolCallPayload {
    /// Create a new tool call with a freshly generated UUID.
    pub fn new(name: impl Into<String>, arguments: Value, raw_arguments: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            arguments,
            raw_arguments: raw_arguments.into(),
        }
    }

    /// Create with a pre-assigned id (e.g., when the provider supplies one).
    pub fn with_id(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: Value,
        raw_arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            arguments,
            raw_arguments: raw_arguments.into(),
        }
    }
}

/// Canonical tool result, keyed by the tool-call id it answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultPayload {
    /// References ToolCallPayload::id.
    pub call_id: String,
    pub content: Value,
    pub is_error: bool,
}

impl ToolResultPayload {
    pub fn ok(call_id: impl Into<String>, content: Value) -> Self {
        Self {
            call_id: call_id.into(),
            content,
            is_error: false,
        }
    }

    pub fn error(call_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            call_id: call_id.into(),
            content: serde_json::json!({ "error": message.into() }),
            is_error: true,
        }
    }
}
