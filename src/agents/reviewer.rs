use crate::agent::{Message, OllamaClient};
use crate::barq::BarqIndex;
use serde_json::Value;
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
            Message {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Reviewer.system_prompt().to_string(),
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

        let mut rx = self.llm.chat_stream(messages, vec![]);

        let mut approved = false;
        while let Some(msg) = rx.recv().await {
            if let Ok(val) = serde_json::from_str::<Value>(&msg) {
                if let Some(final_msg) = val.get("final_answer").and_then(|v| v.as_str()) {
                     if final_msg.to_lowercase().contains("\"approved\": true") || final_msg.to_lowercase().contains("approved: true") {
                         approved = true;
                     }
                }
            } else if msg.to_lowercase().contains("\"approved\": true") {
                approved = true;
            }
        }

        Ok(approved)
    }
}
