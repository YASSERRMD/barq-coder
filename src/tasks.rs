use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: TaskStatus,
    pub assigned_to: Option<String>,
    pub dependencies: Vec<String>,
}

pub struct TaskBoard {
    tasks: RwLock<HashMap<String, Task>>,
}

impl TaskBoard {
    pub fn new() -> Self {
        Self {
            tasks: RwLock::new(HashMap::new()),
        }
    }

    pub fn create_task(&self, id: String, description: String, dependencies: Vec<String>) -> Result<(), String> {
        let mut tasks = self.tasks.write().map_err(|_| "PoisonError")?;
        if tasks.contains_key(&id) {
            return Err(format!("Task {} already exists", id));
        }
        
        let task = Task {
            id: id.clone(),
            description,
            status: TaskStatus::Pending,
            assigned_to: None,
            dependencies,
        };
        tasks.insert(id, task);
        Ok(())
    }

    pub fn update_status(&self, id: &str, status: TaskStatus) -> Result<(), String> {
        let mut tasks = self.tasks.write().map_err(|_| "PoisonError")?;
        if let Some(task) = tasks.get_mut(id) {
            task.status = status;
            Ok(())
        } else {
            Err(format!("Task {} not found", id))
        }
    }

    pub fn assign_task(&self, id: &str, agent: &str) -> Result<(), String> {
        let mut tasks = self.tasks.write().map_err(|_| "PoisonError")?;
        if let Some(task) = tasks.get_mut(id) {
            task.assigned_to = Some(agent.to_string());
            Ok(())
        } else {
            Err(format!("Task {} not found", id))
        }
    }

    pub fn get_task(&self, id: &str) -> Result<Task, String> {
        let tasks = self.tasks.read().map_err(|_| "PoisonError")?;
        tasks.get(id).cloned().ok_or_else(|| format!("Task {} not found", id))
    }

    pub fn list_tasks(&self) -> Result<Vec<Task>, String> {
        let tasks = self.tasks.read().map_err(|_| "PoisonError")?;
        Ok(tasks.values().cloned().collect())
    }

    pub fn are_dependencies_met(&self, id: &str) -> Result<bool, String> {
        let tasks = self.tasks.read().map_err(|_| "PoisonError")?;
        let task = tasks.get(id).ok_or_else(|| format!("Task {} not found", id))?;
        
        for dep_id in &task.dependencies {
            let dep_task = tasks.get(dep_id).ok_or_else(|| format!("Dependency {} not found", dep_id))?;
            if dep_task.status != TaskStatus::Completed {
                return Ok(false);
            }
        }
        
        Ok(true)
    }
}
