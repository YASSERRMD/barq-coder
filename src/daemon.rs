use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::sync::Arc;
use crate::agent::OllamaClient;
use crate::barq::BarqIndex;
use crate::config::Config;
use crate::orchestrator::{Orchestrator, OrchestratorEvent};
use crate::tools::ToolRegistry;
use serde_json::Value;

/// The Daemon receives goals formatted as JSON via TCP and spawns
/// an isolated Orchestrator headless thread to compute them, streaming JSONL OrchestratorEvents back.
pub async fn start_daemon(
    port: u16,
    agent: OllamaClient,
    tools: Arc<ToolRegistry>,
    barq: Arc<BarqIndex>,
    config: Config,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    tracing::info!("BarqCoder Daemon listening on port {}", port);

    loop {
        let (mut socket, _) = listener.accept().await?;
        
        let agent_clone = agent.clone();
        let tools_clone = Arc::clone(&tools);
        let barq_clone = Arc::clone(&barq);
        let config_clone = config.clone();

        tokio::spawn(async move {
            let mut buf = vec![0; 4096];
            loop {
                match socket.read(&mut buf).await {
                    Ok(0) => return, // Connection closed
                    Ok(n) => {
                        let msg = String::from_utf8_lossy(&buf[..n]);
                        
                        // Parse incoming JSON: {"goal": "build the project"}
                        let goal: String = if let Ok(v) = serde_json::from_str::<Value>(&msg) {
                            v.get("goal").and_then(|g| g.as_str()).unwrap_or("").to_string()
                        } else {
                            msg.trim().to_string()
                        };

                        if goal.is_empty() {
                            let _ = socket.write_all(b"{\"error\": \"Empty goal\"}\n").await;
                            continue;
                        }

                        tracing::info!("Daemon running goal: {}", goal);

                        let mut orchestrator = Orchestrator::new(
                            agent_clone.clone(),
                            Arc::clone(&tools_clone),
                            Arc::clone(&barq_clone),
                            config_clone.clone(),
                        );

                        // Spawn orchestrator
                        let mut rx = orchestrator.run(&goal);

                        // Stream events back to the client
                        while let Some(event) = rx.recv().await {
                            let json = match event {
                                OrchestratorEvent::Token(t) => serde_json::json!({"type": "token", "content": t}),
                                OrchestratorEvent::ToolCall { name, args } => serde_json::json!({"type": "tool_call", "name": name, "args": args}),
                                OrchestratorEvent::ToolResult { name, result } => serde_json::json!({"type": "tool_result", "name": name, "result": result}),
                                OrchestratorEvent::PermissionRequested { name, reason, tx: _, .. } => {
                                    // In daemon mode, we auto-allow or reject depending on config. For safety, reject interactive.
                                    serde_json::json!({"type": "permission_denied_in_daemon", "name": name, "reason": reason})
                                }
                                OrchestratorEvent::Done(d) => serde_json::json!({"type": "done", "content": d}),
                                OrchestratorEvent::Error(e) => serde_json::json!({"type": "error", "content": e}),
                                _ => serde_json::json!({"type": "unsupported_event"})
                            };

                            let out_msg = format!("{}\n", json.to_string());
                            if socket.write_all(out_msg.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                    }
                    Err(_) => return,
                }
            }
        });
    }
}
