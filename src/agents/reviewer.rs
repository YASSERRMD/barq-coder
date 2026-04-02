use crate::agent::{OllamaClient, StreamEvent};
use crate::barq::BarqIndex;
use std::sync::Arc;

pub struct ReviewerAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
}

impl ReviewerAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>) -> Self {
        Self { llm, barq }
    }

    pub async fn review_diff(&self, step_id: &str, diff: &str) -> anyhow::Result<bool> {
        let prompt = format!(
            "Review the following diff for step {}:\n\n{}\n\nDoes this code meet quality, security, and performance standards? Reply with strictly JSON containing a 'approved' boolean field and a 'feedback' string.",
            step_id, diff
        );

        let messages = vec![
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Reviewer.system_prompt().to_string(),
                tool_calls: None,
            },
            rusty_ollama::ChatMessage {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
            },
        ];

        let mut rx = self.llm.chat_stream(messages, None);

        let mut full_response = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(text) => {
                    full_response.push_str(&text);
                }
                StreamEvent::Done => break,
                StreamEvent::Error(e) => {
                    return Err(anyhow::anyhow!("LLM error: {}", e));
                }
                _ => {}
            }
        }

        let approved = full_response.to_lowercase().contains("\"approved\": true")
            || full_response.to_lowercase().contains("\"approved\":true");

        Ok(approved)
    }
}
