use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct DiagnosticRule {
    pub id: String,
    pub description: String,
    pub severity: String,
}

pub fn load_rules() -> Vec<DiagnosticRule> {
    // In reality this would load from a rules.yaml file
    vec![
        DiagnosticRule {
            id: "UNSAFE_BLOCK".to_string(),
            description: "Unsafe block detected and requires manual review".to_string(),
            severity: "warn".to_string(),
        },
        DiagnosticRule {
            id: "O_N_SQUARED".to_string(),
            description: "Nested loop detected causing potential O(n^2) perf".to_string(),
            severity: "warn".to_string(),
        },
        DiagnosticRule {
            id: "CLONE_IN_LOOP".to_string(),
            description: ".clone() called inside a loop".to_string(),
            severity: "error".to_string(),
        }
    ]
}
