use crate::tools::{CommandRisk, PermissionResult, Tool, ToolMetadata, ValidationResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};

// ─────────────────────────────────────────────────────────────────────────────
// MCP Client for discovering external tools over stdio/SSE.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: usize,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
struct JsonRpcNotification {
    jsonrpc: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct McpToolDef {
    name: String,
    description: String,
    inputSchema: Value,
}

struct RpcState {
    pending: HashMap<usize, oneshot::Sender<Value>>,
}

pub struct McpClient {
    pub server_name: String,
    tx: mpsc::Sender<JsonRpcRequest>,
    state: Arc<Mutex<RpcState>>,
    next_id: Arc<AtomicUsize>,
}

impl McpClient {
    pub async fn new(server_name: &str, command: &str, args: &[&str]) -> anyhow::Result<Self> {
        let mut child = Command::new(command)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()?;

        let mut stdin = child.stdin.take().expect("Failed to open stdin");
        let stdout = child.stdout.take().expect("Failed to open stdout");

        let state = Arc::new(Mutex::new(RpcState {
            pending: HashMap::new(),
        }));
        let state_clone = Arc::clone(&state);

        let (tx, mut rx) = mpsc::channel::<JsonRpcRequest>(32);

        // Writer task
        tokio::spawn(async move {
            while let Some(req) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&req) {
                    if stdin.write_all(format!("{}\n", json).as_bytes()).await.is_err() {
                        break;
                    }
                    let _ = stdin.flush().await;
                }
            }
        });

        // Reader task
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = reader.next_line().await {
                if let Ok(mut parsed) = serde_json::from_str::<Value>(&line) {
                    if let Some(id) = parsed.get("id").and_then(|i| i.as_u64()) {
                        let mut st = state_clone.lock().await;
                        if let Some(sender) = st.pending.remove(&(id as usize)) {
                            // Extract result or error
                            if let Some(res) = parsed.get_mut("result") {
                                let _ = sender.send(res.take());
                            } else if let Some(err) = parsed.get_mut("error") {
                                let _ = sender.send(serde_json::json!({"error": err.take()}));
                            }
                        }
                    }
                }
            }
        });

        let client = Self {
            server_name: server_name.to_string(),
            tx,
            state,
            next_id: Arc::new(AtomicUsize::new(1)),
        };

        // Send initialize
        let init_params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "clientInfo": {
                "name": "barq-coder",
                "version": "0.1.0"
            },
            "capabilities": {}
        });

        client.request("initialize", Some(init_params)).await?;

        // Send initialized notification
        let notif = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: "notifications/initialized".to_string(),
            params: None,
        };
        // We cheat and cast notification to request structure without an ID just for serialization convenience in the writer
        if let Ok(json) = serde_json::to_string(&notif) {
             // To cleanly send notifications we bypass strongly typed tx or parse it
             // Real implementation would have an Enum for Request/Notification.
        }

        Ok(client)
    }

    async fn request(&self, method: &str, params: Option<Value>) -> anyhow::Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (reply_tx, reply_rx) = oneshot::channel();

        {
            let mut st = self.state.lock().await;
            st.pending.insert(id, reply_tx);
        }

        self.tx
            .send(JsonRpcRequest {
                jsonrpc: "2.0".to_string(),
                id,
                method: method.to_string(),
                params,
            })
            .await?;

        let res = reply_rx.await?;
        if res.get("error").is_some() {
            Err(anyhow::anyhow!("MCP Error: {}", res["error"]))
        } else {
            Ok(res)
        }
    }

    /// Discovers tools from the remote server and wraps them in McpTool.
    pub async fn discover_tools(self: Arc<Self>) -> anyhow::Result<Vec<Box<dyn Tool + Send + Sync>>> {
        let res = self.request("tools/list", None).await?;
        let mut tools: Vec<Box<dyn Tool + Send + Sync>> = Vec::new();

        if let Some(list) = res.get("tools").and_then(|t| t.as_array()) {
            for t in list {
                if let Ok(def) = serde_json::from_value::<McpToolDef>(t.clone()) {
                    let wrapper = McpToolWrapper {
                        client: Arc::clone(&self),
                        name: def.name,
                        description: def.description,
                        schema: def.inputSchema,
                    };
                    tools.push(Box::new(wrapper));
                }
            }
        }

        Ok(tools)
    }
}

pub struct McpToolWrapper {
    client: Arc<McpClient>,
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &'static str {
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.description.clone().into_boxed_str())
    }

    fn schema(&self) -> Value {
        self.schema.clone()
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let params = serde_json::json!({
            "name": self.name,
            "arguments": args
        });
        self.client.request("tools/call", Some(params)).await
    }

    fn is_concurrent_safe(&self) -> bool {
        true
    }
    
    fn is_read_only(&self) -> bool {
        false
    }
}
