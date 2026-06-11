use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use crate::agents::coordinator::CoordinatorAgent;
use crate::barq::BarqIndex;
use crate::providers::ProviderAdapter;
use crate::tools::ToolRegistry;
use super::{PermissionResult, Tool, ToolMetadata, ValidationResult};

pub struct DelegateTask {
    llm: Arc<dyn ProviderAdapter>,
    barq: Arc<BarqIndex>,
}

impl DelegateTask {
    pub fn new(llm: Arc<dyn ProviderAdapter>, barq: Arc<BarqIndex>) -> Self {
        Self { llm, barq }
    }
}

#[async_trait]
impl Tool for DelegateTask {
    fn name(&self) -> &'static str {
        "delegate_task"
    }

    fn description(&self) -> &'static str {
        "Delegate a complex task to the multi-agent swarm. \
         The swarm will decompose the goal and execute steps in parallel, returning the result."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "name": self.name(),
            "description": self.description(),
            "parameters": {
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "The complex task or goal to accomplish.",
                    }
                },
                "required": ["goal"]
            }
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let goal = args
            .get("goal")
            .and_then(|v| v.as_str())
            .unwrap_or_default();

        if goal.is_empty() {
            return Ok(serde_json::json!({"error": "No goal provided"}));
        }

        let task_tools = Arc::new(ToolRegistry::with_barq(self.barq.clone()));
        let coordinator =
            CoordinatorAgent::new(Arc::clone(&self.llm), self.barq.clone(), task_tools);

        match coordinator.execute_goal(goal).await {
            Ok(_) => Ok(serde_json::json!({
                "status": "success",
                "message": "Swarm successfully completed the goal."
            })),
            Err(e) => Ok(serde_json::json!({
                "status": "error",
                "message": format!("Swarm execution failed: {}", e)
            })),
        }
    }

    fn is_destructive(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { false }
    fn get_path(&self, _args: &Value) -> Option<String> { None }

    fn metadata(&self) -> ToolMetadata {
        ToolMetadata {
            max_result_size: 10_000,
            timeout_secs: 600,
            search_hint: Some("delegation swarm parallel agents multi-agent".to_string()),
        }
    }

    fn check_permissions(&self, _args: &Value) -> PermissionResult {
        PermissionResult::Ask(
            "Swarm delegation modifies multiple files automatically across multiple agents. \
             Allow swarm execution?"
                .to_string(),
        )
    }

    fn validate_input(&self, _args: &Value) -> ValidationResult {
        ValidationResult::Ok
    }
}
