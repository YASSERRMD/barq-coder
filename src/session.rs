use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

// ─────────────────────────────────────────────────────────────────────────────
// Session event types — each is one line in the JSONL transcript.
// Inspired by Claude Code's append-only JSONL session persistence.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum SessionEvent {
    #[serde(rename = "user")]
    UserInput { content: String, timestamp: u64 },
    #[serde(rename = "assistant")]
    AssistantMessage { content: String, timestamp: u64 },
    #[serde(rename = "tool_use")]
    ToolCall {
        name: String,
        args: Value,
        timestamp: u64,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        name: String,
        result: Value,
        timestamp: u64,
    },
    #[serde(rename = "edit")]
    EditApplied {
        file: String,
        patch: String,
        timestamp: u64,
    },
    #[serde(rename = "compact_boundary")]
    CompactBoundary {
        summary: String,
        timestamp: u64,
    },
    #[serde(rename = "error")]
    Error { message: String, timestamp: u64 },
    #[serde(rename = "system")]
    System {
        subtype: String,
        content: String,
        timestamp: u64,
    },
}

impl SessionEvent {
    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    pub fn user(content: &str) -> Self {
        Self::UserInput {
            content: content.to_string(),
            timestamp: Self::now(),
        }
    }

    pub fn assistant(content: &str) -> Self {
        Self::AssistantMessage {
            content: content.to_string(),
            timestamp: Self::now(),
        }
    }

    pub fn tool_call(name: &str, args: Value) -> Self {
        Self::ToolCall {
            name: name.to_string(),
            args,
            timestamp: Self::now(),
        }
    }

    pub fn tool_result(name: &str, result: Value) -> Self {
        Self::ToolResult {
            name: name.to_string(),
            result,
            timestamp: Self::now(),
        }
    }

    pub fn compact_boundary(summary: &str) -> Self {
        Self::CompactBoundary {
            summary: summary.to_string(),
            timestamp: Self::now(),
        }
    }

    pub fn error(message: &str) -> Self {
        Self::Error {
            message: message.to_string(),
            timestamp: Self::now(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Session metadata — lightweight summary for listing.
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: u64,
    pub event_count: usize,
    pub workspace: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// SessionStore — append-only JSONL persistence.
// Inspired by Claude Code's ~/.claude/projects/<hash>/sessions/<id>.jsonl
// ─────────────────────────────────────────────────────────────────────────────

pub struct SessionStore {
    sessions_dir: PathBuf,
    workspace: String,
}

impl SessionStore {
    pub fn new(workspace: &str) -> Self {
        let dir = Path::new(workspace).join(".barqcoder/sessions/");
        let _ = fs::create_dir_all(&dir);
        Self {
            sessions_dir: dir,
            workspace: workspace.to_string(),
        }
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.sessions_dir.join(format!("{}.jsonl", id))
    }

    /// Append a single event to the session JSONL file.
    pub fn append(&self, session_id: &str, event: &SessionEvent) -> anyhow::Result<()> {
        let path = self.session_path(session_id);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let line = serde_json::to_string(event)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Load all events from a session JSONL file.
    pub fn load(&self, id: &str) -> anyhow::Result<Vec<SessionEvent>> {
        let path = self.session_path(id);
        let content = fs::read_to_string(path)?;
        let events: Vec<SessionEvent> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        Ok(events)
    }

    /// List all sessions with metadata.
    pub fn list(&self) -> Vec<SessionMeta> {
        let mut metas = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    let id = path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();

                    if let Ok(content) = fs::read_to_string(&path) {
                        let lines: Vec<&str> = content.lines().collect();
                        let event_count = lines.len();

                        // Get created_at from first event
                        let created_at = lines
                            .first()
                            .and_then(|l| serde_json::from_str::<Value>(l).ok())
                            .and_then(|v| v.get("timestamp").and_then(|t| t.as_u64()))
                            .unwrap_or(0);

                        metas.push(SessionMeta {
                            id,
                            created_at,
                            event_count,
                            workspace: self.workspace.clone(),
                        });
                    }
                }
            }
        }
        metas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        metas
    }

    /// Get the last session ID by modification time.
    pub fn last_session_id(&self) -> Option<String> {
        let metas = self.list();
        metas.first().map(|m| m.id.clone())
    }

    /// Replay a session's events as an iterator.
    pub fn replay(&self, id: &str) -> impl Iterator<Item = SessionEvent> {
        self.load(id).unwrap_or_default().into_iter()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Non-blocking transcript recorder.
// Inspired by Claude Code's fire-and-forget recordTranscript().
// ─────────────────────────────────────────────────────────────────────────────

pub struct TranscriptRecorder {
    tx: mpsc::UnboundedSender<(String, SessionEvent)>,
}

impl TranscriptRecorder {
    /// Start a background task that writes events to disk.
    pub fn new(store: SessionStore) -> Self {
        let (tx, mut rx) = mpsc::unbounded_channel::<(String, SessionEvent)>();

        tokio::spawn(async move {
            while let Some((session_id, event)) = rx.recv().await {
                if let Err(e) = store.append(&session_id, &event) {
                    tracing::warn!("Failed to write transcript: {}", e);
                }
            }
        });

        Self { tx }
    }

    /// Fire-and-forget: send an event to be recorded.
    pub fn record(&self, session_id: &str, event: SessionEvent) {
        let _ = self.tx.send((session_id.to_string(), event));
    }
}
