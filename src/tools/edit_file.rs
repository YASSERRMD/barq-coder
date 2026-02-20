use super::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use tokio::process::Command;

pub struct EditFile;

#[async_trait]
impl Tool for EditFile {
    fn name(&self) -> &'static str {
        "edit_file"
    }

    fn description(&self) -> &'static str {
        "Apply a unified diff patch to a file"
    }

    fn schema(&self) -> Value {
        json!({
            "file_path": "string",
            "patch": "string",
            "preview": "bool"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        let patch = args.get("patch").and_then(|v| v.as_str()).unwrap_or("");
        let preview = args
            .get("preview")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        if preview {
            return Ok(json!({
                "success": true,
                "applied": false,
                "reverted": false,
                "errors": Vec::<String>::new(),
                "diff": patch
            }));
        }

        let original_content = fs::read_to_string(file_path).unwrap_or_default();

        let patch_tmp = format!("{}.patch.tmp", file_path);
        fs::write(&patch_tmp, patch)?;

        let mut cmd = Command::new("patch");
        cmd.arg("-u").arg(file_path).arg("-i").arg(&patch_tmp);
        let output = cmd.output().await?;

        let _ = fs::remove_file(&patch_tmp);

        if !output.status.success() {
            return Ok(json!({
                "success": false,
                "applied": false,
                "reverted": false,
                "errors": vec![String::from_utf8_lossy(&output.stderr).to_string()],
            }));
        }

        let check_cmd = Command::new("cargo")
            .arg("check")
            .arg("--message-format")
            .arg("json")
            .output()
            .await?;

        if !check_cmd.status.success() {
            // Revert
            fs::write(file_path, original_content)?;
            return Ok(json!({
                "success": false,
                "applied": false,
                "reverted": true,
                "errors": vec![String::from_utf8_lossy(&check_cmd.stderr).to_string()],
            }));
        }

        Ok(json!({
            "success": true,
            "applied": true,
            "reverted": false,
            "errors": Vec::<String>::new(),
        }))
    }
}
