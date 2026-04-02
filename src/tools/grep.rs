use super::{Tool, ToolMetadata};
use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;
use std::time::Duration;
use tokio::time::timeout;

/// GrepTool — search file contents using ripgrep (like Claude Code's GrepTool).
pub struct GrepTool;

#[async_trait]
impl Tool for GrepTool {
    fn name(&self) -> &'static str { "grep" }

    fn description(&self) -> &'static str {
        "Search file contents using ripgrep for pattern matching"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Search pattern (regex supported)" },
                "path": { "type": "string", "description": "Directory or file to search (default '.')" },
                "include": { "type": "string", "description": "File glob to include (e.g. '*.rs')" },
                "case_insensitive": { "type": "boolean", "description": "Case-insensitive search" }
            },
            "required": ["pattern"]
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            search_hint: Some("search content regex ripgrep find text".to_string()),
            ..Default::default()
        }
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let include = args.get("include").and_then(|v| v.as_str());
        let case_insensitive = args.get("case_insensitive").and_then(|v| v.as_bool()).unwrap_or(false);

        if pattern.is_empty() {
            return Err(anyhow::anyhow!("Pattern is required"));
        }

        // Try ripgrep first, fall back to grep
        let mut cmd = Command::new("rg");
        cmd.arg("--json")
            .arg("--max-count").arg("50")
            .arg("--max-filesize").arg("1M");

        if case_insensitive {
            cmd.arg("-i");
        }

        if let Some(inc) = include {
            cmd.arg("--glob").arg(inc);
        }

        cmd.arg(pattern).arg(path);

        let result = timeout(Duration::from_secs(15), cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let mut matches = Vec::new();

                for line in stdout.lines() {
                    if let Ok(parsed) = serde_json::from_str::<Value>(line) {
                        if parsed.get("type").and_then(|v| v.as_str()) == Some("match") {
                            if let Some(data) = parsed.get("data") {
                                let file = data.get("path").and_then(|p| p.get("text")).and_then(|v| v.as_str()).unwrap_or("");
                                let line_num = data.get("line_number").and_then(|v| v.as_u64()).unwrap_or(0);
                                let text = data.get("lines").and_then(|l| l.get("text")).and_then(|v| v.as_str()).unwrap_or("");

                                matches.push(json!({
                                    "file": file,
                                    "line": line_num,
                                    "content": text.trim()
                                }));
                            }
                        }
                    }
                }

                Ok(json!({
                    "matches": matches,
                    "count": matches.len()
                }))
            }
            Ok(Err(e)) => {
                // ripgrep not found, try plain grep
                let mut fallback = Command::new("grep");
                fallback.arg("-rnI")
                    .arg("--max-count=50")
                    .arg(pattern)
                    .arg(path);

                if case_insensitive {
                    fallback.arg("-i");
                }

                let output = fallback.output().await?;
                let stdout = String::from_utf8_lossy(&output.stdout);
                let matches: Vec<Value> = stdout
                    .lines()
                    .take(50)
                    .filter_map(|line| {
                        let parts: Vec<&str> = line.splitn(3, ':').collect();
                        if parts.len() >= 3 {
                            Some(json!({
                                "file": parts[0],
                                "line": parts[1].parse::<u64>().unwrap_or(0),
                                "content": parts[2].trim()
                            }))
                        } else {
                            None
                        }
                    })
                    .collect();

                Ok(json!({
                    "matches": matches,
                    "count": matches.len(),
                    "fallback": "grep"
                }))
            }
            Err(_) => Ok(json!({
                "matches": [],
                "count": 0,
                "error": "Search timed out"
            })),
        }
    }
}
