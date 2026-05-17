use crate::agent::OllamaClient;
use crate::barq::BarqIndex;
use std::sync::Arc;

pub struct PlannerAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    pub dependencies: Vec<String>,
}

impl PlannerAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>) -> Self {
        Self { llm, barq }
    }

    pub async fn decompose(&self, goal: &str) -> anyhow::Result<Vec<PlanStep>> {
        // Query BARQ for context to inform planning
        let context = self.barq.query(goal, 5);
        let mut context_str = String::new();
        for res in context {
            context_str.push_str(&format!("File: {}\nLine: {}\nScore: {}\n\n", res.file_path, res.line, res.score));
        }

        let prompt = format!(
            "Goal: {}\n\nContext:\n{}\n\nDecompose the goal into logical steps that can be independently built. Maximize parallelization. Break components apart so they share minimal dependencies and can be built concurrently. Output strictly JSON with a 'steps' array of objects containing 'id' (string), 'description' (string), and 'dependencies' (array of string ids). Do not output markdown code blocks, just raw JSON.",
            goal, context_str
        );

        let messages = vec![
            rusty_ollama::ChatMessage {
                role: "system".to_string(),
                content: crate::agents::AgentRole::Planner.system_prompt().to_string(),
                tool_calls: None,
            },
            rusty_ollama::ChatMessage {
                role: "user".to_string(),
                content: prompt,
                tool_calls: None,
            }
        ];

        let mut rx = self.llm.chat_stream(messages, None);
        let mut final_response = String::new();
        
        use crate::agent::StreamEvent;
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(text) => final_response.push_str(&text),
                StreamEvent::Done => break,
                StreamEvent::Error(e) => return Err(anyhow::anyhow!("Planner LLM error: {}", e)),
                _ => {}
            }
        }

        // Try to extract JSON if it was wrapped in a block
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

        // Truncate the response in the error message to avoid flooding logs with
        // the full (potentially very long) LLM output.
        const MAX_RESPONSE_IN_ERROR: usize = 500;
        let response_excerpt = if final_response.len() > MAX_RESPONSE_IN_ERROR {
            format!("{}… [truncated {} chars]", &final_response[..MAX_RESPONSE_IN_ERROR], final_response.len())
        } else {
            final_response.clone()
        };
        let parsed: PlanResponse = serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Failed to parse planner JSON: {}\nResponse was: {}", e, response_excerpt))?;

        if parsed.steps.is_empty() {
             return Err(anyhow::anyhow!("Planner returned empty steps array"));
        }

        Ok(parsed.steps)
    }
}
