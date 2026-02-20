// CRDT mock for concurrent edits
pub struct CrdtDocument {
    pub content: String,
}

impl CrdtDocument {
    pub fn new(content: &str) -> Self {
        Self {
            content: content.to_string()
        }
    }
    
    pub fn apply_delta(&mut self, _delta: &str) {
        // mock apply
    }
}
