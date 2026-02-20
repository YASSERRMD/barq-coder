use super::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

pub struct CargoCheck;

#[async_trait]
impl Tool for CargoCheck {
    fn name(&self) -> &'static str {
        "cargo_check"
    }

    fn description(&self) -> &'static str {
        "Run cargo check on a Rust project directory"
    }

    fn schema(&self) -> Value {
        json!({
            "dir": "string"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let dir = args.get("dir").and_then(|v| v.as_str()).unwrap_or(".");

        let mut cmd = Command::new("cargo");
        cmd.arg("check")
            .arg("--message-format")
            .arg("json")
            .current_dir(dir);

        let result = timeout(Duration::from_secs(30), cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let success = output.status.success();
                let stdout = String::from_utf8_lossy(&output.stdout);

                let mut errors = Vec::new();
                let mut warnings = Vec::new();

                for line in stdout.lines() {
                    if let Ok(msg) = serde_json::from_str::<Value>(line) {
                        if msg["reason"] == "compiler-message" {
                            if let Some(message) = msg.get("message") {
                                if message["level"] == "error" {
                                    errors.push(
                                        message["rendered"].as_str().unwrap_or("").to_string(),
                                    );
                                } else if message["level"] == "warning" {
                                    warnings.push(
                                        message["rendered"].as_str().unwrap_or("").to_string(),
                                    );
                                }
                            }
                        }
                    }
                }

                Ok(json!({
                    "success": success,
                    "errors": errors,
                    "warnings": warnings,
                }))
            }
            Ok(Err(e)) => anyhow::bail!("Failed to execute cargo check: {}", e),
            Err(_) => anyhow::bail!("Timeout exceeded"),
        }
    }
}
