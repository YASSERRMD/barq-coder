use crate::tools::{Tool, ToolMetadata};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;

pub struct NotebookEditTool;

#[async_trait::async_trait]
impl Tool for NotebookEditTool {
    fn name(&self) -> &'static str {
        "notebook_edit"
    }

    fn description(&self) -> &'static str {
        "Edit a Jupyter Notebook (.ipynb) cell. You can replace the source of a cell given its index or cell_id."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "notebook_path": { "type": "string", "description": "Path to the notebook file" },
                "cell_index": { "type": "integer", "description": "The 0-based index of the cell to modify" },
                "new_source": { "type": "string", "description": "The new source code/text for the cell" },
                "cell_type": { "type": "string", "enum": ["code", "markdown"], "description": "Type of the cell" }
            },
            "required": ["notebook_path", "cell_index", "new_source"]
        })
    }

    fn is_destructive(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { false }
    fn is_read_only(&self) -> bool { false }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            max_result_size: 50_000,
            timeout_secs: 10,
            search_hint: Some("jupyter notebook format edit ipynb".to_string()),
        }
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let path_str = args
            .get("notebook_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing notebook_path"))?;
        let cell_index = args
            .get("cell_index")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("Missing cell_index"))? as usize;
        let new_source = args
            .get("new_source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing new_source"))?;
        
        let path = PathBuf::from(path_str);
        if !path.exists() {
            return Err(anyhow::anyhow!("Notebook file does not exist."));
        }

        let content = fs::read_to_string(&path)?;
        let mut notebook: Value = serde_json::from_str(&content)?;

        if let Some(cells) = notebook.get_mut("cells").and_then(|v| v.as_array_mut()) {
            if cell_index < cells.len() {
                let cell = &mut cells[cell_index];
                
                // Update source (Jupyter notebooks often store source as Array of strings, 
                // but single string is also valid for Jupyter frontends, or we can split it).
                // To be perfectly safe, we split by newline and add \n.
                let mut lines: Vec<String> = new_source.split('\n').map(|s| format!("{}\n", s)).collect();
                if let Some(last) = lines.last_mut() {
                    if last.ends_with('\n') {
                        let len = last.len();
                        last.truncate(len - 1); // remove trailing newline from last line natively
                    }
                }
                
                cell["source"] = serde_json::json!(lines);

                if let Some(c_type) = args.get("cell_type").and_then(|v| v.as_str()) {
                    cell["cell_type"] = serde_json::json!(c_type);
                }

                // If code, clear outputs and execution count
                if cell.get("cell_type").and_then(|v| v.as_str()) == Some("code") {
                    cell["outputs"] = serde_json::json!([]);
                    cell["execution_count"] = serde_json::Value::Null;
                }
            } else {
                return Err(anyhow::anyhow!("Cell index {} is out of bounds (total parts: {})", cell_index, cells.len()));
            }
        } else {
            return Err(anyhow::anyhow!("Invalid notebook structure: missing 'cells' array."));
        }

        let new_content = serde_json::to_string_pretty(&notebook)?;
        fs::write(&path, new_content)?;

        Ok(serde_json::json!({
            "status": "success",
            "message": format!("Updated cell {} in {}", cell_index, path_str)
        }))
    }
}
