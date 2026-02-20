use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use crate::barq::BarqIndex;

pub mod cargo_check;
pub mod barq_search;
pub mod edit_file;
pub mod shell;
pub mod file_ops;

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> Value;
    async fn call(&self, args: Value) -> anyhow::Result<Value>;
}

pub struct ToolRegistry {
    pub tools: Vec<Box<dyn Tool + Send + Sync>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: vec![
                Box::new(cargo_check::CargoCheck),
                Box::new(edit_file::EditFile),
                Box::new(shell::ShellExec),
                Box::new(shell::GitTool),
                Box::new(file_ops::ReadFile),
                Box::new(file_ops::ListFiles),
                Box::new(file_ops::CreateFile),
            ],
        }
    }

    pub fn with_barq(barq: Arc<BarqIndex>) -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(barq_search::BarqSearch::new(barq)));
        registry
    }

    pub fn register(&mut self, tool: Box<dyn Tool + Send + Sync>) {
        self.tools.push(tool);
    }

    pub fn get(&self, name: &str) -> Option<&Box<dyn Tool + Send + Sync>> {
        self.tools.iter().find(|t| t.name() == name)
    }

    pub fn schemas(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.schema()).collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
