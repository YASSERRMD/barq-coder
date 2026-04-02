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
    pub planner: Arc<PlannerAgent>,
    pub coder: Arc<CoderAgent>,
    pub tester: Arc<TesterAgent>,
    pub reviewer: Arc<ReviewerAgent>,
}

impl CoordinatorAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            barq: barq.clone(),
            planner: Arc::new(PlannerAgent::new(llm.clone(), barq.clone())),
            coder: Arc::new(CoderAgent::new(llm.clone(), barq.clone(), tools.clone())),
            tester: Arc::new(TesterAgent::new(llm.clone(), barq.clone(), tools.clone())),
            reviewer: Arc::new(ReviewerAgent::new(llm, barq)),
        }
    }

    pub async fn execute_goal(&self, goal: &str) -> anyhow::Result<()> {
        let plan = self.planner.decompose(goal).await?;
        
        let mut completed = std::collections::HashSet::new();
        let mut in_progress = std::collections::HashSet::new();
        use futures::StreamExt;
        let mut tasks = futures::stream::FuturesUnordered::new();

        loop {
            // Spawn any steps that have all dependencies met
            for step in &plan {
                if !completed.contains(&step.id) && !in_progress.contains(&step.id) {
                    let can_start = step.dependencies.iter().all(|d| completed.contains(d));
                    
                    if can_start {
                        in_progress.insert(step.id.clone());
                        let coder = Arc::clone(&self.coder);
                        let tester = Arc::clone(&self.tester);
                        let reviewer = Arc::clone(&self.reviewer);
                        let step_id = step.id.clone();
                        let desc = step.description.clone();

                        tasks.push(tokio::spawn(async move {
                            let impl_result = coder.implement_step(&step_id, &desc).await?;
                            let _test_result = tester.test_step(&step_id, &impl_result).await?;
                            let is_approved = reviewer.review_diff(&step_id, &impl_result).await?;
                            Ok::<_, anyhow::Error>((step_id, is_approved))
                        }));
                    }
                }
            }

            if tasks.is_empty() {
                break;
            }

            if let Some(res) = tasks.next().await {
                match res {
                    Ok(Ok((step_id, approved))) => {
                        in_progress.remove(&step_id);
                        completed.insert(step_id.clone());
                        if !approved {
                            // In real system, re-delegate to coder or mark failed
                        }
                    }
                    Err(join_err) => {
                        return Err(anyhow::anyhow!("Task join failed: {:?}", join_err));
                    }
                    Ok(Err(e)) => {
                        return Err(anyhow::anyhow!("Task failed: {:?}", e));
                    }
                }
            }

            if completed.len() == plan.len() {
                break;
            }
        }

        Ok(())
    }
}
