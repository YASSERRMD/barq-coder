use crate::agent::OllamaClient;
use crate::barq::BarqIndex;
use std::sync::Arc;

pub struct PlannerAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
}

#[derive(Debug)]
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
            "Goal: {}\n\nContext:\n{}\n\nDecompose the goal into logical steps that can be independently built. Output strictly JSON with a 'steps' array. Each step should have an 'id', 'description', and 'dependencies' array of ids.",
            goal, context_str
        );

        // In a real implementation this would call `self.llm.chat_stream` and aggregate.
        // For now, we mock a standard plan.
        let _ = prompt; // Use prompt
        
        Ok(vec![
            PlanStep {
                id: "step_1".to_string(),
                description: "Analyze dependencies and create a structural outline".to_string(),
                dependencies: vec![],
            },
            PlanStep {
                id: "step_2".to_string(),
                description: "Implement core logic based on outline".to_string(),
                dependencies: vec!["step_1".to_string()],
            },
            PlanStep {
                id: "step_3".to_string(),
                description: "Write tests to verify core logic".to_string(),
                dependencies: vec!["step_2".to_string()],
            }
        ])
    }
}
