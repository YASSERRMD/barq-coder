use barq_ir::{AgentMessage, StreamEvent};
use crate::barq::BarqIndex;
use crate::providers::ProviderAdapter;
use crate::sandbox::{GateResult, VerificationGate};
use crate::tools::ToolRegistry;
use std::sync::Arc;

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
    pub llm: Arc<dyn ProviderAdapter>,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
}

impl CoderAgent {
    pub fn new(llm: Arc<dyn ProviderAdapter>, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, barq, tools }
    }

    /// Implement a task step with verification-first contract.
    /// All file writes go into VerificationGate's shadow workspace.
    /// cargo check + cargo test run before the patch is produced.
    pub async fn implement_step_verified(
        &self,
        step_id: &str,
        description: &str,
        workspace_root: &str,
        progress_tx: Option<tokio::sync::mpsc::Sender<CoderProgress>>,
    ) -> anyhow::Result<GateResult> {
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
            AgentMessage::system(crate::agents::AgentRole::Coder.system_prompt()),
            AgentMessage::user(prompt),
        ];

        let tool_schemas = self.tools.schemas();
        let max_iterations = 10;

        for _ in 0..max_iterations {
            let mut rx = self.llm.chat_stream(messages.clone(), Some(tool_schemas.clone()));
            let mut iter_response = String::new();
            let mut tool_calls = Vec::new();

            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::TextDelta { content } => {
                        if let Some(tx) = &progress_tx {
                            let _ = tx.send(CoderProgress::Thinking(content.clone())).await;
                        }
                        iter_response.push_str(&content);
                    }
                    StreamEvent::ToolCallDone { payload } => tool_calls.push(payload),
                    StreamEvent::Finish { .. } => break,
                    StreamEvent::Error { message, .. } => {
                        return Err(anyhow::anyhow!("Coder LLM error: {}", message))
                    }
                    _ => {}
                }
            }

            if iter_response.contains("IMPLEMENTATION_COMPLETE") {
                break;
            }

            if tool_calls.is_empty() {
                break;
            }

            messages.push(AgentMessage::assistant_with_tools(
                iter_response.clone(),
                tool_calls.clone(),
            ));

            for tc in &tool_calls {
                if let Some(tx) = &progress_tx {
                    let _ = tx
                        .send(CoderProgress::ToolCall {
                            name: tc.name.clone(),
                            args: tc.arguments.to_string(),
                        })
                        .await;
                }

                let tool_result =
                    if tc.name == "edit_file" || tc.name == "create_file" {
                        let path = tc.arguments
                            .get("path")
                            .or_else(|| tc.arguments.get("file_path"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let content = tc.arguments
                            .get("content")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();

                        if !path.is_empty() {
                            if let Some(tx) = &progress_tx {
                                let _ = tx.send(CoderProgress::ShadowWrite { path: path.clone() }).await;
                            }
                            match gate.stage_file(&path, &content).await {
                                Ok(()) => serde_json::json!({"ok": true, "staged": path}),
                                Err(e) => serde_json::json!({"error": e.to_string()}),
                            }
                        } else {
                            serde_json::json!({"error": "No path provided"})
                        }
                    } else if let Some(tool) = self.tools.get(&tc.name) {
                        match tool.call(tc.arguments.clone()).await {
                            Ok(res) => res,
                            Err(e) => serde_json::json!({"error": e.to_string()}),
                        }
                    } else {
                        serde_json::json!({"error": format!("Tool not found: {}", tc.name)})
                    };

                if let Some(tx) = &progress_tx {
                    let _ = tx
                        .send(CoderProgress::ToolResult {
                            name: tc.name.clone(),
                            result: tool_result.to_string(),
                        })
                        .await;
                }

                messages.push(AgentMessage::tool_result(
                    tc.id.clone(),
                    tool_result.to_string(),
                ));
            }
        }

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

    pub async fn implement_step(&self, step_id: &str, description: &str) -> anyhow::Result<String> {
        let context = self.barq.query(description, 5);
        let mut context_str = String::new();
        for res in context {
            context_str.push_str(&format!(
                "File: {}\nContent:\n{}\n\n",
                res.file_path, res.content
            ));
        }

        let prompt = format!(
            "Step ID: {}\nDescription: {}\n\nContext:\n{}\n\nImplement this step.",
            step_id, description, context_str
        );

        let mut messages = vec![
            AgentMessage::system(crate::agents::AgentRole::Coder.system_prompt()),
            AgentMessage::user(prompt),
        ];

        let tool_schemas = self.tools.schemas();
        let mut final_response = String::new();

        for _ in 0..7 {
            let mut rx = self.llm.chat_stream(messages.clone(), Some(tool_schemas.clone()));
            let mut iter_response = String::new();
            let mut tool_calls = Vec::new();

            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::TextDelta { content } => iter_response.push_str(&content),
                    StreamEvent::ToolCallDone { payload } => tool_calls.push(payload),
                    StreamEvent::Finish { .. } => break,
                    StreamEvent::Error { message, .. } => {
                        return Err(anyhow::anyhow!("Coder LLM error: {}", message))
                    }
                    _ => {}
                }
            }

            if tool_calls.is_empty() {
                final_response = iter_response;
                break;
            }

            messages.push(AgentMessage::assistant_with_tools(iter_response, tool_calls.clone()));

            for tc in tool_calls {
                let result = if let Some(tool) = self.tools.get(&tc.name) {
                    match tool.call(tc.arguments.clone()).await {
                        Ok(res) => res.to_string(),
                        Err(e) => format!("Error: {}", e),
                    }
                } else {
                    format!("Tool not found: {}", tc.name)
                };
                messages.push(AgentMessage::tool_result(tc.id, result));
            }
        }

        Ok(final_response)
    }
}
