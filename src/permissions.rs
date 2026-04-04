use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use crate::tools::{CommandRisk, PermissionResult};

/// Permission scope tracks auto-allowed tools and paths.
/// Inspired by Claude Code's auto-approval system.
pub struct PermissionManager {
    workspace_root: PathBuf,
    auto_allowed_tools: HashSet<String>,
    always_deny_tools: HashSet<String>,
    always_ask_tools: HashSet<String>,
    allowed_dirs: HashSet<PathBuf>,
    blocked_dirs: HashSet<PathBuf>,
    blocked_command_patterns: Vec<String>,
}

impl PermissionManager {
    pub fn new(workspace_root: &str) -> Self {
        let root = Path::new(workspace_root).canonicalize().unwrap_or_else(|_| PathBuf::from(workspace_root));
        let mut blocked = HashSet::new();

        // Default sensitive paths blocked
        blocked.insert(PathBuf::from("/etc"));
        blocked.insert(PathBuf::from("/bin"));
        blocked.insert(PathBuf::from("/usr/bin"));
        blocked.insert(PathBuf::from("/usr/sbin"));
        blocked.insert(PathBuf::from("/var/run"));
        let blocked_patterns = vec![
            "rm -rf /".to_string(),
            "mkfs".to_string(),
            "dd if=".to_string(),
            "> /dev/sda".to_string(),
        ];
        
        Self {
            workspace_root: root,
            auto_allowed_tools: HashSet::new(),
            always_deny_tools: HashSet::new(),
            always_ask_tools: HashSet::new(),
            allowed_dirs: HashSet::new(),
            blocked_dirs: blocked,
            blocked_command_patterns: blocked_patterns,
        }
    }

    /// Whitelist a specific tool or command for auto-execution
    pub fn auto_allow_tool(&mut self, tool_name: &str) {
        self.auto_allowed_tools.insert(tool_name.to_string());
    }

    /// Whitelist a specific directory
    pub fn allow_directory(&mut self, dir: &str) {
        if let Ok(p) = Path::new(dir).canonicalize() {
            self.allowed_dirs.insert(p);
        }
    }

    /// Determine if a file path is safe to access
    pub fn check_path(&self, target_path: &str) -> PermissionResult {
        let path = Path::new(target_path);
        
        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                // If it doesn't exist, we resolve relative to workspace_root
                let mut p = self.workspace_root.clone();
                p.push(path);
                p
            }
        };

        // 1. Check strict blocks
        for blocked in &self.blocked_dirs {
            if canonical.starts_with(blocked) {
                return PermissionResult::Deny(format!("Access to sensitive path blocked: {}", target_path));
            }
        }

        // 2. Allowed automatically if within workspace root
        if canonical.starts_with(&self.workspace_root) {
            return PermissionResult::Allow;
        }

        // 3. Fallback to explicitly allowed directories
        for allowed in &self.allowed_dirs {
            if canonical.starts_with(allowed) {
                return PermissionResult::Allow;
            }
        }

        // 4. Default deny, ask for permission
        PermissionResult::Ask(format!("Path {} is outside the current workspace. Allow access?", target_path))
    }

    /// Ask if a tool is allowed to execute.
    pub fn check_tool_call(
        &self,
        tool_name: &str,
        risk: CommandRisk,
        tool_specific_result: PermissionResult,
        args: &serde_json::Value,
    ) -> PermissionResult {
        // Evaluate command patterns for shell execution
        if tool_name == "shell_exec" {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                for pattern in &self.blocked_command_patterns {
                    if cmd.contains(pattern) {
                        return PermissionResult::Deny(format!("Command matches blocked pattern: '{}'", pattern));
                    }
                }
            }
        }

        // If the tool specifically denies, respect it unconditionally
        if matches!(tool_specific_result, PermissionResult::Deny(_)) {
            return tool_specific_result;
        }

        // If specifically asked, respect it
        if matches!(tool_specific_result, PermissionResult::Ask(_)) {
            return tool_specific_result;
        }

        if self.always_deny_tools.contains(tool_name) {
            return PermissionResult::Deny(format!("Tool {} is blacklisted by always_deny_tools", tool_name));
        }

        if self.always_ask_tools.contains(tool_name) {
            return PermissionResult::Ask(format!("Tool {} is configured to always ask", tool_name));
        }

        // If the tool says allow, we still check our internal policies
        if self.auto_allowed_tools.contains(tool_name) {
            return PermissionResult::Allow;
        }

        // Destructive tools must either be auto-whitelisted or explicitly asked
        if risk == CommandRisk::Destructive {
            return PermissionResult::Ask(format!(
                "Tool '{}' makes destructive changes. Auto-allow for this session?",
                tool_name
            ));
        }

        PermissionResult::Allow
    }
}
