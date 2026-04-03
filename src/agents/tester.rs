use crate::agent::{OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
use std::sync::Arc;

use crate::verifier::Verifier;

pub struct TesterAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
}

impl TesterAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, barq, tools }
    }

    pub async fn test_step(&self, step_id: &str, impl_result: &str) -> anyhow::Result<String> {
        // Run standard Rust verification checks first
        let verifier = Verifier::new(self.barq.clone(), ".");
        let res = verifier.verify_edit("", "", "").await; // Mock diff for full verification run
        
        let all_pass = res.cargo_check_pass && res.cargo_test_pass;
        if all_pass {
            return Ok(format!("Step ID: {}\nAll tests and build checks passed.", step_id));
        }

        // If it failed, let the Tester Agent think and debug the failures
        let prompt = format!(
            "Step ID: {}\nImplementation Result: {}\n\nVerification failed. Use the tools to debug and fix the tests. Errors:\n{:?}",
            step_id, impl_result, res.errors
        );

        let mut messages = vec![
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Tester.system_prompt().to_string(),
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
        let max_iterations = 5;

        for _ in 0..max_iterations {
            let mut rx = self.llm.chat_stream(messages.clone(), Some(tool_schemas.clone()));
            
            let mut iter_response = String::new();
            let mut tool_calls = Vec::new();

            while let Some(event) = rx.recv().await {
                match event {
                    StreamEvent::Token(text) => iter_response.push_str(&text),
                    StreamEvent::ToolCall(t) => tool_calls.push(t),
                    StreamEvent::Done => break,
                    StreamEvent::Error(e) => return Err(anyhow::anyhow!("Tester LLM error: {}", e)),
                    _ => {}
                }
            }

            if tool_calls.is_empty() {
                final_response = iter_response;
                break;
            } else {
                messages.push(rusty_ollama::ChatMessage {
                    role: "assistant".to_string(),
                    content: iter_response,
                    tool_calls: Some(tool_calls.clone()),
                });

                for tc in tool_calls {
                    let mut tool_result = String::new();
                    if let Some(tool) = self.tools.get(&tc.function.name) {
                        let parsed_args = tc.function.arguments.clone();
                        match tool.call(parsed_args).await {
                            Ok(res) => tool_result = res.to_string(),
                            Err(e) => tool_result = format!("Error executing tool: {}", e),
                        }
                    } else {
                        tool_result = format!("Tool not found: {}", tc.function.name);
                    }
                    
                    messages.push(rusty_ollama::ChatMessage {
                        role: "tool".to_string(),
                        content: tool_result,
                        tool_calls: None,
                    });
                }
            }
        }

        Ok(final_response)
    }
}
