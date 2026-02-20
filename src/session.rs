use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Session {
    pub id: String,
    pub created_at: u64,
    pub workspace: String,
    pub events: Vec<SessionEvent>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum SessionEvent {
    UserInput(String),
    AgentToken(String),
    ToolCalled {
        name: String,
        args: Value,
        result: Value,
    },
    EditApplied {
        file: String,
        patch: String,
    },
    Error(String),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionMeta {
    pub id: String,
    pub created_at: u64,
    pub event_count: usize,
    pub workspace: String,
}

pub struct SessionStore {
    sessions_dir: PathBuf,
}

impl SessionStore {
    pub fn new(workspace: &str) -> Self {
        let dir = Path::new(workspace).join(".barqcoder/sessions/");
        let _ = fs::create_dir_all(&dir);
        Self { sessions_dir: dir }
    }

    pub fn save(&self, session: &Session) -> anyhow::Result<()> {
        let content = serde_json::to_string_pretty(session)?;
        let path = self.sessions_dir.join(format!("{}.json", session.id));
        fs::write(path, content)?;
        Ok(())
    }

    pub fn load(&self, id: &str) -> anyhow::Result<Session> {
        let path = self.sessions_dir.join(format!("{}.json", id));
        let content = fs::read_to_string(path)?;
        let session = serde_json::from_str(&content)?;
        Ok(session)
    }

    pub fn list(&self) -> Vec<SessionMeta> {
        let mut metas = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.sessions_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if let Ok(session) = serde_json::from_str::<Session>(&content) {
                        metas.push(SessionMeta {
                            id: session.id,
                            created_at: session.created_at,
                            event_count: session.events.len(),
                            workspace: session.workspace,
                        });
                    }
                }
            }
        }
        metas.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        metas
    }

    pub fn replay(&self, id: &str) -> impl Iterator<Item = SessionEvent> {
        let session = self.load(id).unwrap_or_else(|_| Session {
            id: id.to_string(),
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            workspace: "".to_string(),
            events: vec![],
        });
        session.events.into_iter()
    }
}
