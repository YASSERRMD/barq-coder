use std::fs;
use std::path::{Path, PathBuf};

/// Project memory file — equivalent to Claude Code's CLAUDE.md.
/// Loaded from `.barqcoder.md` in the workspace root and parent dirs.
pub struct Memory {
    pub entries: Vec<MemoryEntry>,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct MemoryEntry {
    pub content: String,
}

impl Memory {
    /// Load memory from the workspace directory and all parent directories.
    pub fn load(workspace_root: &str) -> Self {
        let workspace = Path::new(workspace_root);
        let path = workspace.join(".barqcoder.md");
        let mut entries = Vec::new();

        // Walk upward from workspace to filesystem root, collecting memory
        let mut current = Some(workspace.to_path_buf());
        while let Some(dir) = current {
            let mem_file = dir.join(".barqcoder.md");
            if mem_file.exists() {
                if let Ok(content) = fs::read_to_string(&mem_file) {
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && !trimmed.starts_with('#') {
                            entries.push(MemoryEntry { content: trimmed.to_string() });
                        }
                    }
                }
            }
            current = dir.parent().map(|p| p.to_path_buf());
        }

        Self { entries, path }
    }

    /// Render memory as a system prompt injection block.
    pub fn to_prompt_block(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let lines = self.entries.iter()
            .map(|e| format!("- {}", e.content))
            .collect::<Vec<_>>()
            .join("\n");
        format!("<project_memory>\n{}\n</project_memory>\n", lines)
    }

    /// Append a new entry to the workspace `.barqcoder.md`.
    pub fn append(workspace_root: &str, note: &str) -> anyhow::Result<()> {
        let path = Path::new(workspace_root).join(".barqcoder.md");
        let existing = if path.exists() {
            fs::read_to_string(&path)?
        } else {
            "# Barq Coder Project Memory\n\n".to_string()
        };
        let updated = format!("{}- {}\n", existing, note.trim());
        fs::write(&path, updated)?;
        Ok(())
    }

    /// Show full memory file contents.
    pub fn show(workspace_root: &str) -> String {
        let path = Path::new(workspace_root).join(".barqcoder.md");
        if path.exists() {
            fs::read_to_string(&path).unwrap_or_default()
        } else {
            "No .barqcoder.md found in workspace.".to_string()
        }
    }
}
