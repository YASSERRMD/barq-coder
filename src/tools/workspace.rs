use super::Tool;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Workspace {
    pub path: String,
    pub name: String,
    pub barq_indexed: bool,
    pub last_indexed: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct WorkspaceData {
    pub workspaces: Vec<Workspace>,
    pub active: usize,
}

pub struct WorkspaceManager {
    store_path: PathBuf,
}

impl WorkspaceManager {
    pub fn new(root_dir: &str) -> Self {
        let path = PathBuf::from(root_dir).join(".barqcoder/workspaces.toml");
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        Self { store_path: path }
    }

    fn load_data(&self) -> WorkspaceData {
        if let Ok(content) = fs::read_to_string(&self.store_path) {
            if let Ok(data) = toml::from_str(&content) {
                return data;
            }
        }
        WorkspaceData::default()
    }

    fn save_data(&self, data: &WorkspaceData) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(data)?;
        fs::write(&self.store_path, content)?;
        Ok(())
    }

    pub fn add(&self, path: &str) -> anyhow::Result<()> {
        let mut data = self.load_data();
        if data.workspaces.iter().any(|w| w.path == path) {
            return Err(anyhow::anyhow!("Workspace already exists"));
        }
        let name = PathBuf::from(path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "unnamed".to_string());
        
        data.workspaces.push(Workspace {
            path: path.to_string(),
            name,
            barq_indexed: false,
            last_indexed: None,
        });
        
        self.save_data(&data)
    }

    pub fn remove(&self, path: &str) -> anyhow::Result<()> {
        let mut data = self.load_data();
        let idx = data.workspaces.iter().position(|w| w.path == path);
        
        if let Some(i) = idx {
            data.workspaces.remove(i);
            if data.active >= data.workspaces.len() {
                data.active = 0;
            }
            self.save_data(&data)
        } else {
            Err(anyhow::anyhow!("Workspace not found"))
        }
    }

    pub fn switch(&self, path: &str) -> anyhow::Result<()> {
        let mut data = self.load_data();
        if let Some(idx) = data.workspaces.iter().position(|w| w.path == path) {
            data.active = idx;
            self.save_data(&data)
        } else {
            Err(anyhow::anyhow!("Workspace not found"))
        }
    }

    pub fn list(&self) -> Vec<Workspace> {
        self.load_data().workspaces
    }

    pub fn active_workspace(&self) -> Option<Workspace> {
        let data = self.load_data();
        if data.workspaces.is_empty() {
            None
        } else {
            Some(data.workspaces[data.active].clone())
        }
    }
}

pub struct WorkspaceTool {
    manager: WorkspaceManager,
}

impl WorkspaceTool {
    pub fn new(root_dir: &str) -> Self {
        Self {
            manager: WorkspaceManager::new(root_dir),
        }
    }
}

#[async_trait]
impl Tool for WorkspaceTool {
    fn name(&self) -> &'static str {
        "manage_workspace"
    }

    fn description(&self) -> &'static str {
        "Add/remove/switch workspaces"
    }

    fn schema(&self) -> Value {
        json!({
            "action": "string",
            "path": "string"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("");
        
        match action {
            "add" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                self.manager.add(path)?;
                Ok(json!({"success": true, "message": "Workspace added"}))
            }
            "remove" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                self.manager.remove(path)?;
                Ok(json!({"success": true, "message": "Workspace removed"}))
            }
            "switch" => {
                let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");
                self.manager.switch(path)?;
                Ok(json!({"success": true, "message": "Workspace switched"}))
            }
            "list" => {
                let workspaces = self.manager.list();
                let active = self.manager.active_workspace();
                Ok(json!({
                    "workspaces": workspaces,
                    "active": active
                }))
            }
            _ => Err(anyhow::anyhow!("Invalid action. Must be add, remove, switch, or list.")),
        }
    }
}
