use async_trait::async_trait;
use barq_ir::{
    AgentMessage, AgentTurn, MessagePart, MessageRole, ProviderMetadata, StopReason, StreamEvent,
    ToolCallPayload, UsageInfo,
};
use serde_json::Value;
use tokio::sync::mpsc;

use super::{ProviderAdapter, ProviderCapabilities, TrustTier};

/// Adapter that wraps the `rusty_ollama` HTTP client and converts its output
/// into the canonical IR. All Ollama-specific logic lives here.
pub struct OllamaAdapter {
    client: rusty_ollama::Client,
    model: String,
}

impl OllamaAdapter {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            client: rusty_ollama::Client::new(base_url, model),
            model: model.to_string(),
        }
    }

    fn provider_meta(&self) -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "ollama".to_string(),
            model_id: self.model.clone(),
            raw_provider_tag: None,
        }
    }

    /// Convert canonical AgentMessages to rusty_ollama ChatMessages.
    fn to_ollama_messages(messages: &[AgentMessage]) -> Vec<rusty_ollama::ChatMessage> {
        messages
            .iter()
            .map(|m| rusty_ollama::ChatMessage {
                role: m.role.as_str().to_string(),
                content: m.content.clone(),
                tool_calls: m.tool_calls.as_ref().map(|tcs| {
                    tcs.iter()
                        .map(|tc| rusty_ollama::ToolCallResponse {
                            id: tc.id.clone(),
                            function: rusty_ollama::ToolCallFunction {
                                name: tc.name.clone(),
                                arguments: tc.arguments.clone(),
                            },
                        })
                        .collect()
                }),
            })
            .collect()
    }

    fn assign_id(raw_id: &str) -> String {
        if raw_id.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            raw_id.to_string()
        }
    }
}

#[async_trait]
impl ProviderAdapter for OllamaAdapter {
    fn provider_id(&self) -> &str {
        "ollama"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::ollama_default()
    }

    fn trust_tier(&self) -> TrustTier {
        TrustTier::Full
    }

    fn chat_stream(
        &self,
        messages: Vec<AgentMessage>,
        tools: Option<Vec<Value>>,
    ) -> mpsc::Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel(256);
        let ollama_messages = Self::to_ollama_messages(&messages);
        let mut raw_rx = self.client.chat_stream(ollama_messages, tools);

        tokio::spawn(async move {
            while let Some(raw_event) = raw_rx.recv().await {
                let canonical = match raw_event {
                    rusty_ollama::StreamEvent::Token(text) => {
                        StreamEvent::TextDelta { content: text }
                    }
                    rusty_ollama::StreamEvent::ToolCall(tc) => {
                        let raw = tc.function.arguments.to_string();
                        let payload = ToolCallPayload::with_id(
                            Self::assign_id(&tc.id),
                            tc.function.name,
                            tc.function.arguments,
                            raw,
                        );
                        StreamEvent::ToolCallDone { payload }
                    }
                    rusty_ollama::StreamEvent::Done => StreamEvent::Finish {
                        stop_reason: StopReason::EndTurn,
                    },
                    rusty_ollama::StreamEvent::Error(e) => StreamEvent::Error {
                        message: e,
                        retryable: false,
                    },
                };
                if tx.send(canonical).await.is_err() {
                    break;
                }
            }
        });

        rx
    }

    async fn chat(
        &self,
        messages: Vec<AgentMessage>,
        tools: Option<Vec<Value>>,
    ) -> Result<AgentTurn, String> {
        let ollama_messages = Self::to_ollama_messages(&messages);
        let raw = self.client.chat(ollama_messages, tools).await?;

        let mut parts = Vec::new();
        if !raw.content.is_empty() {
            parts.push(MessagePart::Text {
                content: raw.content.clone(),
            });
        }
        if let Some(tcs) = raw.tool_calls {
            for tc in tcs {
                let raw_args = tc.function.arguments.to_string();
                parts.push(MessagePart::ToolCall(ToolCallPayload::with_id(
                    Self::assign_id(&tc.id),
                    tc.function.name,
                    tc.function.arguments,
                    raw_args,
                )));
            }
        }

        Ok(AgentTurn::new(parts, self.provider_meta()))
    }
}
