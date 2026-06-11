// Re-export the canonical IR types so the rest of the codebase can import them
// from one place. Nothing in the executor should import from `barq_ir` directly
// or from `rusty_ollama` — everything flows through the ProviderAdapter.
pub use barq_ir::{
    AgentMessage, AgentTurn, MessagePart, MessageRole, ProviderMetadata, SchemaVersion,
    StopReason, StreamEvent, ToolCallPayload, ToolResultPayload, UsageInfo,
};

// Type alias kept for backward compat within the context and session modules.
pub type Message = barq_ir::AgentMessage;
pub type ToolCall = barq_ir::ToolCallPayload;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub reasoning: String,
    #[serde(default)]
    pub tool_calls: Vec<ToolCall>,
    pub final_answer: Option<String>,
}

pub fn parse_response(raw: &str) -> AgentResponse {
    match serde_json::from_str(raw) {
        Ok(res) => res,
        Err(_) => AgentResponse {
            reasoning: raw.to_string(),
            tool_calls: vec![],
            final_answer: None,
        },
    }
}
