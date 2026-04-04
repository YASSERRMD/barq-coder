use crate::tools::{CommandRisk, PermissionResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalSettingsFile {
    #[serde(default)]
    permissions: LocalPermissionSettings,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct LocalPermissionSettings {
    #[serde(default)]
    allow: Vec<String>,
    #[serde(default)]
    ask: Vec<String>,
    #[serde(default)]
    deny: Vec<String>,
    #[serde(default)]
    default_mode: Option<String>,
}

#[derive(Debug, Default)]
struct PersistentPermissions {
    allowed_bash_commands: HashSet<String>,
    allowed_git_commands: HashSet<String>,
}

/// Permission scope tracks auto-allowed tools and paths.
/// Inspired by Claude Code's permission rules and per-project local approvals.
pub struct PermissionManager {
    workspace_root: PathBuf,
    auto_allowed_tools: RwLock<HashSet<String>>,
    always_deny_tools: RwLock<HashSet<String>>,
    always_ask_tools: RwLock<HashSet<String>>,
    allowed_dirs: RwLock<HashSet<PathBuf>>,
    blocked_dirs: HashSet<PathBuf>,
    blocked_command_patterns: Vec<String>,
    allowed_bash_commands: RwLock<HashSet<String>>,
    allowed_git_commands: RwLock<HashSet<String>>,
    accept_edits_for_session: RwLock<bool>,
    settings_path: PathBuf,
}

impl PermissionManager {
    pub fn new(workspace_root: &str) -> Self {
        let root = Path::new(workspace_root)
            .canonicalize()
            .unwrap_or_else(|_| PathBuf::from(workspace_root));
        let mut blocked = HashSet::new();

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

        let settings_path = root.join(".claude/settings.local.json");
        let persisted = Self::load_persistent_permissions(&settings_path);

        Self {
            workspace_root: root,
            auto_allowed_tools: RwLock::new(HashSet::new()),
            always_deny_tools: RwLock::new(HashSet::new()),
            always_ask_tools: RwLock::new(HashSet::new()),
            allowed_dirs: RwLock::new(HashSet::new()),
            blocked_dirs: blocked,
            blocked_command_patterns: blocked_patterns,
            allowed_bash_commands: RwLock::new(persisted.allowed_bash_commands),
            allowed_git_commands: RwLock::new(persisted.allowed_git_commands),
            accept_edits_for_session: RwLock::new(false),
            settings_path,
        }
    }

    pub fn auto_allow_tool(&self, tool_name: &str) {
        let mut tools = self.auto_allowed_tools.write().expect("permission lock poisoned");
        tools.insert(tool_name.to_string());
    }

    pub fn allow_directory(&self, dir: &str) {
        if let Ok(p) = Path::new(dir).canonicalize() {
            let mut dirs = self.allowed_dirs.write().expect("permission lock poisoned");
            dirs.insert(p);
        }
    }

    pub fn allow_edits_for_session(&self) {
        let mut accept = self
            .accept_edits_for_session
            .write()
            .expect("permission lock poisoned");
        *accept = true;
    }

    pub fn remember_shell_command(&self, command: &str) -> anyhow::Result<()> {
        {
            let mut commands = self
                .allowed_bash_commands
                .write()
                .expect("permission lock poisoned");
            commands.insert(command.to_string());
        }
        self.persist_allow_rule(&format!("Bash({})", command))
    }

    pub fn remember_git_command(&self, operation: &str, args: &str) -> anyhow::Result<()> {
        let command = canonical_git_command(operation, args);
        {
            let mut commands = self
                .allowed_git_commands
                .write()
                .expect("permission lock poisoned");
            commands.insert(command.clone());
        }
        self.persist_allow_rule(&format!("Git({})", command))
    }

    pub fn check_path(&self, target_path: &str) -> PermissionResult {
        let path = Path::new(target_path);

        let canonical = match path.canonicalize() {
            Ok(p) => p,
            Err(_) => {
                let mut p = self.workspace_root.clone();
                p.push(path);
                p
            }
        };

        for blocked in &self.blocked_dirs {
            if canonical.starts_with(blocked) {
                return PermissionResult::Deny(format!(
                    "Access to sensitive path blocked: {}",
                    target_path
                ));
            }
        }

        if canonical.starts_with(&self.workspace_root) {
            return PermissionResult::Allow;
        }

        let dirs = self.allowed_dirs.read().expect("permission lock poisoned");
        for allowed in dirs.iter() {
            if canonical.starts_with(allowed) {
                return PermissionResult::Allow;
            }
        }

        PermissionResult::Ask(format!(
            "Path {} is outside the current workspace. Allow access?",
            target_path
        ))
    }

    pub fn check_tool_call(
        &self,
        tool_name: &str,
        risk: CommandRisk,
        tool_specific_result: PermissionResult,
        args: &serde_json::Value,
    ) -> PermissionResult {
        if tool_name == "shell_exec" {
            if let Some(cmd) = args.get("command").and_then(|v| v.as_str()) {
                for pattern in &self.blocked_command_patterns {
                    if cmd.contains(pattern) {
                        return PermissionResult::Deny(format!(
                            "Command matches blocked pattern: '{}'",
                            pattern
                        ));
                    }
                }

                let commands = self
                    .allowed_bash_commands
                    .read()
                    .expect("permission lock poisoned");
                if commands.contains(cmd) {
                    return PermissionResult::Allow;
                }
            }
        }

        if tool_name == "git_ops" {
            let operation = args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
            let raw_args = args.get("args").and_then(|v| v.as_str()).unwrap_or("");
            let command = canonical_git_command(operation, raw_args);
            let commands = self
                .allowed_git_commands
                .read()
                .expect("permission lock poisoned");
            if commands.contains(&command) {
                return PermissionResult::Allow;
            }
        }

        if matches!(tool_name, "edit_file" | "create_file") {
            let accept = self
                .accept_edits_for_session
                .read()
                .expect("permission lock poisoned");
            if *accept {
                return PermissionResult::Allow;
            }
        }

        if matches!(tool_specific_result, PermissionResult::Deny(_)) {
            return tool_specific_result;
        }

        if matches!(tool_specific_result, PermissionResult::Ask(_)) {
            return tool_specific_result;
        }

        if self
            .always_deny_tools
            .read()
            .expect("permission lock poisoned")
            .contains(tool_name)
        {
            return PermissionResult::Deny(format!(
                "Tool {} is blacklisted by always_deny_tools",
                tool_name
            ));
        }

        if self
            .always_ask_tools
            .read()
            .expect("permission lock poisoned")
            .contains(tool_name)
        {
            return PermissionResult::Ask(format!(
                "Tool {} is configured to always ask",
                tool_name
            ));
        }

        if self
            .auto_allowed_tools
            .read()
            .expect("permission lock poisoned")
            .contains(tool_name)
        {
            return PermissionResult::Allow;
        }

        if risk == CommandRisk::Destructive {
            return PermissionResult::Ask(format!(
                "Tool '{}' makes destructive changes. Approve once, or allow a matching scope?",
                tool_name
            ));
        }

        PermissionResult::Allow
    }

    fn load_persistent_permissions(path: &Path) -> PersistentPermissions {
        let mut permissions = PersistentPermissions::default();

        let Ok(content) = fs::read_to_string(path) else {
            return permissions;
        };
        let Ok(settings) = serde_json::from_str::<LocalSettingsFile>(&content) else {
            return permissions;
        };

        for rule in settings.permissions.allow {
            if let Some(command) = rule
                .strip_prefix("Bash(")
                .and_then(|rest| rest.strip_suffix(')'))
            {
                permissions.allowed_bash_commands.insert(command.to_string());
            } else if let Some(command) = rule
                .strip_prefix("Git(")
                .and_then(|rest| rest.strip_suffix(')'))
            {
                permissions.allowed_git_commands.insert(command.to_string());
            }
        }

        permissions
    }

    fn persist_allow_rule(&self, rule: &str) -> anyhow::Result<()> {
        if let Some(parent) = self.settings_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut settings = if self.settings_path.exists() {
            let raw = fs::read_to_string(&self.settings_path)?;
            serde_json::from_str::<LocalSettingsFile>(&raw).unwrap_or_default()
        } else {
            LocalSettingsFile::default()
        };

        if !settings.permissions.allow.iter().any(|existing| existing == rule) {
            settings.permissions.allow.push(rule.to_string());
        }

        let content = serde_json::to_string_pretty(&settings)?;
        fs::write(&self.settings_path, content)?;
        Ok(())
    }
}

fn canonical_git_command(operation: &str, args: &str) -> String {
    if args.trim().is_empty() {
        format!("git {}", operation)
    } else {
        format!("git {} {}", operation, args.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::{canonical_git_command, LocalPermissionSettings, LocalSettingsFile, PermissionManager};
    use crate::tools::{CommandRisk, PermissionResult};
    use serde_json::json;
    use std::fs;
    use std::path::PathBuf;

    fn test_workspace(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "barqcoder-permissions-{}-{}",
            name,
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("create temp workspace");
        dir
    }

    #[test]
    fn canonical_git_command_trims_args() {
        assert_eq!(canonical_git_command("status", ""), "git status");
        assert_eq!(canonical_git_command("commit", " -m test "), "git commit -m test");
    }

    #[test]
    fn persisted_bash_rule_is_loaded() {
        let workspace = test_workspace("load");
        let claude_dir = workspace.join(".claude");
        fs::create_dir_all(&claude_dir).expect("claude dir");
        let settings = LocalSettingsFile {
            permissions: LocalPermissionSettings {
                allow: vec!["Bash(cargo check)".to_string()],
                ask: Vec::new(),
                deny: Vec::new(),
                default_mode: None,
            },
        };
        fs::write(
            claude_dir.join("settings.local.json"),
            serde_json::to_string_pretty(&settings).expect("serialize settings"),
        )
        .expect("write settings");

        let manager = PermissionManager::new(workspace.to_str().expect("workspace path"));
        let result = manager.check_tool_call(
            "shell_exec",
            CommandRisk::Destructive,
            PermissionResult::Allow,
            &json!({ "command": "cargo check" }),
        );

        assert!(matches!(result, PermissionResult::Allow));
        let _ = fs::remove_dir_all(workspace);
    }

    #[test]
    fn remember_shell_command_persists_local_rule() {
        let workspace = test_workspace("persist");
        let manager = PermissionManager::new(workspace.to_str().expect("workspace path"));

        manager
            .remember_shell_command("cargo check")
            .expect("remember command");

        let saved = fs::read_to_string(workspace.join(".claude/settings.local.json"))
            .expect("saved settings");
        assert!(saved.contains("Bash(cargo check)"));
        let _ = fs::remove_dir_all(workspace);
    }
}
