use super::{Tool, PermissionResult, ToolMetadata};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use tokio::process::Command;

/// EditFile tool — supports both unified diff and string-replace editing.
/// Inspired by Claude Code's FileEditTool which uses string-replace semantics.
pub struct EditFile;

#[async_trait]
impl Tool for EditFile {
    fn name(&self) -> &'static str { "edit_file" }

    fn description(&self) -> &'static str {
        "Edit a file using string replacement or unified diff patch. Runs cargo check after applying."
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string", "description": "Path to the file to edit" },
                "old_string": { "type": "string", "description": "The exact string to find and replace" },
                "new_string": { "type": "string", "description": "The replacement string" },
                "patch": { "type": "string", "description": "Alternative: unified diff patch to apply" },
                "preview": { "type": "boolean", "description": "If true, show diff only without applying" }
            },
            "required": ["file_path"]
        })
    }

    fn is_destructive(&self) -> bool { true }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            timeout_secs: 30,
            search_hint: Some("modify change update write replace content".to_string()),
            ..Default::default()
        }
    }

    fn check_permissions(&self, args: &Value) -> PermissionResult {
        let path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        // Block editing system files
        if path.starts_with("/etc") || path.starts_with("/usr") || path.starts_with("/bin") {
            return PermissionResult::Deny(format!("Cannot edit system path: {}", path));
        }
        PermissionResult::Allow
    }

    fn get_path(&self, args: &Value) -> Option<String> {
        args.get("file_path").and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let file_path = args.get("file_path").and_then(|v| v.as_str()).unwrap_or("");
        let preview = args.get("preview").and_then(|v| v.as_bool()).unwrap_or(false);

        // String-replace mode (preferred — like Claude Code's FileEditTool)
        if let (Some(old_str), Some(new_str)) = (
            args.get("old_string").and_then(|v| v.as_str()),
            args.get("new_string").and_then(|v| v.as_str()),
        ) {
            return self.string_replace(file_path, old_str, new_str, preview).await;
        }

        // Unified diff mode (fallback)
        if let Some(patch) = args.get("patch").and_then(|v| v.as_str()) {
            return self.apply_patch(file_path, patch, preview).await;
        }

        Err(anyhow::anyhow!("Must provide either old_string/new_string or patch"))
    }
}

impl EditFile {
    /// String-replace editing — find exact match and replace.
    async fn string_replace(&self, file_path: &str, old_str: &str, new_str: &str, preview: bool) -> anyhow::Result<Value> {
        let original = fs::read_to_string(file_path)
            .map_err(|e| anyhow::anyhow!("Cannot read {}: {}", file_path, e))?;

        let occurrences = original.matches(old_str).count();
        if occurrences == 0 {
            return Ok(json!({
                "success": false,
                "applied": false,
                "error": "old_string not found in file"
            }));
        }

        if occurrences > 1 {
            return Ok(json!({
                "success": false,
                "applied": false,
                "error": format!("old_string found {} times — must be unique. Add more context lines.", occurrences)
            }));
        }

        let patched = original.replacen(old_str, new_str, 1);

        if preview {
            return Ok(json!({
                "success": true,
                "applied": false,
                "preview": true,
                "diff": format!("-{}\n+{}", old_str, new_str)
            }));
        }

        // Apply and verify
        fs::write(file_path, &patched)?;

        // Run cargo check
        let check = Command::new("cargo")
            .arg("check")
            .arg("--message-format")
            .arg("json")
            .output()
            .await;

        if let Ok(output) = check {
            if !output.status.success() {
                // Revert
                fs::write(file_path, &original)?;
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                return Ok(json!({
                    "success": false,
                    "applied": false,
                    "reverted": true,
                    "errors": [stderr]
                }));
            }
        }

        Ok(json!({
            "success": true,
            "applied": true,
            "reverted": false,
            "occurrences": 1
        }))
    }

    /// Unified diff patch mode (legacy).
    async fn apply_patch(&self, file_path: &str, patch: &str, preview: bool) -> anyhow::Result<Value> {
        if preview {
            return Ok(json!({
                "success": true,
                "applied": false,
                "preview": true,
                "diff": patch
            }));
        }

        let original = fs::read_to_string(file_path).unwrap_or_default();

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
                "errors": [String::from_utf8_lossy(&output.stderr).to_string()]
            }));
        }

        // Verify with cargo check
        let check = Command::new("cargo")
            .arg("check")
            .arg("--message-format")
            .arg("json")
            .output()
            .await?;

        if !check.status.success() {
            fs::write(file_path, original)?;
            return Ok(json!({
                "success": false,
                "applied": false,
                "reverted": true,
                "errors": [String::from_utf8_lossy(&check.stderr).to_string()]
            }));
        }

        Ok(json!({
            "success": true,
            "applied": true,
            "reverted": false
        }))
    }
}
