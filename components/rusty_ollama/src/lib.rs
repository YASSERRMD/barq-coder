use futures::StreamExt;
use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct Client {
    pub base_url: String,
    pub model: String,
    pub http: HttpClient,
}

#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCallResponse>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResponse {
    #[serde(default)]
    pub id: String,
    pub function: ToolCallFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallFunction {
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Deserialize)]
pub struct ChatStreamChunk {
    #[serde(default)]
    pub message: Option<ChatMessage>,
    #[serde(default)]
    pub done: bool,
    #[serde(default)]
    pub done_reason: Option<String>,
}

/// Events emitted by the streaming chat.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// A partial text token from the assistant.
    Token(String),
    /// The assistant wants to call a tool.
    ToolCall(ToolCallResponse),
    /// The stream has finished.
    Done,
    /// An error occurred.
    Error(String),
}

impl Client {
    pub fn new(base_url: &str, model: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
            http: HttpClient::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    /// Send a streaming chat request to Ollama.
    /// Returns a channel receiver that yields `StreamEvent` values.
    pub fn chat_stream(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<Value>>,
    ) -> mpsc::Receiver<StreamEvent> {
        let (tx, rx) = mpsc::channel(256);

        let url = format!("{}/api/chat", self.base_url);
        let request_body = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
            tools,
            format: None,
        };

        let http = self.http.clone();

        tokio::spawn(async move {
            let response = match http.post(&url).json(&request_body).send().await {
                Ok(resp) => resp,
                Err(e) => {
                    let _ = tx.send(StreamEvent::Error(format!("HTTP request failed: {}", e))).await;
                    return;
                }
            };

            if !response.status().is_success() {
                let status = response.status();
                let body = response.text().await.unwrap_or_default();
                let _ = tx
                    .send(StreamEvent::Error(format!(
                        "Ollama returned {}: {}",
                        status, body
                    )))
                    .await;
                return;
            }

            let mut stream = response.bytes_stream();
            let mut buffer = String::new();

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(format!("Stream error: {}", e))).await;
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                // Ollama sends NDJSON — one JSON object per line
                while let Some(newline_pos) = buffer.find('\n') {
                    let line = buffer[..newline_pos].trim().to_string();
                    buffer = buffer[newline_pos + 1..].to_string();

                    if line.is_empty() {
                        continue;
                    }

                    let parsed: ChatStreamChunk = match serde_json::from_str(&line) {
                        Ok(p) => p,
                        Err(e) => {
                            let _ = tx
                                .send(StreamEvent::Error(format!("JSON parse error: {} in: {}", e, line)))
                                .await;
                            continue;
                        }
                    };

                    if let Some(msg) = &parsed.message {
                        // Check for tool calls
                        if let Some(tool_calls) = &msg.tool_calls {
                            for tc in tool_calls {
                                let _ = tx.send(StreamEvent::ToolCall(tc.clone())).await;
                            }
                        }

                        // Emit text tokens
                        if !msg.content.is_empty() {
                            let _ = tx.send(StreamEvent::Token(msg.content.clone())).await;
                        }
                    }

                    if parsed.done {
                        let _ = tx.send(StreamEvent::Done).await;
                        return;
                    }
                }
            }

            // If we exit the stream without a done signal
            let _ = tx.send(StreamEvent::Done).await;
        });

        rx
    }

    /// Non-streaming chat — sends a request and returns the full response.
    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
        tools: Option<Vec<Value>>,
    ) -> Result<ChatMessage, String> {
        let url = format!("{}/api/chat", self.base_url);
        let request_body = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: false,
            tools,
            format: None,
        };

        let response = self
            .http
            .post(&url)
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Ollama returned {}: {}", status, body));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let msg = body
            .get("message")
            .ok_or("No message in response")?;

        serde_json::from_value(msg.clone())
            .map_err(|e| format!("Failed to parse message: {}", e))
    }
}
