use super::Tool;
use crate::barq::{BarqIndex, BarqResult};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;

pub struct BarqSearch {
    barq: Arc<BarqIndex>,
}

impl BarqSearch {
    pub fn new(barq: Arc<BarqIndex>) -> Self {
        Self { barq }
    }
}

#[async_trait]
impl Tool for BarqSearch {
    fn name(&self) -> &'static str {
        "barq_search"
    }

    fn description(&self) -> &'static str {
        "Semantic search over indexed codebase using BARQDB"
    }

    fn schema(&self) -> Value {
        json!({
            "query": "string",
            "top_k": "number",
            "filter_lang": "string"
        })
    }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
        let top_k = args.get("top_k").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
        let filter_lang = args.get("filter_lang").and_then(|v| v.as_str());

        let results: Vec<BarqResult> = self.barq.query(query, top_k);
        let filtered = if let Some(lang) = filter_lang {
            results
                .into_iter()
                .filter(|r| r.file_path.ends_with(lang))
                .collect::<Vec<_>>()
        } else {
            results
        };

        let json_results: Vec<Value> = filtered
            .into_iter()
            .map(|r| {
                json!({
                    "file": r.file_path,
                    "content": r.content,
                    "score": r.score,
                    "line": r.line,
                })
            })
            .collect();

        Ok(json!({
            "results": json_results
        }))
    }
}
