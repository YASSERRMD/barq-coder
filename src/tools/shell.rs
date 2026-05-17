use super::Tool;
use async_trait::async_trait;
use serde_json::{json, Value};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

// ─────────────────────────────────────────────────────────────────────────────
// Shell command safety enforcement
// ─────────────────────────────────────────────────────────────────────────────

/// Dangerous whole-command patterns (substring of the full command string).
/// These are checked regardless of word boundaries because even embedding them
/// in a pipeline is dangerous (e.g., `echo foo | rm -rf /`).
const DANGEROUS_SUBSTRINGS: &[&str] = &[
    "rm -rf /",
    "> /dev/sda",
    "dd if=",
    "mkfs",
    ":(){ :|:& };:", // fork bomb
];

/// Commands that are only blocked when they appear as a standalone word at the
/// start of the command or after a pipe/semicolon/ampersand delimiter.
/// Simple string-contains would block "scurl" or "myssh" incorrectly.
const BLOCKED_COMMANDS: &[&str] = &[
    "sudo",
    "su",
    "curl",
    "wget",
    "ssh",
    "scp",
    "nc",    // netcat
    "ncat",
    "socat",
    "chmod",
    "chown",
    "chroot",
];

/// Returns the name of the first blocked pattern found in `cmd`, or `None`
/// if the command is safe to run.
fn blocked_shell_command(cmd: &str) -> Option<&'static str> {
    // Check dangerous substrings first (no word-boundary needed)
    for pattern in DANGEROUS_SUBSTRINGS {
        if cmd.contains(pattern) {
            return Some(pattern);
        }
    }

    // Check word-boundary blocked commands: split on shell delimiters and
    // check the first token of each segment.
    for segment in cmd.split(['|', ';', '&', '\n']) {
        let token = segment.trim().split_whitespace().next().unwrap_or("");
        // Strip any leading path prefix (e.g., /usr/bin/sudo → sudo)
        let base = token.rsplit('/').next().unwrap_or(token);
        for blocked in BLOCKED_COMMANDS {
            if base == *blocked {
                return Some(blocked);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::blocked_shell_command;

    #[test]
    fn blocks_dangerous_substrings_regardless_of_position() {
        assert!(blocked_shell_command("rm -rf /").is_some());
        assert!(blocked_shell_command("echo foo && rm -rf / --no-preserve-root").is_some());
        assert!(blocked_shell_command("dd if=/dev/zero of=/dev/sda").is_some());
    }

    #[test]
    fn blocks_sudo_as_whole_word() {
        assert!(blocked_shell_command("sudo apt install vim").is_some());
        assert!(blocked_shell_command("cargo build && sudo make install").is_some());
    }

    #[test]
    fn does_not_block_commands_containing_blocked_word_as_substring() {
        // "scurl" or "my_curl_wrapper" should NOT be blocked
        assert!(blocked_shell_command("scurl https://example.com").is_none());
        assert!(blocked_shell_command("my_ssh_wrapper host").is_none());
        assert!(blocked_shell_command("echo sudoer").is_none());
    }

    #[test]
    fn allows_safe_cargo_commands() {
        assert!(blocked_shell_command("cargo check").is_none());
        assert!(blocked_shell_command("cargo test --all").is_none());
        assert!(blocked_shell_command("cargo build --release").is_none());
    }

    #[test]
    fn blocks_absolute_path_to_blocked_command() {
        assert!(blocked_shell_command("/usr/bin/sudo ls").is_some());
        assert!(blocked_shell_command("/bin/ssh host").is_some());
    }
}

pub struct ShellExec;

#[async_trait]
impl Tool for ShellExec {
    fn name(&self) -> &'static str {
        "shell_exec"
    }

    fn description(&self) -> &'static str {
        "Run shell command in sandboxed workspace"
    }

    fn schema(&self) -> Value {
        json!({
            "command": "string",
            "working_dir": "string",
            "timeout_secs": "number",
            "dry_run": "boolean" // Phase 4 Safe Mode
        })
    }

    fn is_destructive(&self) -> bool { true } // Shell commands are considered Destructive by default unless checked
    fn is_read_only(&self) -> bool { false }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("");
        let working_dir = args
            .get("working_dir")
            .and_then(|v| v.as_str())
            .unwrap_or(".");
        let timeout_secs = args.get("timeout_secs").and_then(|v| v.as_u64()).unwrap_or(30);
        let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
        let timeout_secs = std::cmp::min(timeout_secs, 60);

        let command_str = command.to_string();
        if let Some(blocked) = blocked_shell_command(&command_str) {
            return Err(anyhow::anyhow!("Blocked command detected: {}", blocked));
        }

        if dry_run {
            return Ok(json!({
                "stdout": format!("[Dry Run] Would execute: {} in {}", command_str, working_dir),
                "stderr": "",
                "exit_code": 0,
                "timed_out": false
            }));
        }

        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(command).current_dir(working_dir);

        let result = timeout(Duration::from_secs(timeout_secs), cmd.output()).await;

        match result {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);

                Ok(json!({
                    "stdout": stdout,
                    "stderr": stderr,
                    "exit_code": exit_code,
                    "timed_out": false
                }))
            }
            Ok(Err(e)) => Err(anyhow::anyhow!("Failed to execute command: {}", e)),
            Err(_) => Ok(json!({
                "stdout": "",
                "stderr": "Command timed out",
                "exit_code": -1,
                "timed_out": true
            })),
        }
    }
}

pub struct GitTool;

#[async_trait]
impl Tool for GitTool {
    fn name(&self) -> &'static str {
        "git_ops"
    }

    fn description(&self) -> &'static str {
        "Git operations: status/diff/log/add/commit"
    }

    fn schema(&self) -> Value {
        json!({
            "operation": "string",
            "args": "string",
            "dry_run": "boolean"
        })
    }

    fn is_destructive(&self) -> bool { false } // Basic tracking operations only, mostly Mutating
    fn is_read_only(&self) -> bool { false }

    async fn call(&self, args: Value) -> anyhow::Result<Value> {
        let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("");
        let cmd_args = args.get("args").and_then(|v| v.as_str()).unwrap_or("");
        let dry_run = args.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

        let valid_ops = ["status", "diff", "log", "add", "commit"];
        if !valid_ops.contains(&op) {
            return Err(anyhow::anyhow!("Invalid git operation: {}", op));
        }

        if dry_run {
            return Ok(json!({
                "output": format!("[Dry Run] Would run: git {} {}", op, cmd_args),
                "success": true
            }));
        }

        let mut cmd = Command::new("git");
        cmd.arg(op);
        
        if !cmd_args.is_empty() {
             // Split args carefully in a real implementation
             for arg in cmd_args.split_whitespace() {
                 cmd.arg(arg);
             }
        }

        let result = cmd.output().await;

        match result {
            Ok(output) => {
                let success = output.status.success();
                let out_str = if success {
                    String::from_utf8_lossy(&output.stdout).to_string()
                } else {
                    String::from_utf8_lossy(&output.stderr).to_string()
                };

                Ok(json!({
                    "output": out_str,
                    "success": success
                }))
            }
            Err(e) => Err(anyhow::anyhow!("Failed to execute git command: {}", e)),
        }
    }
}
