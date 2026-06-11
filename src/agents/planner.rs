use barq_ir::{AgentMessage, StreamEvent};
use crate::barq::BarqIndex;
use crate::providers::ProviderAdapter;
use std::sync::Arc;

pub struct PlannerAgent {
    pub llm: Arc<dyn ProviderAdapter>,
    pub barq: Arc<BarqIndex>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

impl PlannerAgent {
    pub fn new(llm: Arc<dyn ProviderAdapter>, barq: Arc<BarqIndex>) -> Self {
        Self { llm, barq }
    }

    pub async fn decompose(&self, goal: &str) -> anyhow::Result<Vec<PlanStep>> {
        let context = self.barq.query(goal, 5);
        let mut context_str = String::new();
        for res in context {
            context_str.push_str(&format!(
                "File: {}\nLine: {}\nScore: {}\n\n",
                res.file_path, res.line, res.score
            ));
        }

        let prompt = format!(
            "Goal: {}\n\nContext:\n{}\n\nDecompose the goal into logical steps that can be independently built. \
             Maximize parallelization. Output strictly JSON with a 'steps' array of objects containing \
             'id' (string), 'description' (string), and 'dependencies' (array of string ids). \
             Do not output markdown code blocks, just raw JSON.",
            goal, context_str
        );

        let messages = vec![
            AgentMessage::system(crate::agents::AgentRole::Planner.system_prompt()),
            AgentMessage::user(prompt),
        ];

        let mut rx = self.llm.chat_stream(messages, None);
        let mut final_response = String::new();

        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::TextDelta { content } => final_response.push_str(&content),
                StreamEvent::Finish { .. } => break,
                StreamEvent::Error { message, .. } => {
                    return Err(anyhow::anyhow!("Planner LLM error: {}", message))
                }
                _ => {}
            }
        }

        let mut json_str = final_response.as_str();
        if let Some(start) = json_str.find('{') {
            if let Some(end) = json_str.rfind('}') {
                json_str = &json_str[start..=end];
            }
        }

        #[derive(serde::Deserialize)]
        struct PlanResponse {
            steps: Vec<PlanStep>,
        }

        let parsed: PlanResponse = serde_json::from_str(json_str).map_err(|e| {
            anyhow::anyhow!(
                "Failed to parse planner JSON: {}\nResponse was: {}",
                e,
                final_response
            )
        })?;

        if parsed.steps.is_empty() {
            return Err(anyhow::anyhow!("Planner returned empty steps array"));
        }

        Ok(parsed.steps)
    }
}
