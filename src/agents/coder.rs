use crate::agent::{Message, OllamaClient};
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct CoderAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
}

impl CoderAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self { llm, barq, tools }
    }

    pub async fn implement_step(&self, step_id: &str, description: &str) -> anyhow::Result<String> {
        let context = self.barq.query(description, 5);
        let mut context_str = String::new();
        for res in context {
            context_str.push_str(&format!("File: {}\nContent:\n{}\n\n", res.file_path, res.content));
        }

        let prompt = format!(
            "Step ID: {}\nDescription: {}\n\nContext:\n{}\n\nImplement the step using the context provided. Return the final modified source code or state what actions were taken.",
            step_id, description, context_str
        );

        let messages = vec![
            Message {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Coder.system_prompt().to_string(),
                tool_calls: None,
                tool_call_id: None,
            },
            Message {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
                tool_call_id: None,
            }
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
