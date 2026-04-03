use crate::agent::{OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use crate::sandbox::{GateResult, VerificationGate};
use crate::tools::ToolRegistry;
use std::sync::Arc;

/// Progress events emitted by the CoderAgent during a step.
#[derive(Debug, Clone)]
pub enum CoderProgress {
    Thinking(String),
    ToolCall { name: String, args: String },
    ToolResult { name: String, result: String },
    ShadowWrite { path: String },
    VerificationStarted,
    VerificationPassed { patch: String },
    VerificationFailed { diagnostics: Vec<String>, patch: String },
    Done(String),
}

pub struct CoderAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
}

impl CoderAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, barq, tools }
    }

    /// Implement a task step using the Verification-First contract.
    ///
    /// All file writes go into `VerificationGate`'s shadow workspace.
    /// The gate runs `cargo check` + `cargo test` before producing the patch.
    /// The patch is returned via `GateResult` — the coordinator / TUI decides
    /// whether to present it to the user for approval before applying.
    pub async fn implement_step_verified(
        &self,
        step_id: &str,
        description: &str,
        workspace_root: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<CoderProgress>>,
    ) -> anyhow::Result<GateResult> {
        // Create an isolated shadow workspace for this step.
        let gate = VerificationGate::new(workspace_root, Arc::clone(&self.barq)).await?;

        let context = self.barq.query(description, 5);
        let mut context_str = String::new();
        for res in context {
            context_str.push_str(&format!(
                "File: {}\nContent:\n{}\n\n",
                res.file_path, res.content
            ));
        }

        let prompt = format!(
            "Step ID: {}\nDescription: {}\n\nContext:\n{}\n\n\
             You are working inside a shadow workspace. When you write files using \
             `edit_file` or `create_file`, write the relative path directly — they \
             will be staged in the verification sandbox. Do NOT run `cargo` yourself; \
             the verification gate will run it automatically after you finish.\n\n\
             When you have completed all file writes, output EXACTLY the string \
             `IMPLEMENTATION_COMPLETE` on its own line.",
            step_id, description, context_str
        );

        let mut messages = vec![
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Coder.system_prompt().to_string(),
                tool_calls: None,
            },
            rusty_ollama::ChatMessage {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
            },
        ];

        let tool_schemas = self.tools.schemas();
        let max_iterations = 10;

        for _ in 0..max_iterations {
            let mut rx = self.llm.chat_stream(messages.clone(), Some(tool_schemas.clone()));

            let mut iter_response = String::new();
            let mut tool_calls = Vec::new();

            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Token(text) => {
                        if let Some(tx) = &progress_tx {
                            let _ = tx.send(CoderProgress::Thinking(text.clone())).await;
                        }
                        iter_response.push_str(&text);
                    }
                    StreamEvent::ToolCall(t) => tool_calls.push(t),
                    StreamEvent::Done => break,
                    StreamEvent::Error(e) => return Err(anyhow::anyhow!("Coder LLM error: {}", e)),
                    _ => {}
                }
            }

            // Check for completion sentinel.
            if iter_response.contains("IMPLEMENTATION_COMPLETE") {
                break;
            }

            if tool_calls.is_empty() {
                // No more tool calls and no sentinel — treat as done.
                break;
            }

            messages.push(rusty_ollama::ChatMessage {
                role: "assistant".to_string(),
                content: iter_response.clone(),
                tool_calls: Some(tool_calls.clone()),
            });

            for tc in &tool_calls {
                let tool_name = &tc.function.name;
                let args = tc.function.arguments.clone();

                if let Some(tx) = &progress_tx {
                    let _ = tx
                        .send(CoderProgress::ToolCall {
                            name: tool_name.clone(),
                            args: args.to_string(),
                        })
                        .await;
                }

                // If the tool writes a file, intercept it to the shadow workspace.
                let tool_result = if tool_name == "edit_file" || tool_name == "create_file" {
                    // Extract path + content from args.
                    let path = args
                        .get("path")
                        .or_else(|| args.get("file_path"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let content = args
                        .get("content")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if !path.is_empty() {
                        if let Some(tx) = &progress_tx {
                            let _ = tx
                                .send(CoderProgress::ShadowWrite { path: path.clone() })
                                .await;
                        }
                        match gate.stage_file(&path, &content).await {
                            Ok(()) => serde_json::json!({"ok": true, "staged": path}),
                            Err(e) => serde_json::json!({"error": e.to_string()}),
                        }
                    } else {
                        serde_json::json!({"error": "No path provided"})
                    }
                } else {
                    // All other tools execute normally against the real workspace.
                    if let Some(tool) = self.tools.get(tool_name) {
                        match tool.call(args.clone()).await {
                            Ok(res) => res,
                            Err(e) => serde_json::json!({"error": e.to_string()}),
                        }
                    } else {
                        serde_json::json!({"error": format!("Tool not found: {}", tool_name)})
                    }
                };

                if let Some(tx) = &progress_tx {
                    let _ = tx
                        .send(CoderProgress::ToolResult {
                            name: tool_name.clone(),
                            result: tool_result.to_string(),
                        })
                        .await;
                }

                messages.push(rusty_ollama::ChatMessage {
                    role: "tool".to_string(),
                    content: tool_result.to_string(),
                    tool_calls: None,
                });
            }
        }

        // --- Verification Gate ---
        if let Some(tx) = &progress_tx {
            let _ = tx.send(CoderProgress::VerificationStarted).await;
        }

        let gate_result = gate.verify().await;

        if let Some(tx) = &progress_tx {
            if gate_result.approved {
                let _ = tx
                    .send(CoderProgress::VerificationPassed {
                        patch: gate_result.patch.clone(),
                    })
                    .await;
            } else {
                let _ = tx
                    .send(CoderProgress::VerificationFailed {
                        diagnostics: gate_result.diagnostics.clone(),
                        patch: gate_result.patch.clone(),
                    })
                    .await;
            }
        }

        Ok(gate_result)
    }

    // ── Legacy non-verified path (kept for backwards compat / headless mode) ──

    pub async fn implement_step(
        &self,
        step_id: &str,
        description: &str,
    ) -> anyhow::Result<String> {
        let context = self.barq.query(description, 5);
        let mut context_str = String::new();
        for res in context {
            context_str
                .push_str(&format!("File: {}\nContent:\n{}\n\n", res.file_path, res.content));
        }

        let prompt = format!(
            "Step ID: {}\nDescription: {}\n\nContext:\n{}\n\nImplement this step.",
            step_id, description, context_str
        );

        let mut messages = vec![
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Coder.system_prompt().to_string(),
                tool_calls: None,
            },
            rusty_ollama::ChatMessage {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
            },
        ];

        let tool_schemas = self.tools.schemas();
        let mut final_response = String::new();

        for _ in 0..7 {
            let mut rx = self.llm.chat_stream(messages.clone(), Some(tool_schemas.clone()));
            let mut iter_response = String::new();
            let mut tool_calls = Vec::new();

            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Token(text) => iter_response.push_str(&text),
                    StreamEvent::ToolCall(t) => tool_calls.push(t),
                    StreamEvent::Done => break,
                    StreamEvent::Error(e) => return Err(anyhow::anyhow!("Coder LLM error: {}", e)),
                    _ => {}
                }
            }

            if tool_calls.is_empty() {
                final_response = iter_response;
                break;
            }

            messages.push(rusty_ollama::ChatMessage {
                role: "assistant".to_string(),
                content: iter_response,
                tool_calls: Some(tool_calls.clone()),
            });

            for tc in tool_calls {
                let mut result = String::new();
                if let Some(tool) = self.tools.get(&tc.function.name) {
                    match tool.call(tc.function.arguments.clone()).await {
                        Ok(res) => result = res.to_string(),
                        Err(e) => result = format!("Error: {}", e),
                    }
                } else {
                    result = format!("Tool not found: {}", tc.function.name);
                }
                messages.push(rusty_ollama::ChatMessage {
                    role: "tool".to_string(),
                    content: result,
                    tool_calls: None,
                });
            }
        }

        Ok(final_response)
    }
}
