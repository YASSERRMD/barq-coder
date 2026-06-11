use serde::{Deserialize, Serialize};
use crate::parts::{ToolCallPayload, ToolResultPayload};

/// Why a model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Error,
}

/// Canonical stream-event vocabulary.
///
/// All model-specific wire formats (Ollama NDJSON, OpenAI SSE, Anthropic SSE)
/// are normalised to one of these variants by the adapter. The executor never
/// sees the raw wire format, only these events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StreamEvent {
    /// A text token from the model's generation.
    TextDelta { content: String },

    /// The model is beginning a tool call; name is known, arguments still streaming.
    ToolCallStart { call_id: String, name: String },

    /// An incremental argument delta for an in-progress tool call.
    ToolCallDelta { call_id: String, arguments_delta: String },

    /// The tool call is fully assembled — arguments are parsed and stable.
    ToolCallDone { payload: ToolCallPayload },

    /// A tool result ready to be fed back in the next turn.
    ToolResult { payload: ToolResultPayload },

    /// One reasoning step finished (for chain-of-thought providers).
    StepFinish { stop_reason: StopReason },

    /// The entire generation is complete.
    Finish { stop_reason: StopReason },

    /// A recoverable or fatal error from the provider.
    Error { message: String, retryable: bool },
}
