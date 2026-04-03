/// Slash command dispatcher.
/// Parses `/command [args]` strings and returns a `SlashCommand` variant.

use crate::memory::Memory;
use crate::session::SessionStore;
use crate::cost_tracker::CostTracker;

#[derive(Debug, Clone, PartialEq)]
pub enum SlashCommand {
    Compact,
    Plan,
    Review,
    Memory(MemoryAction),
    Model(String),
    Status,
    Help,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum MemoryAction {
    Show,
    Add(String),
}

/// Parse a raw `/command [args]` input string.
pub fn parse(input: &str) -> Option<SlashCommand> {
    let input = input.trim();
    if !input.starts_with('/') {
        return None;
    }
    let without_slash = &input[1..];
    let mut parts = without_slash.splitn(2, ' ');
    let cmd = parts.next().unwrap_or("").to_lowercase();
    let rest = parts.next().unwrap_or("").trim().to_string();

    let result = match cmd.as_str() {
        "compact" => SlashCommand::Compact,
        "plan" => SlashCommand::Plan,
        "review" => SlashCommand::Review,
        "memory" | "mem" => {
            if rest.starts_with("add ") {
                SlashCommand::Memory(MemoryAction::Add(rest[4..].trim().to_string()))
            } else if rest == "show" || rest.is_empty() {
                SlashCommand::Memory(MemoryAction::Show)
            } else {
                SlashCommand::Memory(MemoryAction::Add(rest))
            }
        }
        "model" => {
            if rest.is_empty() {
                SlashCommand::Unknown("Usage: /model <model-name>".to_string())
            } else {
                SlashCommand::Model(rest)
            }
        }
        "status" => SlashCommand::Status,
        "help" | "?" => SlashCommand::Help,
        other => SlashCommand::Unknown(other.to_string()),
    };
    Some(result)
}

/// Execute a slash command and return a display string for the TUI.
pub fn execute(
    cmd: &SlashCommand,
    workspace_root: &str,
    session_id: &str,
    session_store: &SessionStore,
    cost: &CostTracker,
    current_model: &str,
) -> CommandResult {
    match cmd {
        SlashCommand::Compact => CommandResult::Compaction,

        SlashCommand::Plan => CommandResult::Message(
            "Plan mode enabled — agent will outline steps before executing tools.".to_string(),
        ),

        SlashCommand::Review => {
            let events = session_store.replay(session_id);
            let mut lines = vec!["--- Session Diff Review ---".to_string()];
            for ev in events {
                match ev {
                    crate::session::SessionEvent::EditApplied { file, patch, .. } => {
                        lines.push(format!("File: {}\n{}", file, patch));
                    }
                    _ => {}
                }
            }
            if lines.len() == 1 {
                lines.push("No edits found in this session.".to_string());
            }
            CommandResult::Message(lines.join("\n"))
        }

        SlashCommand::Memory(MemoryAction::Show) => {
            CommandResult::Message(Memory::show(workspace_root))
        }

        SlashCommand::Memory(MemoryAction::Add(note)) => {
            match Memory::append(workspace_root, note) {
                Ok(()) => CommandResult::Message(format!("Memory updated: {}", note)),
                Err(e) => CommandResult::Message(format!("Failed to update memory: {}", e)),
            }
        }

        SlashCommand::Model(name) => CommandResult::SwitchModel(name.clone()),

        SlashCommand::Status => {
            let msg = format!(
                "Session: {}\nModel: {}\n{}",
                session_id, current_model, cost.summary()
            );
            CommandResult::Message(msg)
        }

        SlashCommand::Help => CommandResult::Message(HELP_TEXT.to_string()),

        SlashCommand::Unknown(name) => {
            CommandResult::Message(format!("Unknown command: /{}\n\nType /help for available commands.", name))
        }
    }
}

pub enum CommandResult {
    Message(String),
    SwitchModel(String),
    Compaction,
}

pub const HELP_TEXT: &str = "\
Available slash commands:
  /compact          Compact conversation history to save context window space
  /plan             Enter plan mode (outline steps before executing)
  /review           Show all edits made this session as a diff review
  /memory [show]    Show project memory (.barqcoder.md)
  /memory add <note>  Add a note to project memory
  /model <name>     Switch the active LLM model mid-session
  /status           Show session stats: turns, token usage, tool calls
  /help             Show this help message";
