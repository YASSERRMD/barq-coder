use super::{Tool, ToolMetadata};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::path::Path;

/// GlobTool — find files matching a glob pattern (like Claude Code's GlobTool).
pub struct GlobTool;

#[async_trait]
impl Tool for GlobTool {
    fn name(&self) -> &'static str { "glob" }

    fn description(&self) -> &'static str {
        "Find files matching a glob pattern in the workspace"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. '**/*.rs')" },
                "path": { "type": "string", "description": "Root directory to search from (default '.')" }
            },
            "required": ["pattern"]
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            search_hint: Some("find files pattern match directory".to_string()),
            ..Default::default()
        }
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("**/*");
        let root = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let full_pattern = if Path::new(root).join(pattern).to_string_lossy().contains("**") {
            format!("{}/{}", root, pattern)
        } else {
            format!("{}/{}", root, pattern)
        };

        let mut matches = Vec::new();
        for entry in glob::glob(&full_pattern).map_err(|e| anyhow::anyhow!("Invalid glob: {}", e))? {
            match entry {
                Ok(path) => {
                    if matches.len() >= 1000 {
                        break;
                    }
                    matches.push(path.to_string_lossy().to_string());
                }
                Err(_) => continue,
            }
        }

        Ok(json!({
            "matches": matches,
            "count": matches.len(),
            "truncated": matches.len() >= 1000
        }))
    }
}
