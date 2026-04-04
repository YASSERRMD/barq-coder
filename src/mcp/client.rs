use crate::tools::{Tool, PermissionResult, ValidationResult, CommandRisk, ToolMetadata};
use async_trait::async_trait;
use serde_json::Value;

// ─────────────────────────────────────────────────────────────────────────────
// MCP Client for discovering external tools over stdio/SSE.
// ─────────────────────────────────────────────────────────────────────────────

pub struct McpClient {
    pub server_name: String,
    pub command: String,
}

impl McpClient {
    pub fn new(server_name: &str, command: &str) -> Self {
        Self {
            server_name: server_name.to_string(),
            command: command.to_string(),
        }
    }

    /// Discovers tools from the remote server and wraps them in McpTool.
    pub async fn discover_tools(&self) -> Vec<Box<dyn Tool + Send + Sync>> {
        // Placeholder: In a real implementation this spawns `self.command` via stdio,
        // reads the JSON-RPC initialization, and iterates over tool schemas.
        vec![]
    }
}

pub struct McpToolWrapper {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[async_trait]
impl Tool for McpToolWrapper {
    fn name(&self) -> &'static str {
        // This is a rough prototype; realistic implementation requires lifetime/string management
        // to return a &str from a dynamically created tool, usually via Box::leak or similar 
        // string interning for the harness trait, since it demands &'static str.
        Box::leak(self.name.clone().into_boxed_str())
    }

    fn description(&self) -> &'static str {
        Box::leak(self.description.clone().into_boxed_str())
    }

    fn schema(&self) -> Value {
        self.schema.clone()
    }

    async fn call(&self, _args: Value) -> anyhow::Result<Value> {
        // Proxy call to the external MCP server process over JSON-RPC.
        Ok(serde_json::json!({"status": "proxied via tool_call protocol"}))
    }
    
    fn is_concurrent_safe(&self) -> bool { true }
    fn is_read_only(&self) -> bool { false }
}
