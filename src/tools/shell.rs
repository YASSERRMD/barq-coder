use super::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub struct ShellExec;

#[async_trait]
impl Tool for ShellExec {
    fn name(&self) -> &'static str {
        "shell_exec"
    }

    fn description(&self) -> &'static str {
        "Run shell command in sandboxed workspace"
    }

    fn schema(&self) -> Value {
        json!({
            "command": "string",
            "working_dir": "string",
            "timeout_secs": "number"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let working_dir = args
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let timeout_secs = args.get("timeout_secs").and_then(|v| v.as_u64()).unwrap_or(30);
        let timeout_secs = std::cmp::min(timeout_secs, 60);

        let command_str = command.to_string();
        let blocked_cmds = ["rm -rf /", "sudo", "curl", "wget", "ssh"];
        for blocked in blocked_cmds.iter() {
            if command_str.contains(blocked) {
                return Err(anyhow::anyhow!("Blocked command detected: {}", blocked));
            }
        }

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(working_dir);

        let result = timeout(Duration::from_secs(timeout_secs), cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                Ok(json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                    "timed_out": false
                }))
            }
            Ok(Err(e)) => Err(anyhow::anyhow!("Failed to execute command: {}", e)),
            Err(_) => Ok(json!({
                "stdout": "",
                "stderr": "Command timed out",
                "exit_code": -1,
                "timed_out": true
            })),
        }
    }
}

pub struct GitTool;

#[async_trait]
impl Tool for GitTool {
    fn name(&self) -> &'static str {
        "git_ops"
    }

    fn description(&self) -> &'static str {
        "Git operations: status/diff/log/add/commit"
    }

    fn schema(&self) -> Value {
        json!({
            "operation": "string",
            "args": "string"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
        let cmd_args = args.get("args").and_then(|v| v.as_str()).unwrap_or("");

        let valid_ops = ["status", "diff", "log", "add", "commit"];
        if !valid_ops.contains(&op) {
            return Err(anyhow::anyhow!("Invalid git operation: {}", op));
        }

        let mut cmd = Command::new("git");
        cmd.arg(op);
        
        if !cmd_args.is_empty() {
             // Split args carefully in a real implementation
             for arg in cmd_args.split_whitespace() {
                 cmd.arg(arg);
             }
        }

        let result = cmd.output().await;

        match result {
            Ok(output) => {
                let success = output.status.success();
                let out_str = if success {
                    String::from_utf8_lossy(&output.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&output.stderr).to_string()
                };

                Ok(json!({
                    "output": out_str,
                    "success": success
                }))
            }
            Err(e) => Err(anyhow::anyhow!("Failed to execute git command: {}", e)),
        }
    }
}
