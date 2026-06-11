use async_trait::async_trait;
use barq_ir::{
    AgentMessage, AgentTurn, MessagePart, MessageRole, ProviderMetadata, StopReason, StreamEvent,
    ToolCallPayload, UsageInfo,
};
use futures::StreamExt;
use reqwest::Client as HttpClient;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

use super::{ProviderAdapter, ProviderCapabilities, TrustTier};
use super::registry::CapabilityRegistry;

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com";
const ANTHROPIC_MESSAGES_PATH: &str = "/v1/messages";
const ANTHROPIC_API_VERSION: &str = "2023-06-01";

/// Adapter for the Anthropic Messages API (claude-*).
///
/// Notable differences from the OpenAI wire format:
/// - Auth header: `x-api-key` (not `Authorization: Bearer`)
/// - System messages extracted to a top-level `system` field; no system objects
///   may appear in the `messages` array
/// - Consecutive `tool_result` messages must be coalesced into one `user` message
///   containing a `content` array of `tool_result` blocks
/// - Tool schemas use `input_schema` (JSON Schema) instead of OpenAI's `parameters`
/// - SSE events: `content_block_start`, `content_block_delta`, `content_block_stop`,
///   `message_delta`, `message_stop` (not `data: {"choices": [...]}`)
pub struct AnthropicAdapter {
    base_url: String,
    model: String,
    api_key: String,
    http: HttpClient,
    registry: Arc<CapabilityRegistry>,
}

impl AnthropicAdapter {
    pub fn new(model: &str, api_key: &str, registry: Arc<CapabilityRegistry>) -> Self {
        Self::with_base_url(ANTHROPIC_API_BASE, model, api_key, registry)
    }

    pub fn with_base_url(
        base_url: &str,
        model: &str,
        api_key: &str,
        registry: Arc<CapabilityRegistry>,
    ) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            http: HttpClient::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to build HTTP client for AnthropicAdapter"),
            registry,
        }
    }

    fn provider_meta(&self) -> ProviderMetadata {
        ProviderMetadata {
            provider_id: "anthropic".to_string(),
            model_id: self.model.clone(),
            raw_provider_tag: Some(self.model.clone()),
        }
    }

    fn messages_url(&self) -> String {
        format!("{}{}", self.base_url, ANTHROPIC_MESSAGES_PATH)
    }

    /// Convert canonical `AgentMessage` list to the Anthropic `messages` array.
    ///
    /// Rules:
    /// - System messages are extracted; they must not appear in the array.
    /// - Consecutive tool results are merged into one `user` message with
    ///   `content: [{ type: "tool_result", ... }, ...]`.
    /// - Assistant messages with tool calls become content blocks.
    fn to_anthropic_messages(messages: &[AgentMessage]) -> (Option<String>, Vec<Value>) {
        let mut system_text: Option<String> = None;
        let mut result: Vec<Value> = Vec::new();
        // Accumulate pending tool_result blocks to merge
        let mut pending_tool_results: Vec<Value> = Vec::new();

        for msg in messages {
            match &msg.role {
                MessageRole::System => {
                    system_text = Some(msg.content.clone());
                    continue;
                }
                MessageRole::Tool => {
                    pending_tool_results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": msg.tool_call_id.as_deref().unwrap_or(""),
                        "content": msg.content,
                    }));
                    continue;
                }
                _ => {
                    if !pending_tool_results.is_empty() {
                        result.push(serde_json::json!({
                            "role": "user",
                            "content": pending_tool_results,
                        }));
                        pending_tool_results = Vec::new();
                    }
                }
            }

            match &msg.role {
                MessageRole::User => {
                    result.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content,
                    }));
                }
                MessageRole::Assistant => {
                    if let Some(tool_calls) = &msg.tool_calls {
                        let mut content_blocks: Vec<Value> = Vec::new();
                        if !msg.content.is_empty() {
                            content_blocks.push(serde_json::json!({
                                "type": "text",
                                "text": msg.content,
                            }));
                        }
                        for tc in tool_calls {
                            let input: Value = serde_json::from_str(&tc.raw_arguments)
                                .unwrap_or_else(|_| serde_json::json!({}));
                            content_blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": tc.id,
                                "name": tc.name,
                                "input": input,
                            }));
                        }
                        result.push(serde_json::json!({
                            "role": "assistant",
                            "content": content_blocks,
                        }));
                    } else {
                        result.push(serde_json::json!({
                            "role": "assistant",
                            "content": msg.content,
                        }));
                    }
                }
                _ => {}
            }
        }

        if !pending_tool_results.is_empty() {
            result.push(serde_json::json!({
                "role": "user",
                "content": pending_tool_results,
            }));
        }

        (system_text, result)
    }

    /// Convert OpenAI-style tool schemas to Anthropic format.
    ///
    /// OpenAI:   `{ type: "function", function: { name, description, parameters } }`
    /// Anthropic: `{ name, description, input_schema }`
    fn to_anthropic_tools(tools: &[Value]) -> Vec<Value> {
        tools
            .iter()
            .filter_map(|t| {
                let func = t.get("function")?;
                let name = func.get("name")?.as_str()?.to_string();
                let description = func
                    .get("description")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_schema = func
                    .get("parameters")
                    .cloned()
                    .unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} }));
                Some(serde_json::json!({
                    "name": name,
                    "description": description,
                    "input_schema": input_schema,
                }))
            })
            .collect()
    }
}

// ─── SSE event types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum AnthropicEvent {
    ContentBlockStart {
        index: u32,
        content_block: ContentBlock,
    },
    ContentBlockDelta {
        index: u32,
        delta: ContentBlockDeltaPayload,
    },
    ContentBlockStop {
        index: u32,
    },
    MessageDelta {
        delta: MessageDeltaPayload,
        usage: Option<AnthropicUsage>,
    },
    MessageStop,
    MessageStart {
        message: MessageStartPayload,
    },
    Ping,
    Error {
        error: AnthropicError,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ContentBlockDeltaPayload {
    TextDelta { text: String },
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Deserialize)]
struct MessageDeltaPayload {
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageStartPayload {
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct AnthropicError {
    message: String,
}

// ─── Non-streaming response types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsageNonStream>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum AnthropicContent {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
}

#[derive(Debug, Deserialize)]
struct AnthropicUsageNonStream {
    input_tokens: u32,
    output_tokens: u32,
}

// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ProviderAdapter for AnthropicAdapter {
    fn provider_id(&self) -> &str {
        "anthropic"
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.registry.lookup("anthropic", &self.model, ProviderCapabilities::anthropic_default())
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

        let url = self.messages_url();
        let (system, anthropic_messages) = Self::to_anthropic_messages(&messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 8192,
            "messages": anthropic_messages,
            "stream": true,
        });

        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }

        if let Some(ref tool_list) = tools {
            if !tool_list.is_empty() {
                body["tools"] = serde_json::json!(Self::to_anthropic_tools(tool_list));
                body["tool_choice"] = serde_json::json!({ "type": "auto" });
            }
        }

        let http = self.http.clone();
        let api_key = self.api_key.clone();

        tokio::spawn(async move {
            let response = match http
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", ANTHROPIC_API_VERSION)
                .header("content-type", "application/json")
                .json(&body)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(StreamEvent::Error {
                            message: format!("HTTP error: {}", e),
                            retryable: true,
                        })
                        .await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body_text = response.text().await.unwrap_or_default();
                let _ = tx
                    .send(StreamEvent::Error {
                        message: format!("Anthropic error {} : {}", status, body_text),
                        retryable: status.is_server_error(),
                    })
                    .await;
                return;
            }

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            // index → (tool_use_id, tool_name, accumulated_json)
            let mut tool_accum: HashMap<u32, (String, String, String)> = HashMap::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx
                            .send(StreamEvent::Error {
                                message: format!("Stream error: {}", e),
                                retryable: false,
                            })
                            .await;
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(nl_pos) = buffer.find('\n') {
                    let line = buffer[..nl_pos].trim().to_string();
                    buffer = buffer[nl_pos + 1..].to_string();

                    if line.is_empty() || !line.starts_with("data: ") {
                        continue;
                    }

                    let data = &line["data: ".len()..];
                    let event: AnthropicEvent = match serde_json::from_str(data) {
                        Ok(e) => e,
                        Err(_) => continue,
                    };

                    match event {
                        AnthropicEvent::ContentBlockStart { index, content_block } => {
                            match content_block {
                                ContentBlock::ToolUse { id, name } => {
                                    tool_accum.insert(index, (id.clone(), name.clone(), String::new()));
                                    let _ = tx
                                        .send(StreamEvent::ToolCallStart {
                                            call_id: id,
                                            name,
                                        })
                                        .await;
                                }
                                ContentBlock::Text { .. } => {}
                            }
                        }

                        AnthropicEvent::ContentBlockDelta { index, delta } => {
                            match delta {
                                ContentBlockDeltaPayload::TextDelta { text } => {
                                    let _ = tx
                                        .send(StreamEvent::TextDelta { content: text })
                                        .await;
                                }
                                ContentBlockDeltaPayload::InputJsonDelta { partial_json } => {
                                    if let Some(entry) = tool_accum.get_mut(&index) {
                                        let _ = tx
                                            .send(StreamEvent::ToolCallDelta {
                                                call_id: entry.0.clone(),
                                                arguments_delta: partial_json.clone(),
                                            })
                                            .await;
                                        entry.2.push_str(&partial_json);
                                    }
                                }
                            }
                        }

                        AnthropicEvent::ContentBlockStop { index } => {
                            if let Some((id, name, args)) = tool_accum.remove(&index) {
                                let parsed = serde_json::from_str::<Value>(&args)
                                    .unwrap_or_else(|_| serde_json::json!({}));
                                let payload =
                                    ToolCallPayload::with_id(id, name, parsed, args);
                                let _ = tx.send(StreamEvent::ToolCallDone { payload }).await;
                            }
                        }

                        AnthropicEvent::MessageDelta { delta, .. } => {
                            if let Some(reason) = delta.stop_reason {
                                let stop_reason = match reason.as_str() {
                                    "tool_use" => StopReason::ToolUse,
                                    "max_tokens" => StopReason::MaxTokens,
                                    _ => StopReason::EndTurn,
                                };
                                let _ = tx.send(StreamEvent::Finish { stop_reason }).await;
                            }
                        }

                        AnthropicEvent::MessageStop => {
                            let _ = tx
                                .send(StreamEvent::Finish { stop_reason: StopReason::EndTurn })
                                .await;
                        }

                        AnthropicEvent::Error { error } => {
                            let _ = tx
                                .send(StreamEvent::Error {
                                    message: format!("Anthropic stream error: {}", error.message),
                                    retryable: false,
                                })
                                .await;
                            return;
                        }

                        AnthropicEvent::Ping | AnthropicEvent::MessageStart { .. } => {}
                    }
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
        let url = self.messages_url();
        let (system, anthropic_messages) = Self::to_anthropic_messages(&messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "max_tokens": 8192,
            "messages": anthropic_messages,
        });

        if let Some(sys) = system {
            body["system"] = serde_json::json!(sys);
        }

        if let Some(ref tool_list) = tools {
            if !tool_list.is_empty() {
                body["tools"] = serde_json::json!(Self::to_anthropic_tools(tool_list));
                body["tool_choice"] = serde_json::json!({ "type": "auto" });
            }
        }

        let response = self
            .http
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", ANTHROPIC_API_VERSION)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(format!("Anthropic error {} : {}", status, body_text));
        }

        let resp: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let mut parts = Vec::new();
        for block in resp.content {
            match block {
                AnthropicContent::Text { text } => {
                    if !text.is_empty() {
                        parts.push(MessagePart::Text { content: text });
                    }
                }
                AnthropicContent::ToolUse { id, name, input } => {
                    let raw_args = serde_json::to_string(&input).unwrap_or_default();
                    parts.push(MessagePart::ToolCall(ToolCallPayload::with_id(
                        id, name, input, raw_args,
                    )));
                }
            }
        }

        let mut turn = AgentTurn::new(parts, self.provider_meta());
        if let Some(usage) = resp.usage {
            turn = turn.with_usage(UsageInfo {
                prompt_tokens: usage.input_tokens,
                completion_tokens: usage.output_tokens,
            });
        }

        Ok(turn)
    }
}
