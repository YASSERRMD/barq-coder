use serde::{Deserialize, Serialize};

pub mod planner;
pub mod coder;
pub mod tester;
pub mod reviewer;
pub mod coordinator;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    Planner,
    Coder,
    Tester,
    Reviewer,
    Coordinator,
}

impl AgentRole {
    pub fn system_prompt(&self) -> &'static str {
        match self {
            Self::Planner => "You are the Planner agent. Your job is to decompose the task into a logical graph of executable steps.",
            Self::Coder => "You are the Coder agent. Your job is to implement the specific step using the BARQ context provided.",
            Self::Tester => "You are the Tester agent. Your job is to write and run tests to verify the Coder's implementation.",
            Self::Reviewer => "You are the Reviewer agent. Your job is to review the code diffs for correctness, performance, and security before they are applied.",
            Self::Coordinator => "You are the Coordinator agent. Your job is to route tasks between agents and merge their results."
        }
    }
}
