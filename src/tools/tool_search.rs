use super::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};

/// ToolSearchTool — lets the model search for tools by keyword
/// (like Claude Code's ToolSearchTool for deferred tools).
pub struct ToolSearchTool;

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &'static str { "tool_search" }

    fn description(&self) -> &'static str {
        "Search for available tools by keyword or capability description"
    }

    fn schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Keyword to search for tools" }
            },
            "required": ["query"]
        })
    }

    fn is_read_only(&self) -> bool { true }
    fn is_concurrent_safe(&self) -> bool { true }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        // This tool needs access to the registry. Since we can't hold a reference,
        // we return a hardcoded list of all tools. The orchestrator should ideally
        // inject this dynamically.
        //
        // In a production implementation, this would be wired through ToolUseContext.
        let all_tools = vec![
            ("cargo_check", "Run cargo check on a Rust project"),
            ("edit_file", "Apply a unified diff patch to a file"),
            ("shell_exec", "Run shell command in sandboxed workspace"),
            ("git_ops", "Git operations: status/diff/log/add/commit"),
            ("read_file", "Read complete file content"),
            ("list_files", "List files in directory"),
            ("create_file", "Create new file with content"),
            ("manage_workspace", "Add/remove/switch workspaces"),
            ("cargo_bench", "Run cargo bench on a Rust project"),
            ("barq_search", "Semantic search over indexed codebase"),
            ("glob", "Find files matching a glob pattern"),
            ("grep", "Search file contents with ripgrep"),
            ("web_fetch", "Fetch content from a URL"),
            ("file_history", "Undo file edits by restoring snapshots"),
        ];

        let query_lower = query.to_lowercase();
        let matches: Vec<Value> = all_tools
            .iter()
            .filter(|(name, desc)| {
                name.to_lowercase().contains(&query_lower)
                    || desc.to_lowercase().contains(&query_lower)
            })
            .map(|(name, desc)| json!({ "name": name, "description": desc }))
            .collect();

        Ok(json!({
            "query": query,
            "results": matches,
            "count": matches.len()
        }))
    }
}
