use std::sync::Arc;
use tokio::sync::mpsc;
use crate::barq::BarqIndex;
use crate::config::Config;
use crate::orchestrator::{Orchestrator, OrchestratorEvent};
use crate::providers::ProviderAdapter;
use crate::tools::ToolRegistry;

/// Forks a new Orchestrator in a background task with isolated conversation context,
/// mirroring Claude Code's Fork subagent capability.
pub struct ForkAgent {
    pub llm: Arc<dyn ProviderAdapter>,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
    pub config: Config,
}

impl ForkAgent {
    pub fn new(
        llm: Arc<dyn ProviderAdapter>,
        barq: Arc<BarqIndex>,
        tools: Arc<ToolRegistry>,
        config: Config,
    ) -> Self {
        Self { llm, barq, tools, config }
    }

    pub fn spawn_fork(&self, goal: &str) -> mpsc::Receiver<OrchestratorEvent> {
        let mut sub_orchestrator = Orchestrator::new(
            Arc::clone(&self.llm),
            Arc::clone(&self.tools),
            Arc::clone(&self.barq),
            self.config.clone(),
        );

        let goal_str = goal.to_string();
        let (tx, rx) = mpsc::channel(256);

        tokio::spawn(async move {
            let mut sub_rx = sub_orchestrator.run(&goal_str);
            while let Some(ev) = sub_rx.recv().await {
                if tx.send(ev).await.is_err() {
                    break;
                }
            }
        });

        rx
    }
}
