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

/// Adapter for any OpenAI-compatible chat completion API.
///
/// Works with: OpenAI, Google Gemini (v1beta/openai endpoint), Mistral,
/// Groq, Together AI, DeepSeek, xAI, Perplexity, Cohere (compatibility/v1),
/// Fireworks, Cerebras — any provider that speaks the OpenAI SSE/REST protocol.
///
/// The `provider_id` field controls which capability defaults are used in the
/// registry lookup and which label appears in `OrchestratorEvent::ProviderInfo`.
pub struct OpenAiAdapter {
    provider_id: String,
    base_url: String,
    /// Path appended to base_url for chat completions.
    /// Defaults to "/v1/chat/completions"; set "/chat/completions" for Perplexity.
    chat_path: String,
    model: String,
    api_key: String,
    http: HttpClient,
    registry: Arc<CapabilityRegistry>,
}

impl OpenAiAdapter {
    /// Create an OpenAI-branded adapter (provider_id = "openai").
    pub fn new(base_url: &str, model: &str, api_key: &str, registry: Arc<CapabilityRegistry>) -> Self {
        Self::new_for_provider("openai", base_url, "/v1/chat/completions", model, api_key, registry)
    }

    /// Create an adapter for any OpenAI-compatible provider.
    ///
    /// - `provider_id`: stable id used in registry lookup and event labels
    ///   (e.g. `"groq"`, `"gemini"`, `"mistral"`)
    /// - `chat_path`: URL path for the completions endpoint;
    ///   `"/v1/chat/completions"` works for most providers;
    ///   Perplexity uses `"/chat/completions"` (no `/v1/` prefix)
    pub fn new_for_provider(
        provider_id: &str,
        base_url: &str,
        chat_path: &str,
        model: &str,
        api_key: &str,
        registry: Arc<CapabilityRegistry>,
    ) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            chat_path: chat_path.to_string(),
            model: model.to_string(),
            api_key: api_key.to_string(),
            http: HttpClient::builder()
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .expect("Failed to build HTTP client for OpenAiAdapter"),
            registry,
        }
    }

    fn provider_meta(&self) -> ProviderMetadata {
        ProviderMetadata {
            provider_id: self.provider_id.clone(),
            model_id: self.model.clone(),
            raw_provider_tag: Some(self.model.clone()),
        }
    }

    fn completions_url(&self) -> String {
        format!("{}{}", self.base_url, self.chat_path)
    }

    /// Select the correct provider default for the registry lookup.
    fn provider_default(&self) -> ProviderCapabilities {
        match self.provider_id.as_str() {
            "gemini" => ProviderCapabilities::gemini_default(),
            "mistral" => ProviderCapabilities::mistral_default(),
            "groq" => ProviderCapabilities::groq_default(),
            "together" => ProviderCapabilities::together_default(),
            "deepseek" => ProviderCapabilities::deepseek_default(),
            "xai" => ProviderCapabilities::xai_default(),
            "perplexity" => ProviderCapabilities::perplexity_default(),
            "cohere" => ProviderCapabilities::cohere_default(),
            "fireworks" => ProviderCapabilities::fireworks_default(),
            "cerebras" => ProviderCapabilities::cerebras_default(),
            _ => ProviderCapabilities::openai_default(),
        }
    }

    /// Convert canonical AgentMessages to the OpenAI messages array format.
    fn to_openai_messages(messages: &[AgentMessage]) -> Vec<Value> {
        messages
            .iter()
            .map(|m| match &m.role {
                MessageRole::Tool => serde_json::json!({
                    "role": "tool",
                    "content": m.content,
                    "tool_call_id": m.tool_call_id.as_deref().unwrap_or(""),
                }),
                MessageRole::Assistant if m.tool_calls.is_some() => {
                    let tool_calls: Vec<Value> = m
                        .tool_calls
                        .as_ref()
                        .unwrap()
                        .iter()
                        .map(|tc| {
                            serde_json::json!({
                                "id": tc.id,
                                "type": "function",
                                "function": {
                                    "name": tc.name,
                                    "arguments": tc.raw_arguments,
                                }
                            })
                        })
                        .collect();
                    serde_json::json!({
                        "role": "assistant",
                        "content": m.content,
                        "tool_calls": tool_calls,
                    })
                }
                _ => serde_json::json!({
                    "role": m.role.as_str(),
                    "content": m.content,
                }),
            })
            .collect()
    }
}

// ─── SSE chunk types ──────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatCompletionChunk {
    choices: Vec<ChunkChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct ChunkChoice {
    delta: ChunkDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChunkDelta {
    content: Option<String>,
    tool_calls: Option<Vec<ToolCallDelta>>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    index: u32,
    id: Option<String>,
    function: Option<FunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct FunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAiUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
}

// ─── Non-streaming response types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ChatCompletion {
    choices: Vec<CompletionChoice>,
    usage: Option<OpenAiUsage>,
}

#[derive(Debug, Deserialize)]
struct CompletionChoice {
    message: CompletionMessage,
}

#[derive(Debug, Deserialize)]
struct CompletionMessage {
    content: Option<String>,
    tool_calls: Option<Vec<CompletionToolCall>>,
}

#[derive(Debug, Deserialize)]
struct CompletionToolCall {
    id: String,
    function: CompletionFunction,
}

#[derive(Debug, Deserialize)]
struct CompletionFunction {
    name: String,
    arguments: String,
}

// ─────────────────────────────────────────────────────────────────────────────

#[async_trait]
impl ProviderAdapter for OpenAiAdapter {
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    fn model_id(&self) -> &str {
        &self.model
    }

    fn capabilities(&self) -> ProviderCapabilities {
        self.registry.lookup(&self.provider_id, &self.model, self.provider_default())
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

        let url = self.completions_url();
        let openai_messages = Self::to_openai_messages(&messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": openai_messages,
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        if let Some(ref tool_list) = tools {
            if !tool_list.is_empty() {
                body["tools"] = serde_json::json!(tool_list);
                body["tool_choice"] = serde_json::json!("auto");
            }
        }

        let http = self.http.clone();
        let api_key = self.api_key.clone();

        tokio::spawn(async move {
            let response = match http.post(&url).bearer_auth(&api_key).json(&body).send().await {
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
                        message: format!("Provider error {} : {}", status, body_text),
                        retryable: status.is_server_error(),
                    })
                    .await;
                return;
            }

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();
            // index -> (id, name, accumulated_args)
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

                    if line.is_empty() {
                        continue;
                    }

                    if line == "data: [DONE]" {
                        for (_, (id, name, args)) in &tool_accum {
                            let parsed = serde_json::from_str::<Value>(args)
                                .unwrap_or_else(|_| serde_json::json!({}));
                            let payload = ToolCallPayload::with_id(
                                id.clone(), name.clone(), parsed, args.clone(),
                            );
                            let _ = tx.send(StreamEvent::ToolCallDone { payload }).await;
                        }
                        let _ = tx.send(StreamEvent::Finish { stop_reason: StopReason::EndTurn }).await;
                        return;
                    }

                    let data = match line.strip_prefix("data: ") {
                        Some(d) => d,
                        None => continue,
                    };

                    let chunk: ChatCompletionChunk = match serde_json::from_str(data) {
                        Ok(c) => c,
                        Err(_) => continue,
                    };

                    for choice in &chunk.choices {
                        if let Some(content) = &choice.delta.content {
                            if !content.is_empty() {
                                let _ = tx
                                    .send(StreamEvent::TextDelta { content: content.clone() })
                                    .await;
                            }
                        }

                        if let Some(tc_deltas) = &choice.delta.tool_calls {
                            for tc_delta in tc_deltas {
                                let entry = tool_accum
                                    .entry(tc_delta.index)
                                    .or_insert_with(|| (String::new(), String::new(), String::new()));

                                if let Some(id) = &tc_delta.id {
                                    if entry.0.is_empty() {
                                        entry.0 = id.clone();
                                    }
                                }

                                if let Some(func) = &tc_delta.function {
                                    if let Some(name) = &func.name {
                                        if entry.1.is_empty() {
                                            entry.1 = name.clone();
                                            let _ = tx
                                                .send(StreamEvent::ToolCallStart {
                                                    call_id: entry.0.clone(),
                                                    name: name.clone(),
                                                })
                                                .await;
                                        }
                                    }
                                    if let Some(args_delta) = &func.arguments {
                                        let _ = tx
                                            .send(StreamEvent::ToolCallDelta {
                                                call_id: entry.0.clone(),
                                                arguments_delta: args_delta.clone(),
                                            })
                                            .await;
                                        entry.2.push_str(args_delta);
                                    }
                                }
                            }
                        }

                        if let Some(reason) = &choice.finish_reason {
                            let stop_reason = match reason.as_str() {
                                "tool_calls" => {
                                    for (_, (id, name, args)) in &tool_accum {
                                        let parsed = serde_json::from_str::<Value>(args)
                                            .unwrap_or_else(|_| serde_json::json!({}));
                                        let payload = ToolCallPayload::with_id(
                                            id.clone(), name.clone(), parsed, args.clone(),
                                        );
                                        let _ = tx.send(StreamEvent::ToolCallDone { payload }).await;
                                    }
                                    tool_accum.clear();
                                    StopReason::ToolUse
                                }
                                "length" => StopReason::MaxTokens,
                                _ => StopReason::EndTurn,
                            };
                            let _ = tx.send(StreamEvent::Finish { stop_reason }).await;
                        }
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
        let url = self.completions_url();
        let openai_messages = Self::to_openai_messages(&messages);

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": openai_messages,
            "stream": false,
        });

        if let Some(ref tool_list) = tools {
            if !tool_list.is_empty() {
                body["tools"] = serde_json::json!(tool_list);
                body["tool_choice"] = serde_json::json!("auto");
            }
        }

        let response = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("HTTP error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response.text().await.unwrap_or_default();
            return Err(format!("Provider error {} : {}", status, body_text));
        }

        let completion: ChatCompletion = response
            .json()
            .await
            .map_err(|e| format!("JSON parse error: {}", e))?;

        let choice = completion
            .choices
            .into_iter()
            .next()
            .ok_or_else(|| "No choices in provider response".to_string())?;

        let mut parts = Vec::new();
        if let Some(content) = choice.message.content {
            if !content.is_empty() {
                parts.push(MessagePart::Text { content });
            }
        }
        if let Some(tcs) = choice.message.tool_calls {
            for tc in tcs {
                let parsed = serde_json::from_str::<Value>(&tc.function.arguments)
                    .unwrap_or_else(|_| serde_json::json!({}));
                parts.push(MessagePart::ToolCall(ToolCallPayload::with_id(
                    tc.id,
                    tc.function.name,
                    parsed,
                    tc.function.arguments,
                )));
            }
        }

        let mut turn = AgentTurn::new(parts, self.provider_meta());
        if let Some(usage) = completion.usage {
            turn = turn.with_usage(UsageInfo {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
            });
        }

        Ok(turn)
    }
}
