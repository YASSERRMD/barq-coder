use crate::agent::OllamaClient;
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
use std::sync::Arc;
use super::planner::{PlannerAgent, PlanStep};
use super::coder::CoderAgent;
use super::tester::TesterAgent;
use super::reviewer::ReviewerAgent;

pub struct CoordinatorAgent {
    pub barq: Arc<BarqIndex>,
    pub planner: PlannerAgent,
    pub coder: CoderAgent,
    pub tester: TesterAgent,
    pub reviewer: ReviewerAgent,
}

impl CoordinatorAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            barq: barq.clone(),
            planner: PlannerAgent::new(llm.clone(), barq.clone()),
            coder: CoderAgent::new(llm.clone(), barq.clone(), tools.clone()),
            tester: TesterAgent::new(llm.clone(), barq.clone(), tools.clone()),
            reviewer: ReviewerAgent::new(llm, barq),
        }
    }

    pub async fn execute_goal(&self, goal: &str) -> anyhow::Result<()> {
        let plan = self.planner.decompose(goal).await?;
        
        for step in plan {
            // Very simplified synchronous routing for now. 
            // Commits 37-38 will handle parallel execution and conflicts.
            let impl_result = self.coder.implement_step(&step.id, &step.description).await?;
            let _test_result = self.tester.test_step(&step.id, &impl_result).await?;
            
            // In reality, Reviewer reviews a diff, we'll mock it passing for now.
            let is_approved = self.reviewer.review_diff(&step.id, &impl_result).await?;
            
            if !is_approved {
                eprintln!("Step {} was rejected by reviewer.", step.id);
            }
        }

        Ok(())
    }
}
