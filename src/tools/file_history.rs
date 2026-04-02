use super::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::sync::Mutex;

/// FileHistoryTool — undo/redo for file edits (like Claude Code's FileHistoryState).
pub struct FileHistoryTool {
    snapshots: Mutex<HashMap<String, Vec<String>>>,
}

impl FileHistoryTool {
    pub fn new() -> Self {
        Self {
            snapshots: Mutex::new(HashMap::new()),
        }
    }

    /// Save a snapshot of the file before editing.
    pub fn save_snapshot(&self, path: &str) {
        if let Ok(content) = fs::read_to_string(path) {
            let mut snaps = self.snapshots.lock().unwrap();
            snaps.entry(path.to_string()).or_default().push(content);
        }
    }
}

impl Default for FileHistoryTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for FileHistoryTool {
    fn name(&self) -> &'static str { "file_history" }

    fn description(&self) -> &'static str {
        "Undo file edits by restoring previous snapshots"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "action": { "type": "string", "description": "Action: 'undo', 'list'" },
                "path": { "type": "string", "description": "File path" }
            },
            "required": ["action", "path"]
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

        match action {
            "undo" => {
                let mut snaps = self.snapshots.lock().unwrap();
                if let Some(history) = snaps.get_mut(path) {
                    if let Some(prev) = history.pop() {
                        fs::write(path, &prev)?;
                        Ok(json!({
                            "success": true,
                            "restored": true,
                            "remaining_snapshots": history.len()
                        }))
                    } else {
                        Ok(json!({ "success": false, "error": "No snapshots available" }))
                    }
                } else {
                    Ok(json!({ "success": false, "error": "No history for this file" }))
                }
            }
            "list" => {
                let snaps = self.snapshots.lock().unwrap();
                let count = snaps.get(path).map(|h| h.len()).unwrap_or(0);
                Ok(json!({
                    "path": path,
                    "snapshot_count": count
                }))
            }
            _ => Err(anyhow::anyhow!("Unknown action: {}", action)),
        }
    }
}
