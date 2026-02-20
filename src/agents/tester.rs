use crate::agent::{Message, OllamaClient};
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
use serde_json::Value;
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
            Message {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Tester.system_prompt().to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let tool_schemas = self.tools.schemas();
        let mut rx = self.llm.chat_stream(messages, tool_schemas);

        let mut final_response = String::new();
        while let Some(msg) = rx.recv().await {
            if let Ok(val) = serde_json::from_str::<Value>(&msg) {
                if let Some(text) = val.get("final_answer").and_then(|v| v.as_str()) {
                    final_response.push_str(text);
                }
            } else {
                final_response.push_str(&msg);
            }
        }

        Ok(final_response)
    }
}
