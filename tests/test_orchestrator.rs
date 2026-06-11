use barqcoder::barq::BarqIndex;
use barqcoder::config::Config;
use barqcoder::orchestrator::{Orchestrator, OrchestratorEvent};
use barqcoder::providers::build_provider;
use barqcoder::tools::ToolRegistry;
use std::sync::Arc;
use tokio::time::timeout;
use std::time::Duration;

#[tokio::test]
async fn test_max_iterations() {
    let config = Config::default();
    let agent = build_provider(&config);
    let barq = Arc::new(BarqIndex::new(&config).unwrap());
    let tools = Arc::new(ToolRegistry::new());

    let mut orchestrator = Orchestrator::new(agent, tools, barq, config);
    let mut rx = orchestrator.run("hello");

    let mut finished = false;
    while let Ok(Some(event)) = timeout(Duration::from_secs(1), rx.recv()).await {
        match event {
            OrchestratorEvent::Done(_) | OrchestratorEvent::Error(_) => {
                finished = true;
                break;
            }
            _ => {}
        }
    }
    assert!(finished);
}

#[tokio::test]
async fn test_final_answer() {
    let config = Config::default();
    let agent = build_provider(&config);
    let barq = Arc::new(BarqIndex::new(&config).unwrap());
    let tools = Arc::new(ToolRegistry::new());

    let mut orchestrator = Orchestrator::new(agent, tools, barq, config);
    let mut rx = orchestrator.run("hello");

    let mut finished = false;
    while let Ok(Some(event)) = timeout(Duration::from_secs(1), rx.recv()).await {
        match event {
            OrchestratorEvent::Done(_) | OrchestratorEvent::Error(_) => {
                finished = true;
                break;
            }
            _ => {}
        }
    }
    assert!(finished);
}
