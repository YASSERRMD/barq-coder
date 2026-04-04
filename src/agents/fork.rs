use std::sync::Arc;
use tokio::sync::mpsc;
use crate::agent::OllamaClient;
use crate::barq::BarqIndex;
use crate::config::Config;
use crate::orchestrator::{Orchestrator, OrchestratorEvent};
use crate::tools::ToolRegistry;

// ─────────────────────────────────────────────────────────────────────────────
// Fork Agent Strategy 
// Spawns a dedicated Orchestrator in a background thread with isolated context,
// mimicking Claude Code's "Fork Subagent" capability.
// ─────────────────────────────────────────────────────────────────────────────

pub struct ForkAgent {
    pub llm: OllamaClient,
    pub barq: Arc<BarqIndex>,
    pub tools: Arc<ToolRegistry>,
    pub config: Config,
}

impl ForkAgent {
    pub fn new(llm: OllamaClient, barq: Arc<BarqIndex>, tools: Arc<ToolRegistry>, config: Config) -> Self {
        Self {
            llm,
            barq,
            tools,
            config,
        }
    }

    /// Forks a new agent process (as an asynchronous tokio task) to handle a background goal.
    /// Returns a channel to stream back the sub-agent's events without blocking the main UI.
    pub fn spawn_fork(&self, goal: &str) -> mpsc::Receiver<OrchestratorEvent> {
        let mut sub_orchestrator = Orchestrator::new(
            self.llm.clone(),
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
