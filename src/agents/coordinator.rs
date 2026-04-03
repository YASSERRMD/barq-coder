use crate::agent::OllamaClient;
use crate::barq::BarqIndex;
use crate::tools::ToolRegistry;
use std::sync::Arc;
use super::planner::PlannerAgent;
use super::coder::CoderAgent;
use super::tester::TesterAgent;
use super::reviewer::ReviewerAgent;
use crate::tasks::{TaskBoard, TaskStatus};
use std::time::Duration;

pub struct CoordinatorAgent {
    pub barq: Arc<BarqIndex>,
    pub planner: Arc<PlannerAgent>,
    pub coder: Arc<CoderAgent>,
    pub tester: Arc<TesterAgent>,
    pub reviewer: Arc<ReviewerAgent>,
    pub board: Arc<TaskBoard>,
}

impl CoordinatorAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            barq: barq.clone(),
            planner: Arc::new(PlannerAgent::new(llm.clone(), barq.clone())),
            coder: Arc::new(CoderAgent::new(llm.clone(), barq.clone(), tools.clone())),
            tester: Arc::new(TesterAgent::new(llm.clone(), barq.clone(), tools.clone())),
            reviewer: Arc::new(ReviewerAgent::new(llm, barq)),
            board: Arc::new(TaskBoard::new()),
        }
    }

    pub async fn execute_goal(&self, goal: &str) -> anyhow::Result<()> {
        let plan = self.planner.decompose(goal).await?;
        
        for step in &plan {
            let _ = self.board.create_task(step.id.clone(), step.description.clone(), step.dependencies.clone());
        }

        use futures::StreamExt;
        let mut tasks = futures::stream::FuturesUnordered::new();

        loop {
            // Check board for pending tasks that have dependencies met
            let all_tasks = self.board.list_tasks().map_err(|e| anyhow::anyhow!(e))?;
            
            let mut all_completed = true;
            for task in &all_tasks {
                if task.status != TaskStatus::Completed {
                    all_completed = false;
                }

                if task.status == TaskStatus::Pending {
                    let can_start = self.board.are_dependencies_met(&task.id).unwrap_or(false);
                    
                    if can_start {
                        let _ = self.board.update_status(&task.id, TaskStatus::InProgress);
                        let _ = self.board.assign_task(&task.id, "sub_agent_swarm");
                        
                        let coder = Arc::clone(&self.coder);
                        let tester = Arc::clone(&self.tester);
                        let reviewer = Arc::clone(&self.reviewer);
                        let step_id = task.id.clone();
                        let desc = task.description.clone();

                        tasks.push(tokio::spawn(async move {
                            let impl_result = coder.implement_step(&step_id, &desc).await?;
                            let _test_result = tester.test_step(&step_id, &impl_result).await?;
                            let is_approved = reviewer.review_diff(&step_id, &impl_result).await?;
                            Ok::<_, anyhow::Error>((step_id, is_approved))
                        }));
                    }
                }
            }

            if all_completed && tasks.is_empty() {
                break;
            }

            // If tasks are running, wait for the next completion. If tasks are empty, wait and loop (should only happen if tasks fail and stay "Failed").
            if tasks.is_empty() {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            if let Some(res) = tasks.next().await {
                match res {
                    Ok(Ok((step_id, approved))) => {
                        if approved {
                            let _ = self.board.update_status(&step_id, TaskStatus::Completed);
                        } else {
                            let _ = self.board.update_status(&step_id, TaskStatus::Failed);
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
        }

        Ok(())
    }
}
