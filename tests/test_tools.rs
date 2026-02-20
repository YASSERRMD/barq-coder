use barqcoder::barq::BarqIndex;
use barqcoder::config::Config;
use barqcoder::tools::{Tool, ToolRegistry};
use serde_json::json;
use std::sync::Arc;

#[tokio::test]
async fn test_cargo_check_valid() {
    let registry = ToolRegistry::new();
    let tool = registry.get("cargo_check").unwrap();
    let args = json!({ "dir": "testdata" }); // We'll just run on barq-coder root since testdata isn't a crate, wait actually we can run on .
    // Running on current repo directory
    let args = json!({ "dir": "." });
    let res = tool.call(args).await.unwrap();
    assert_eq!(res["success"], true);
}

#[tokio::test]
async fn test_cargo_check_invalid() {
    let registry = ToolRegistry::new();
    let tool = registry.get("cargo_check").unwrap();
    let args = json!({ "dir": "testdata/bad_rust" });
    let res = tool.call(args).await.unwrap();
    assert_eq!(res["success"], false);
    assert!(!res["errors"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_edit_file_preview() {
    let registry = ToolRegistry::new();
    let tool = registry.get("edit_file").unwrap();
    let args = json!({
        "file_path": "testdata/sample.rs",
        "patch": "--- testdata/sample.rs\n+++ testdata/sample.rs\n@@ -1,1 +1,1 @@\n-fn main() {}\n+fn main() { println!(\"preview\"); }\n",
        "preview": true
    });
    let res = tool.call(args).await.unwrap();
    assert_eq!(res["success"], true);
    assert_eq!(res["applied"], false);
}

#[tokio::test]
async fn test_barq_search_returns() {
    let barq = Arc::new(BarqIndex::new(&Config::default()).unwrap());
    let registry = ToolRegistry::with_barq(barq);
    let tool = registry.get("barq_search").unwrap();
    let args = json!({ "query": "struct", "top_k": 10 });
    let res = tool.call(args).await.unwrap();
    let results = res["results"].as_array().unwrap();
    assert!(!results.is_empty());
}
