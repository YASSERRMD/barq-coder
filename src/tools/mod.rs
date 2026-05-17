use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use crate::barq::BarqIndex;
use crate::tasks::TaskBoard;

pub mod cargo_check;
pub mod barq_search;
pub mod edit_file;
pub mod shell;
pub mod file_ops;
pub mod workspace;
pub mod bench;
pub mod glob;
pub mod grep;
pub mod web_fetch;
pub mod file_history;
pub mod tool_search;
pub mod delegate;
pub mod task_tools;
pub mod notebook_edit;
pub mod python_tool;

// ─────────────────────────────────────────────────────────────────────────────
// Permission result from tool-specific permission checks.
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum PermissionResult {
    /// Tool is allowed to execute.
    Allow,
    /// Tool is denied with a reason.
    Deny(String),
    /// Tool requires interactive confirmation from the user.
    Ask(String),
}

// ─────────────────────────────────────────────────────────────────────────────
// Validation result for tool input.
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Ok,
    Error { message: String, code: i32 },
}

// ─────────────────────────────────────────────────────────────────────────────
// Risk classification for sandboxing and explicit safe mode.
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandRisk {
    /// Read-only operation, safe to run without specific approval.
    ReadOnly,
    /// Mutates state (writes to files, builds things) but is not necessarily destructive.
    Mutating,
    /// Destructive operation (rm, drop, massive rewrites, remote execution).
    /// Requires explicit elevated permission.
    Destructive,
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool metadata — describes capabilities and limits for the harness.
// ─────────────────────────────────────────────────────────────────────────────
#[derive(Debug, Clone)]
pub struct ToolMetadata {
    /// Maximum characters in the tool result before it is truncated.
    pub max_result_size: usize,
    /// Timeout in seconds for tool execution.
    pub timeout_secs: u64,
    /// A search hint for ToolSearch keyword matching.
    pub search_hint: Option<String>,
}

impl Default for ToolMetadata {
    fn default() -> Self {
        Self {
            max_result_size: 100_000,
            timeout_secs: 30,
            search_hint: None,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The Tool trait — every tool implements this.
// Inspired by Claude Code's buildTool() pattern.
// ─────────────────────────────────────────────────────────────────────────────
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name used in dispatching (e.g. "cargo_check").
    fn name(&self) -> &'static str;

    /// Human-readable description used in system prompts.
    fn description(&self) -> &'static str;

    /// JSON Schema for the tool's input parameters.
    fn schema(&self) -> Value;

    /// Execute the tool with the given arguments.
    async fn call(&self, args: Value) -> anyhow::Result<Value>;

    // ── Capability Flags (defaults provided) ──

    /// Whether the tool is currently enabled.
    fn is_enabled(&self) -> bool { true }

    /// Whether the tool can run in parallel with other tools safely.
    fn is_concurrent_safe(&self) -> bool { false }

    /// Whether the tool is read-only (no side effects).
    fn is_read_only(&self) -> bool { false }

    /// Whether the tool performs irreversible / destructive operations.
    fn is_destructive(&self) -> bool { false }

    /// Return the risk classification of the tool.
    fn risk(&self) -> CommandRisk {
        if self.is_destructive() {
             CommandRisk::Destructive
        } else if self.is_read_only() {
             CommandRisk::ReadOnly
        } else {
             CommandRisk::Mutating
        }
    }

    /// Metadata for the tool harness.
    fn metadata(&self) -> ToolMetadata { ToolMetadata::default() }

    // ── Validation & Permissions ──

    /// Validate the input before any permission check.
    /// Default: always valid.
    fn validate_input(&self, _args: &Value) -> ValidationResult {
        ValidationResult::Ok
    }

    /// Tool-specific permission check (e.g., path sandboxing).
    /// Default: allow.
    fn check_permissions(&self, _args: &Value) -> PermissionResult {
        PermissionResult::Allow
    }

    /// Short user-facing name for UI display.
    fn user_facing_name(&self) -> &str {
        self.name()
    }

    /// Gets the file path this tool operates on, if applicable.
    fn get_path(&self, _args: &Value) -> Option<String> {
        None
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tool Registry — holds all registered tools.
// `tools` preserves registration order for schema/prompt generation.
// `index` gives O(1) lookup by name.
// ─────────────────────────────────────────────────────────────────────────────
pub struct ToolRegistry {
    pub tools: Vec<Box<dyn Tool + Send + Sync>>,
    index: HashMap<String, usize>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: Vec::new(),
            index: HashMap::new(),
        };
        registry.register(Box::new(cargo_check::CargoCheck));
        registry.register(Box::new(edit_file::EditFile));
        registry.register(Box::new(shell::ShellExec));
        registry.register(Box::new(shell::GitTool));
        registry.register(Box::new(file_ops::ReadFile));
        registry.register(Box::new(file_ops::ListFiles));
        registry.register(Box::new(file_ops::CreateFile));
        registry.register(Box::new(workspace::WorkspaceTool::new(".")));
        registry.register(Box::new(bench::CargoBench));
        registry.register(Box::new(glob::GlobTool));
        registry.register(Box::new(grep::GrepTool));
        registry.register(Box::new(web_fetch::WebFetchTool));
        registry.register(Box::new(file_history::FileHistoryTool::new()));
        registry.register(Box::new(tool_search::ToolSearchTool));
        registry.register(Box::new(notebook_edit::NotebookEditTool));
        registry.register(Box::new(python_tool::PythonTool));
        registry
    }

    pub fn with_barq(barq: Arc<BarqIndex>) -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(barq_search::BarqSearch::new(barq)));
        registry
    }

    /// Extend the registry with task-board tools. Call after `new()` or `with_barq()`.
    pub fn with_task_board(mut self, task_board: Arc<TaskBoard>) -> Self {
        self.register(Box::new(task_tools::TaskCreateTool::new(Arc::clone(&task_board))));
        self.register(Box::new(task_tools::TaskUpdateTool::new(Arc::clone(&task_board))));
        self.register(Box::new(task_tools::TaskListTool::new(task_board)));
        self
    }

    /// Register a tool, updating the name index for O(1) later lookups.
    pub fn register(&mut self, tool: Box<dyn Tool + Send + Sync>) {
        let name = tool.name().to_string();
        let idx = self.tools.len();
        self.tools.push(tool);
        self.index.insert(name, idx);
    }

    /// O(1) tool lookup by name.
    pub fn get(&self, name: &str) -> Option<&Box<dyn Tool + Send + Sync>> {
        self.index.get(name).and_then(|&i| self.tools.get(i))
    }

    pub fn schemas(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.schema()).collect()
    }

    /// Search tools by keyword — used by ToolSearchTool.
    pub fn search(&self, query: &str) -> Vec<(&str, &str)> {
        let query_lower = query.to_lowercase();
        self.tools
            .iter()
            .filter(|t| {
                t.name().to_lowercase().contains(&query_lower)
                    || t.description().to_lowercase().contains(&query_lower)
                    || t.metadata()
                        .search_hint
                        .as_deref()
                        .map(|h| h.to_lowercase().contains(&query_lower))
                        .unwrap_or(false)
            })
            .map(|t| (t.name(), t.description()))
            .collect()
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}
