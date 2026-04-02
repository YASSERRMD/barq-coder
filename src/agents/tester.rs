use crate::agent::{OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
use std::sync::Arc;

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
        let prompt = format!(
            "Step ID: {}\nImplementation Result: {}\n\nWrite and run Rust tests to verify this implementation. Return the test results or state what actions were taken.",
            step_id, impl_result
        );

        let messages = vec![
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
        let mut rx = self.llm.chat_stream(messages, Some(tool_schemas));

        let mut final_response = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(text) => {
                    final_response.push_str(&text);
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => {
                    return Err(anyhow::anyhow!("LLM error: {}", e));
                }
                _ => {}
            }
        }

        Ok(final_response)
    }
}
