use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

const EMBEDDED_TRANSCRIPT_CONTROL_TS: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/control_plane/transcript_control.ts"));

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizedMessageKind {
    User,
    Agent,
    ToolCall,
    ToolResult,
    System,
    Error,
    Permission,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedMessage {
    pub kind: NormalizedMessageKind,
    pub title: String,
    pub content: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ControlPlaneResponse {
    Noop,
    Append {
        message: NormalizedMessage,
        #[serde(default)]
        tool_log_entry: Option<String>,
    },
    ReplaceLast {
        message: NormalizedMessage,
        #[serde(default)]
        tool_log_entry: Option<String>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlPlaneCommand {
    Reset,
    UserMessage { content: String },
    AssistantToken { token: String },
    AssistantDone { #[serde(skip_serializing_if = "Option::is_none")] content: Option<String> },
    ToolCall { name: String, args: Value },
    ToolResult { name: String, result: Value },
    SystemMessage { content: String },
    ErrorMessage { content: String },
    PermissionRequest { name: String, args: Value, reason: String },
    PermissionResolved { name: String, decision: PermissionDecision },
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Approved,
    Denied,
}

pub struct TranscriptControlBridge {
    child: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    _script_path: PathBuf,
}

impl TranscriptControlBridge {
    pub fn new() -> anyhow::Result<Self> {
        let script_path = resolve_control_plane_script()?;
        let node_bin = node_binary();

        let mut child = Command::new(&node_bin)
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start {} control plane at {}",
                    node_bin,
                    script_path.display()
                )
            })?;

        let stdin = child
            .stdin
            .take()
            .context("control plane stdin unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("control plane stdout unavailable")?;

        let mut bridge = Self {
            child,
            stdin: BufWriter::new(stdin),
            stdout: BufReader::new(stdout),
            _script_path: script_path,
        };
        let _ = bridge.send(ControlPlaneCommand::Reset)?;
        Ok(bridge)
    }

    pub fn send(&mut self, command: ControlPlaneCommand) -> anyhow::Result<ControlPlaneResponse> {
        let payload = serde_json::to_string(&command)?;
        self.stdin.write_all(payload.as_bytes())?;
        self.stdin.write_all(b"\n")?;
        self.stdin.flush()?;

        let mut line = String::new();
        let bytes = self.stdout.read_line(&mut line)?;
        if bytes == 0 {
            return Err(anyhow!("control plane closed stdout unexpectedly"));
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("control plane returned an empty response"));
        }

        serde_json::from_str(trimmed).context("failed to parse control plane response")
    }
}

impl Drop for TranscriptControlBridge {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn resolve_control_plane_script() -> anyhow::Result<PathBuf> {
    if let Ok(override_path) = std::env::var("BARQ_CONTROL_PLANE_SCRIPT") {
        let override_path = PathBuf::from(override_path);
        if override_path.exists() {
            return Ok(override_path);
        }

        return Err(anyhow!(
            "BARQ_CONTROL_PLANE_SCRIPT points to a missing file: {}",
            override_path.display()
        ));
    }

    materialize_embedded_script()
}

pub fn node_binary() -> String {
    std::env::var("BARQ_NODE_BIN").unwrap_or_else(|_| "node".to_string())
}

pub fn check_node_runtime() -> anyhow::Result<String> {
    let output = Command::new(node_binary())
        .arg("--version")
        .output()
        .context("failed to run node --version")?;

    if !output.status.success() {
        return Err(anyhow!(
            "node --version failed with exit {:?}",
            output.status.code()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn materialize_embedded_script() -> anyhow::Result<PathBuf> {
    let cache_dir = std::env::temp_dir()
        .join("barqcoder")
        .join("control_plane");
    fs::create_dir_all(&cache_dir)
        .with_context(|| format!("failed to create {}", cache_dir.display()))?;

    let mut hasher = DefaultHasher::new();
    EMBEDDED_TRANSCRIPT_CONTROL_TS.hash(&mut hasher);
    let hash = hasher.finish();
    let script_path = cache_dir.join(format!("transcript_control-{:016x}.ts", hash));

    let needs_write = match fs::read_to_string(&script_path) {
        Ok(existing) => existing != EMBEDDED_TRANSCRIPT_CONTROL_TS,
        Err(_) => true,
    };

    if needs_write {
        fs::write(&script_path, EMBEDDED_TRANSCRIPT_CONTROL_TS)
            .with_context(|| format!("failed to write {}", script_path.display()))?;
    }

    Ok(script_path)
}

#[cfg(test)]
mod tests {
    use super::{
        check_node_runtime, resolve_control_plane_script, ControlPlaneCommand,
        ControlPlaneResponse, NormalizedMessageKind, PermissionDecision, TranscriptControlBridge,
    };
    use serde_json::json;

    #[test]
    fn control_plane_extracts_streamed_json_answer() {
        let mut bridge = TranscriptControlBridge::new().unwrap();

        let first = bridge
            .send(ControlPlaneCommand::AssistantToken {
                token: "{\"final_answer\":\"Hi".to_string(),
            })
            .unwrap();
        let second = bridge
            .send(ControlPlaneCommand::AssistantToken {
                token: " there\"}".to_string(),
            })
            .unwrap();
        let done = bridge
            .send(ControlPlaneCommand::AssistantDone { content: None })
            .unwrap();

        match first {
            ControlPlaneResponse::Append { message, .. } => {
                assert_eq!(message.kind, NormalizedMessageKind::Agent);
                assert_eq!(message.content, "Hi");
            }
            other => panic!("unexpected first response: {:?}", other),
        }

        match second {
            ControlPlaneResponse::ReplaceLast { message, .. } => {
                assert_eq!(message.content, "Hi there");
            }
            other => panic!("unexpected second response: {:?}", other),
        }

        match done {
            ControlPlaneResponse::ReplaceLast { message, .. } => {
                assert_eq!(message.content, "Hi there");
                assert_eq!(message.status.as_deref(), Some("final"));
            }
            other => panic!("unexpected done response: {:?}", other),
        }
    }

    #[test]
    fn control_plane_summarizes_tool_calls_without_dumping_payloads() {
        let mut bridge = TranscriptControlBridge::new().unwrap();

        let response = bridge
            .send(ControlPlaneCommand::ToolCall {
                name: "create_file".to_string(),
                args: json!({
                    "path": "src/new.rs",
                    "content": "fn main() {}\n".repeat(40),
                }),
            })
            .unwrap();

        match response {
            ControlPlaneResponse::Append {
                message,
                tool_log_entry,
            } => {
                assert_eq!(message.kind, NormalizedMessageKind::ToolCall);
                assert!(message.content.contains("Create src/new.rs"));
                assert!(!message.content.contains("fn main() {}"));
                assert_eq!(tool_log_entry.as_deref(), Some("Create src/new.rs"));
            }
            other => panic!("unexpected response: {:?}", other),
        }
    }

    #[test]
    fn control_plane_formats_permission_requests() {
        let mut bridge = TranscriptControlBridge::new().unwrap();

        let request = bridge
            .send(ControlPlaneCommand::PermissionRequest {
                name: "shell_exec".to_string(),
                args: json!({
                    "command": "cargo check",
                    "working_dir": "."
                }),
                reason: "Needs verification.".to_string(),
            })
            .unwrap();
        let resolution = bridge
            .send(ControlPlaneCommand::PermissionResolved {
                name: "shell_exec".to_string(),
                decision: PermissionDecision::Approved,
            })
            .unwrap();

        match request {
            ControlPlaneResponse::Append { message, .. } => {
                assert_eq!(message.kind, NormalizedMessageKind::Permission);
                assert!(message.content.contains("Shell Exec needs approval"));
                assert_eq!(message.status.as_deref(), Some("pending"));
            }
            other => panic!("unexpected request response: {:?}", other),
        }

        match resolution {
            ControlPlaneResponse::Append { message, .. } => {
                assert_eq!(message.kind, NormalizedMessageKind::Permission);
                assert!(message.content.contains("Shell Exec was approved."));
                assert_eq!(message.status.as_deref(), Some("approved"));
            }
            other => panic!("unexpected resolution response: {:?}", other),
        }
    }

    #[test]
    fn embedded_script_is_materialized_outside_workspace() {
        let path = resolve_control_plane_script().unwrap();

        assert!(path.exists());
        assert!(path.to_string_lossy().contains("barqcoder/control_plane"));
    }

    #[test]
    fn node_runtime_is_detectable() {
        let version = check_node_runtime().unwrap();

        assert!(version.starts_with('v'));
    }
}
