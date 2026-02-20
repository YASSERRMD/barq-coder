use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct MacroGoalFile {
    pub name: String,
    pub description: String,
    pub phases: Vec<Phase>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Phase {
    pub name: String,
    pub description: String,
    pub tasks: Vec<Task>,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub description: String,
    pub status: String,
    pub command: Option<String>,
}

pub struct GoalManager {
    goals_dir: PathBuf,
}

impl GoalManager {
    pub fn new(workspace: &str) -> Self {
        let dir = Path::new(workspace).join(".barqcoder/goals");
        let _ = fs::create_dir_all(&dir);
        Self { goals_dir: dir }
    }

    pub fn load_goal(&self, name: &str) -> anyhow::Result<MacroGoalFile> {
        let path = self.goals_dir.join(format!("{}.yaml", name));
        let content = fs::read_to_string(path)?;
        let goal: MacroGoalFile = serde_yaml::from_str(&content)?;
        Ok(goal)
    }

    pub fn save_goal(&self, goal: &MacroGoalFile) -> anyhow::Result<()> {
        let content = serde_yaml::to_string(goal)?;
        let path = self.goals_dir.join(format!("{}.yaml", goal.name));
        fs::write(path, content)?;
        Ok(())
    }

    pub fn list_goals(&self) -> Vec<String> {
        let mut goals = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.goals_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Some(name) = entry.file_name().to_str() {
                    if name.ends_with(".yaml") {
                        goals.push(name.trim_end_matches(".yaml").to_string());
                    }
                }
            }
        }
        goals
    }
}
