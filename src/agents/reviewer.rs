use barq_ir::{AgentMessage, StreamEvent};
use crate::barq::BarqIndex;
use crate::providers::ProviderAdapter;
use std::sync::Arc;

pub struct ReviewerAgent {
    pub llm: Arc<dyn ProviderAdapter>,
    pub barq: Arc<BarqIndex>,
}

impl ReviewerAgent {
    pub fn new(llm: Arc<dyn ProviderAdapter>, barq: Arc<BarqIndex>) -> Self {
        Self { llm, barq }
    }

    pub async fn review_diff(&self, step_id: &str, diff: &str) -> anyhow::Result<bool> {
        let prompt = format!(
            "Review the following diff for step {}:\n\n{}\n\n\
             Does this code meet quality, security, and performance standards? \
             Reply with strictly JSON containing an 'approved' boolean field and a 'feedback' string.",
            step_id, diff
        );

        let messages = vec![
            AgentMessage::system(crate::agents::AgentRole::Reviewer.system_prompt()),
            AgentMessage::user(prompt),
        ];

        let mut rx = self.llm.chat_stream(messages, None);
        let mut full_response = String::new();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta { content } => full_response.push_str(&content),
                StreamEvent::Finish { .. } => break,
                StreamEvent::Error { message, .. } => {
                    return Err(anyhow::anyhow!("Reviewer LLM error: {}", message))
                }
                _ => {}
            }
        }

        let approved = full_response.to_lowercase().contains("\"approved\": true")
            || full_response.to_lowercase().contains("\"approved\":true");

        Ok(approved)
    }
}
