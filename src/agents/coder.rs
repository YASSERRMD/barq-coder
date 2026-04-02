use crate::agent::{OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
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
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Coder.system_prompt().to_string(),
                tool_calls: None,
            },
            rusty_ollama::ChatMessage {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
            }
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
