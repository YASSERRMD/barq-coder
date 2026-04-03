use crate::tools::{Tool, ToolMetadata};
use serde_json::Value;
use std::process::Command;

pub struct PythonTool;

#[async_trait::async_trait]
impl Tool for PythonTool {
    fn name(&self) -> &'static str {
        "python_repl"
    }

    fn description(&self) -> &'static str {
        "Execute inline Python code and return the standard output and standard error. Useful for calculating math, formatting strings, or running simple scripts."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "The Python code to execute" }
            },
            "required": ["code"]
        })
    }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            max_result_size: 50_000,
            timeout_secs: 15,
            search_hint: Some("execute python script repl math logic".to_string()),
        }
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let code = args
            .get("code")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing code"))?;

        let output = Command::new("python3")
            .arg("-c")
            .arg(code)
            .output()
            .map_err(|e| anyhow::anyhow!("Failed to execute python3: {}", e))?;

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(serde_json::json!({
            "stdout": stdout,
            "stderr": stderr,
            "exit_code": output.status.code(),
        }))
    }
}
