use super::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::fs;
use std::path::Path;
use walkdir::WalkDir;

pub struct ReadFile;

#[async_trait]
impl Tool for ReadFile {
    fn name(&self) -> &'static str {
        "read_file"
    }

    fn description(&self) -> &'static str {
        "Read complete file content"
    }

    fn schema(&self) -> Value {
        json!({
            "path": "string",
            "start_line": "number",
            "end_line": "number"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        
        let content = fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read file: {}", e))?;
            
        let metadata = fs::metadata(path)?;
        let size_bytes = metadata.len();
        
        let lines: Vec<&str> = content.lines().collect();
        let line_count = lines.len();
        
        // Optional line slicing
        let mut final_content = content.clone();
        if let Some(start) = args.get("start_line").and_then(|v| v.as_u64()) {
            let start_idx = start as usize;
            let end_idx = args.get("end_line").and_then(|v| v.as_u64()).map(|v| v as usize).unwrap_or(line_count);
            
            if start_idx <= line_count && start_idx <= end_idx {
                let end = std::cmp::min(end_idx, line_count);
                let start_idx = start_idx.saturating_sub(1); // 1-indexed to 0-indexed loosely handled
                final_content = lines.into_iter().skip(start_idx).take(end - start_idx).collect::<Vec<_>>().join("\n");
            }
        }

        Ok(json!({
            "content": final_content,
            "line_count": line_count,
            "size_bytes": size_bytes
        }))
    }
}

pub struct ListFiles;

#[async_trait]
impl Tool for ListFiles {
    fn name(&self) -> &'static str {
        "list_files"
    }

    fn description(&self) -> &'static str {
        "List files in directory"
    }

    fn schema(&self) -> Value {
        json!({
            "path": "string",
            "extension": "string"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
        let extension = args.get("extension").and_then(|v| v.as_str());

        let mut files = Vec::new();

        for entry in WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                let pass = if let Some(ext) = extension {
                    entry.path().extension().map(|e| e.to_string_lossy().to_string()) == Some(ext.to_string())
                } else {
                    true
                };

                if pass {
                    if let Ok(metadata) = entry.metadata() {
                        let sys_time = metadata.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH);
                        let duration = sys_time.duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_secs();
                        
                        files.push(json!({
                            "path": entry.path().to_string_lossy().to_string(),
                            "size": metadata.len(),
                            "modified": duration
                        }));
                    }
                }
            }
        }

        Ok(json!({
            "files": files
        }))
    }
}

pub struct CreateFile;

#[async_trait]
impl Tool for CreateFile {
    fn name(&self) -> &'static str {
        "create_file"
    }

    fn description(&self) -> &'static str {
        "Create new file with content"
    }

    fn schema(&self) -> Value {
        json!({
            "path": "string",
            "content": "string"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let content = args.get("content").and_then(|v| v.as_str()).unwrap_or("");
        
        let p = Path::new(path_str);
        if p.exists() {
            return Err(anyhow::anyhow!("File already exists: {}", path_str));
        }

        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        
        fs::write(p, content)?;

        Ok(json!({
            "created": true,
            "path": path_str
        }))
    }
}
