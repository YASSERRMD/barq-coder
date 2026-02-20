use async_trait::async_trait;
use serde_json::Value;

pub mod cargo_check;

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
            ],
        }
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
