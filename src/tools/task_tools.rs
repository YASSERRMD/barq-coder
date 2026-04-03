use crate::tasks::{TaskBoard, TaskStatus};
use crate::tools::{Tool, ToolMetadata};
use serde_json::Value;
use std::sync::Arc;

pub struct TaskCreateTool {
    task_board: Arc<TaskBoard>,
}

impl TaskCreateTool {
    pub fn new(task_board: Arc<TaskBoard>) -> Self {
        Self { task_board }
    }
}

pub struct TaskUpdateTool {
    task_board: Arc<TaskBoard>,
}

impl TaskUpdateTool {
    pub fn new(task_board: Arc<TaskBoard>) -> Self {
        Self { task_board }
    }
}

pub struct TaskListTool {
    task_board: Arc<TaskBoard>,
}

impl TaskListTool {
    pub fn new(task_board: Arc<TaskBoard>) -> Self {
        Self { task_board }
    }
}

#[async_trait::async_trait]
impl Tool for TaskCreateTool {
    fn name(&self) -> &'static str {
        "task_create"
    }

    fn description(&self) -> &'static str {
        "Creates a new task on the agent task board. Use this to break down complex goals into tracking items."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string", "description": "Unique identifier for the task" },
                "description": { "type": "string", "description": "What needs to be done" },
                "dependencies": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "List of task IDs that must be completed before this one"
                }
            },
            "required": ["id", "description"]
        })
    }

    fn is_concurrent_safe(&self) -> bool { true }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let id = args.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing id"))?
            .to_string();
        let description = args.get("description")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("Missing description"))?
            .to_string();
        
        let deps = if let Some(arr) = args.get("dependencies").and_then(|v| v.as_array()) {
            arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect()
        } else {
            Vec::new()
        };

        self.task_board.create_task(id.clone(), description, deps).map_err(|e: String| anyhow::anyhow!(e))?;
        Ok(serde_json::json!({ "status": "success", "task_id": id }))
    }
}

#[async_trait::async_trait]
impl Tool for TaskUpdateTool {
    fn name(&self) -> &'static str {
        "task_update"
    }

    fn description(&self) -> &'static str {
        "Update the status or assignee of an existing task on the task board."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "id": { "type": "string" },
                "status": { "type": "string", "enum": ["Pending", "InProgress", "Completed", "Failed"] },
                "assignee": { "type": "string" }
            },
            "required": ["id"]
        })
    }

    fn is_concurrent_safe(&self) -> bool { true }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let id = args.get("id").and_then(|v| v.as_str()).ok_or_else(|| anyhow::anyhow!("Missing id"))?;
        
        if let Some(status_str) = args.get("status").and_then(|v| v.as_str()) {
            let status = match status_str {
                "Pending" => TaskStatus::Pending,
                "InProgress" => TaskStatus::InProgress,
                "Completed" => TaskStatus::Completed,
                "Failed" => TaskStatus::Failed,
                _ => return Err(anyhow::anyhow!("Invalid status")),
            };
            self.task_board.update_status(id, status).map_err(|e: String| anyhow::anyhow!(e))?;
        }

        if let Some(assignee) = args.get("assignee").and_then(|v| v.as_str()) {
            self.task_board.assign_task(id, assignee).map_err(|e: String| anyhow::anyhow!(e))?;
        }

        Ok(serde_json::json!({ "status": "success" }))
    }
}

#[async_trait::async_trait]
impl Tool for TaskListTool {
    fn name(&self) -> &'static str {
        "task_list"
    }

    fn description(&self) -> &'static str {
        "List all tasks on the task board to see current progress."
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {}
        })
    }

    fn is_concurrent_safe(&self) -> bool { true }
    fn is_read_only(&self) -> bool { true }

    async fn call(&self, _args: Value) -> anyhow::Result<Value> {
        let tasks = self.task_board.list_tasks().map_err(|e: String| anyhow::anyhow!(e))?;
        Ok(serde_json::json!({ "tasks": tasks }))
    }
}
