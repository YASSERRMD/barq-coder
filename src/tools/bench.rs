use async_trait::async_trait;
use serde_json::{json, Value};
use tokio::process::Command;
use crate::tools::Tool;

pub mod baseline;
pub mod compare;
pub mod regress;
pub mod suggest;
pub mod profile;

pub struct CargoBench;

#[async_trait]
impl Tool for CargoBench {
    fn name(&self) -> &'static str {
        "cargo_bench"
    }

    fn description(&self) -> &'static str {
        "Run cargo bench in the given directory"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "dir": {
                    "type": "string",
                    "description": "Directory to run cargo bench in"
                }
            }
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let dir = args.get("dir").and_then(|v| v.as_str()).unwrap_or(".");
        let mut cmd = Command::new("cargo");
        cmd.arg("bench").current_dir(dir);

        let out = cmd.output().await?;
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();

        Ok(json!({
            "success": out.status.success(),
            "stdout": stdout,
            "stderr": stderr
        }))
    }
}
