use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

pub use rusty_ollama::{ChatMessage, StreamEvent, ToolCallFunction, ToolCallResponse};

#[derive(Clone)]
pub struct OllamaClient {
    pub base_url: String,
    pub model: String,
    pub client: rusty_ollama::Client,
}

impl OllamaClient {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            model: model.to_string(),
            client: rusty_ollama::Client::new(base_url, model),
        }
    }

    /// Start a streaming chat and return a channel of StreamEvents.
    pub fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<Value>>,
    ) -> mpsc::Receiver<StreamEvent> {
        self.client.chat_stream(messages, tools)
    }

    /// Non-streaming chat.
    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<Value>>,
    ) -> Result<ChatMessage, String> {
        self.client.chat(messages, tools).await
    }
}

/// Conversation message used internally by the orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

impl Message {
    /// Convert to the format expected by rusty_ollama.
    pub fn to_chat_message(&self) -> ChatMessage {
        let tool_calls = self.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| ToolCallResponse {
                    id: tc.id.clone(),
                    function: ToolCallFunction {
                        name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    },
                })
                .collect()
        });

        ChatMessage {
            role: self.role.clone(),
            content: self.content.clone(),
            tool_calls,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

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
