pub mod server;
pub mod client;
pub mod sync;
pub mod tui;
pub mod auth;

pub struct CollabSession {
    pub session_id: String,
    pub users: Vec<String>,
}

impl CollabSession {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            users: Vec::new(),
        }
    }
}
