use crate::barq::BarqIndex;
use std::sync::Arc;

pub fn suggest_fix(barq: Arc<BarqIndex>, fn_name: &str) -> Vec<String> {
    let query = format!("perf fix for {}", fn_name);
    let results = barq.query(&query, 3);
    results.into_iter().map(|r| r.content).collect()
}
