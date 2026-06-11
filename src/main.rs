// Broad dead_code allow is intentional: collab, voice, mcp, lsp and sandbox
// modules are early-stage and contain placeholder implementations.
// Individual unused-import / unused-variable warnings are still reported.
#![allow(dead_code)]

use crossterm::{
    event::{
        self, Event, EventStream, KeyCode, KeyEventKind, KeyModifiers,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{io, sync::Arc, time::Duration};
use tokio::sync::mpsc;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

mod agent;
mod agents;
mod barq;
mod cli;
mod collab;
mod commands;
mod config;
mod control_plane;
mod context;
mod cost_tracker;
mod daemon;
mod lsp;
mod macro_goals;
mod markdown;
mod memory;
mod mcp;
mod orchestrator;
mod permissions;
mod sandbox;
mod session;
mod symbolic;
mod tasks;
mod tools;
mod tui;
mod verifier;
mod voice;


use barq::BarqIndex;
use cli::{Cli, Commands};
use clap::Parser;
use config::Config;
use control_plane::{
    check_node_runtime, resolve_control_plane_script, ControlPlaneCommand,
    ControlPlaneResponse, NormalizedMessage, NormalizedMessageKind,
    PermissionDecision as ControlPlanePermissionDecision, TranscriptControlBridge,
};
use cost_tracker::CostTracker;
use orchestrator::{Orchestrator, OrchestratorEvent};
use session::{SessionEvent, SessionStore};
use tools::ToolRegistry;
use agents::coordinator::CoordinatorAgent;
use tui::{ActiveTab, ChatMessage, Focus, SessionEntry, TuiState};

// ─────────────────────────────────────────────────────────────────────────────
// App: top-level state container
// ─────────────────────────────────────────────────────────────────────────────
struct App {
    tui: TuiState,
    orchestrator: Orchestrator,
    config: Config,
    coordinator: Arc<CoordinatorAgent>,
    event_rx: Option<mpsc::Receiver<OrchestratorEvent>>,
    session_store: SessionStore,
    session_id: String,
    transcript_control: TranscriptControlBridge,
    pending_permission_requests: std::collections::VecDeque<PendingPermissionRequest>,
    pending_budget_request: Option<tokio::sync::oneshot::Sender<bool>>,
    cost: CostTracker,
    skip_permissions: bool,
    streaming_assistant_raw: String,
}

struct PendingPermissionRequest {
    name: String,
    args: serde_json::Value,
    reason: String,
    tx: tokio::sync::oneshot::Sender<bool>,
}

enum PermissionDecision {
    AllowOnce,
    AllowRemembered,
    Deny,
}

fn normalized_to_chat_message(message: NormalizedMessage) -> ChatMessage {
    let kind = match message.kind {
        NormalizedMessageKind::User => tui::MessageKind::User,
        NormalizedMessageKind::Agent => tui::MessageKind::Agent,
        NormalizedMessageKind::ToolCall => tui::MessageKind::ToolCall,
        NormalizedMessageKind::ToolResult => tui::MessageKind::ToolResult,
        NormalizedMessageKind::System => tui::MessageKind::System,
        NormalizedMessageKind::Error => tui::MessageKind::Error,
        NormalizedMessageKind::Permission => tui::MessageKind::Permission,
    };

    ChatMessage::new(kind, message.title, message.content, message.status)
}

fn apply_control_plane_response(tui: &mut TuiState, response: ControlPlaneResponse) {
    match response {
        ControlPlaneResponse::Noop => {}
        ControlPlaneResponse::Append {
            message,
            tool_log_entry,
        } => {
            tui.add_message(normalized_to_chat_message(message));
            if let Some(entry) = tool_log_entry.filter(|entry| !entry.trim().is_empty()) {
                tui.add_tool_log_entry(entry);
                tui.tool_scroll = usize::MAX;
            }
        }
        ControlPlaneResponse::ReplaceLast {
            message,
            tool_log_entry,
        } => {
            tui.replace_last_message(normalized_to_chat_message(message));
            if let Some(entry) = tool_log_entry.filter(|entry| !entry.trim().is_empty()) {
                tui.add_tool_log_entry(entry);
                tui.tool_scroll = usize::MAX;
            }
        }
    }
}

fn dispatch_control_plane(app: &mut App, command: ControlPlaneCommand) {
    match app.transcript_control.send(command) {
        Ok(response) => apply_control_plane_response(&mut app.tui, response),
        Err(err) => app
            .tui
            .add_message(ChatMessage::error(format!("Transcript control plane failed: {}", err))),
    }
}

fn replay_session_event(app: &mut App, ev: SessionEvent) {
    match ev {
        SessionEvent::UserInput { content, .. } => {
            dispatch_control_plane(app, ControlPlaneCommand::UserMessage { content });
        }
        SessionEvent::AssistantMessage { content, .. } => {
            dispatch_control_plane(
                app,
                ControlPlaneCommand::AssistantDone {
                    content: Some(content),
                },
            );
        }
        SessionEvent::ToolCall { name, args, .. } => {
            dispatch_control_plane(app, ControlPlaneCommand::ToolCall { name, args });
        }
        SessionEvent::ToolResult { name, result, .. } => {
            dispatch_control_plane(app, ControlPlaneCommand::ToolResult { name, result });
        }
        SessionEvent::EditApplied { file, patch, .. } => {
            app.tui.update_diff(format!("Latest Diff • {}", file), &patch);
            dispatch_control_plane(
                app,
                ControlPlaneCommand::SystemMessage {
                    content: format!("Edit applied to {}", file),
                },
            );
        }
        SessionEvent::Error { message, .. } => {
            dispatch_control_plane(app, ControlPlaneCommand::ErrorMessage { content: message });
        }
        SessionEvent::System { content, .. } => {
            dispatch_control_plane(app, ControlPlaneCommand::SystemMessage { content });
        }
        SessionEvent::Verification {
            step_id,
            approved,
            ..
        } => {
            dispatch_control_plane(
                app,
                ControlPlaneCommand::SystemMessage {
                    content: format!(
                        "Verification {} for {}",
                        if approved { "approved" } else { "recorded" },
                        step_id
                    ),
                },
            );
        }
        SessionEvent::CompactBoundary { summary, .. } => {
            dispatch_control_plane(
                app,
                ControlPlaneCommand::SystemMessage {
                    content: format!("Compaction boundary: {}", summary),
                },
            );
        }
    }
}

const TOOL_CALL_SUMMARY_MAX_LINES: usize = 8;
const TOOL_CALL_SUMMARY_MAX_CHARS: usize = 480;
const PERMISSION_PREVIEW_MAX_LINES: usize = 80;
const PERMISSION_PREVIEW_MAX_CHARS: usize = 4_000;

fn collect_workspace_files(workspace_root: &str) -> Vec<String> {
    walkdir::WalkDir::new(workspace_root)
        .max_depth(3)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path().to_string_lossy();
            !p.contains("target/")
                && !p.contains(".git/")
                && e.path().extension().map_or(false, |ext| {
                    matches!(
                        ext.to_string_lossy().as_ref(),
                        "rs" | "toml" | "md" | "yaml" | "yml" | "json"
                    )
                })
        })
        .take(80)
        .map(|e| {
            e.path()
                .strip_prefix(workspace_root)
                .unwrap_or(e.path())
                .to_string_lossy()
                .to_string()
        })
        .collect()
}

fn collect_saved_sessions(session_store: &SessionStore) -> Vec<SessionEntry> {
    session_store
        .list()
        .into_iter()
        .map(|m| SessionEntry {
            id: m.id,
            created_at: m.created_at,
            event_count: m.event_count,
            workspace: m.workspace,
        })
        .collect()
}

fn refresh_tui_previews(app: &mut App) {
    refresh_file_preview(app);
    refresh_session_preview(app);
}

fn refresh_file_preview(app: &mut App) {
    let Some(idx) = app.tui.file_list_state.selected() else {
        app.tui.file_preview_title = "File Preview".to_string();
        app.tui.file_preview.clear();
        return;
    };

    let Some(relative_path) = app.tui.workspace_files.get(idx) else {
        app.tui.file_preview_title = "File Preview".to_string();
        app.tui.file_preview.clear();
        return;
    };

    let full_path = std::path::Path::new(&app.config.workspace_root).join(relative_path);
    app.tui.file_preview_title = format!("Preview • {}", relative_path);

    match std::fs::read_to_string(&full_path) {
        Ok(content) => {
            let lines: Vec<&str> = content.lines().collect();
            let preview_limit = 12usize;
            let mut preview = lines
                .iter()
                .take(preview_limit)
                .enumerate()
                .map(|(idx, line)| format!("{:>3} {}", idx + 1, line))
                .collect::<Vec<_>>();

            if lines.len() > preview_limit {
                preview.push(format!("… {} more lines", lines.len() - preview_limit));
            }

            if preview.is_empty() {
                preview.push("(empty file)".to_string());
            }

            app.tui.file_preview = preview;
        }
        Err(err) => {
            app.tui.file_preview = vec![format!("Unable to read {}: {}", relative_path, err)];
        }
    }
}

fn session_preview_line(event: &SessionEvent) -> String {
    match event {
        SessionEvent::UserInput { content, .. } => format!("You: {}", content.lines().next().unwrap_or_default()),
        SessionEvent::AssistantMessage { content, .. } => {
            format!("Barq: {}", content.lines().next().unwrap_or_default())
        }
        SessionEvent::ToolCall { name, .. } => format!("Tool call: {}", name),
        SessionEvent::ToolResult { name, result, .. } => summarize_tool_result(name, result),
        SessionEvent::EditApplied { file, .. } => format!("Edit applied: {}", file),
        SessionEvent::Error { message, .. } => format!("Error: {}", message),
        SessionEvent::Verification { step_id, approved, .. } => {
            format!("Verification {}: {}", step_id, if *approved { "approved" } else { "failed" })
        }
        SessionEvent::CompactBoundary { summary, .. } => {
            format!("Compacted: {}", summary.lines().next().unwrap_or_default())
        }
        SessionEvent::System { subtype, content, .. } => {
            format!("System [{}]: {}", subtype, content.lines().next().unwrap_or_default())
        }
    }
}

fn describe_timestamp(ts: u64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let diff = now.saturating_sub(ts);
    if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}

fn strip_fenced_block(text: &str) -> &str {
    let trimmed = text.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }

    let Some(first_newline) = trimmed.find('\n') else {
        return trimmed;
    };

    let body = &trimmed[first_newline + 1..];
    body.strip_suffix("```").unwrap_or(body).trim()
}

fn extract_preferred_text(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(text) => {
            let cleaned = text.trim();
            (!cleaned.is_empty()).then(|| cleaned.to_string())
        }
        serde_json::Value::Object(map) => {
            for key in ["final_answer", "answer", "content", "message", "reasoning"] {
                if let Some(text) = map.get(key).and_then(extract_preferred_text) {
                    if !text.trim().is_empty() {
                        return Some(text);
                    }
                }
            }
            None
        }
        serde_json::Value::Array(items) => {
            let pieces = items
                .iter()
                .filter_map(extract_preferred_text)
                .collect::<Vec<_>>();
            if pieces.is_empty() {
                None
            } else {
                Some(pieces.join("\n"))
            }
        }
        _ => None,
    }
}

fn extract_json_string_field(raw: &str, field: &str) -> Option<String> {
    let needle = format!("\"{}\"", field);
    let start = raw.find(&needle)?;
    let after_field = &raw[start + needle.len()..];
    let colon = after_field.find(':')?;
    let mut chars = after_field[colon + 1..].trim_start().chars();
    if chars.next()? != '"' {
        return None;
    }

    let mut extracted = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            extracted.push(match ch {
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                '\\' => '\\',
                '"' => '"',
                other => other,
            });
            escaped = false;
            continue;
        }

        match ch {
            '\\' => escaped = true,
            '"' => return Some(extracted),
            other => extracted.push(other),
        }
    }

    (!extracted.trim().is_empty()).then_some(extracted)
}

fn sanitize_assistant_text(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let unfenced = strip_fenced_block(trimmed);
    if let Ok(value) = serde_json::from_str::<serde_json::Value>(unfenced) {
        if let Some(text) = extract_preferred_text(&value) {
            return text;
        }
    }

    for field in ["final_answer", "answer", "content", "message", "reasoning"] {
        if let Some(text) = extract_json_string_field(unfenced, field) {
            if !text.trim().is_empty() {
                return text;
            }
        }
    }

    let looks_structured = unfenced.starts_with('{')
        || unfenced.starts_with('[')
        || trimmed.starts_with("```json")
        || trimmed.starts_with("```JSON");

    if looks_structured {
        String::new()
    } else {
        raw.to_string()
    }
}

fn set_agent_transcript_message(tui: &mut TuiState, text: &str) {
    if text.trim().is_empty() {
        return;
    }

    match tui.messages.last_mut() {
        Some(last) if matches!(last.kind, tui::MessageKind::Agent) => {
            last.content = text.to_string();
        }
        _ => tui.add_message(ChatMessage::agent(text)),
    }
}

fn refresh_session_preview(app: &mut App) {
    let Some(idx) = app.tui.session_list_state.selected() else {
        app.tui.session_preview_title = "Session Preview".to_string();
        app.tui.session_preview.clear();
        return;
    };

    let Some(session) = app.tui.sessions.get(idx) else {
        app.tui.session_preview_title = "Session Preview".to_string();
        app.tui.session_preview.clear();
        return;
    };

    app.tui.session_preview_title = format!("Preview • {}", session.id);
    match app.session_store.load(&session.id) {
        Ok(events) => {
            let mut preview = vec![
                format!("Workspace: {}", session.workspace),
                format!("Created: {}", describe_timestamp(session.created_at)),
                format!("Events: {}", events.len()),
                String::new(),
                "Recent activity:".to_string(),
            ];

            let recent = events.iter().rev().take(12).rev().map(session_preview_line);
            preview.extend(recent);
            app.tui.session_preview = preview;
        }
        Err(err) => {
            app.tui.session_preview = vec![format!("Unable to load {}: {}", session.id, err)];
        }
    }
}

fn refresh_tui_metadata(app: &mut App) {
    let file_selection = app.tui.file_list_state.selected().unwrap_or(0);
    let session_selection = app.tui.session_list_state.selected().unwrap_or(0);

    app.tui.workspace_files = collect_workspace_files(&app.config.workspace_root);
    app.tui.sessions = collect_saved_sessions(&app.session_store);

    if app.tui.workspace_files.is_empty() {
        app.tui.file_list_state.select(None);
    } else {
        app.tui.file_list_state.select(Some(file_selection.min(app.tui.workspace_files.len() - 1)));
    }

    if app.tui.sessions.is_empty() {
        app.tui.session_list_state.select(None);
    } else {
        app.tui.session_list_state.select(Some(session_selection.min(app.tui.sessions.len() - 1)));
    }

    refresh_tui_previews(app);
}

fn summarize_tool_result(name: &str, result: &serde_json::Value) -> String {
    if let Some(error) = result.get("error").and_then(|v| v.as_str()) {
        return format!("{} failed: {}", name, error);
    }

    match name {
        "edit_file" => {
            let path = result
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");

            if result.get("preview").and_then(|v| v.as_bool()).unwrap_or(false) {
                format!("Preview edit for {}", path)
            } else if result.get("applied").and_then(|v| v.as_bool()).unwrap_or(false) {
                format!("Updated {}", path)
            } else {
                format!("Processed edit request for {}", path)
            }
        }
        "create_file" => {
            let path = result.get("path").and_then(|v| v.as_str()).unwrap_or("<unknown>");
            if result.get("created").and_then(|v| v.as_bool()).unwrap_or(false) {
                format!("Created {}", path)
            } else {
                format!("Processed create request for {}", path)
            }
        }
        "shell_exec" => {
            let exit_code = result.get("exit_code").and_then(|v| v.as_i64()).unwrap_or(-1);
            let timed_out = result.get("timed_out").and_then(|v| v.as_bool()).unwrap_or(false);
            if timed_out {
                format!("shell_exec timed out (exit {})", exit_code)
            } else {
                format!("shell_exec completed with exit {}", exit_code)
            }
        }
        _ => serde_json::to_string(result)
            .map(|raw| {
                let compact = raw.replace('\n', " ");
                if compact.len() > 120 {
                    format!("{}: {}...", name, &compact[..120])
                } else {
                    format!("{}: {}", name, compact)
                }
            })
            .unwrap_or_else(|_| format!("{} completed", name)),
    }
}

fn extract_edit_patch(name: &str, result: &serde_json::Value) -> Option<(String, String)> {
    let patch = result.get("diff").and_then(|v| v.as_str())?;

    match name {
        "edit_file" => {
            let applied = result.get("applied").and_then(|v| v.as_bool()).unwrap_or(false);
            if !applied {
                return None;
            }
            let path = result.get("file_path").and_then(|v| v.as_str())?;
            Some((path.to_string(), patch.to_string()))
        }
        "create_file" => {
            let created = result.get("created").and_then(|v| v.as_bool()).unwrap_or(false);
            if !created {
                return None;
            }
            let path = result.get("path").and_then(|v| v.as_str())?;
            Some((path.to_string(), patch.to_string()))
        }
        _ => None,
    }
}

fn build_permission_preview(path: &str, old: Option<&str>, new: Option<&str>) -> String {
    let mut lines = vec![
        format!("--- a/{}", path),
        format!("+++ b/{}", path),
        "@@ pending @@".to_string(),
    ];

    if let Some(old) = old {
        for line in old.lines() {
            lines.push(format!("-{}", line));
        }
    }

    if let Some(new) = new {
        for line in new.lines() {
            lines.push(format!("+{}", line));
        }
    }

    if lines.len() == 3 {
        lines.push("Preview unavailable".to_string());
    }

    truncate_multiline(
        &lines.join("\n"),
        PERMISSION_PREVIEW_MAX_LINES,
        PERMISSION_PREVIEW_MAX_CHARS,
    )
}

fn truncate_inline(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }

    let mut shortened = String::new();
    for ch in trimmed.chars().take(max_chars.saturating_sub(1)) {
        shortened.push(ch);
    }
    shortened.push('…');
    shortened
}

fn truncate_multiline(text: &str, max_lines: usize, max_chars: usize) -> String {
    let mut kept = Vec::new();
    let mut used_chars = 0usize;
    let mut truncated = false;
    let all_lines: Vec<&str> = text.lines().collect();
    let total_lines = all_lines.len();
    let total_chars = text.chars().count();

    for line in &all_lines {
        let line_len = line.chars().count();
        let next_chars = used_chars + line_len + usize::from(!kept.is_empty());
        if kept.len() >= max_lines || next_chars > max_chars {
            truncated = true;
            break;
        }
        kept.push((*line).to_string());
        used_chars = next_chars;
    }

    if kept.is_empty() && !text.is_empty() {
        return truncate_inline(text, max_chars.max(1));
    }

    if truncated || total_lines > kept.len() || total_chars > used_chars {
        kept.push(format!(
            "… truncated {} more lines ({} chars total)",
            total_lines.saturating_sub(kept.len()),
            total_chars
        ));
    }

    kept.join("\n")
}

fn content_stats(label: &str, text: Option<&str>) -> Option<String> {
    let text = text?;
    Some(format!(
        "{}: {} lines / {} chars",
        label,
        text.lines().count().max(1),
        text.chars().count()
    ))
}

fn summarize_tool_call(name: &str, args: &serde_json::Value) -> String {
    match name {
        "edit_file" => {
            let path = args
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            let mut details = Vec::new();
            if let Some(patch) = args.get("patch").and_then(|v| v.as_str()) {
                details.push(format!(
                    "patch: {} lines / {} chars",
                    patch.lines().count().max(1),
                    patch.chars().count()
                ));
            } else {
                if let Some(old) = content_stats("old", args.get("old_string").and_then(|v| v.as_str())) {
                    details.push(old);
                }
                if let Some(new) = content_stats("new", args.get("new_string").and_then(|v| v.as_str())) {
                    details.push(new);
                }
            }

            if details.is_empty() {
                format!("Calling edit_file on {}", path)
            } else {
                format!("Calling edit_file on {}\n{}", path, details.join("\n"))
            }
        }
        "create_file" => {
            let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("<unknown>");
            let content = args.get("content").and_then(|v| v.as_str());
            let detail = content_stats("content", content).unwrap_or_else(|| "content: unavailable".to_string());
            format!("Calling create_file for {}\n{}", path, detail)
        }
        "shell_exec" => {
            let command = args.get("command").and_then(|v| v.as_str()).unwrap_or("<unknown>");
            let working_dir = args
                .get("working_dir")
                .and_then(|v| v.as_str())
                .filter(|dir| !dir.is_empty() && *dir != ".")
                .unwrap_or(".");
            if working_dir == "." {
                format!("Calling shell_exec: {}", truncate_inline(command, 140))
            } else {
                format!(
                    "Calling shell_exec: {}\nworking_dir: {}",
                    truncate_inline(command, 120),
                    working_dir
                )
            }
        }
        "git_ops" => {
            let operation = args.get("operation").and_then(|v| v.as_str()).unwrap_or("status");
            let git_args = args.get("args").and_then(|v| v.as_str()).unwrap_or("");
            let command = if git_args.trim().is_empty() {
                format!("git {}", operation)
            } else {
                format!("git {} {}", operation, git_args.trim())
            };
            format!("Calling git_ops: {}", truncate_inline(&command, 140))
        }
        _ => {
            let pretty = serde_json::to_string_pretty(args)
                .unwrap_or_else(|_| args.to_string());
            format!(
                "Calling {} with\n{}",
                name,
                truncate_multiline(&pretty, TOOL_CALL_SUMMARY_MAX_LINES, TOOL_CALL_SUMMARY_MAX_CHARS)
            )
        }
    }
}

fn build_pending_action(name: &str, args: &serde_json::Value, reason: &str) -> tui::PendingAction {
    match name {
        "edit_file" => {
            let path = args
                .get("file_path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            let patch = args
                .get("patch")
                .and_then(|v| v.as_str())
                .map(|s| truncate_multiline(s, PERMISSION_PREVIEW_MAX_LINES, PERMISSION_PREVIEW_MAX_CHARS))
                .unwrap_or_else(|| {
                    build_permission_preview(
                        &path,
                        args.get("old_string").and_then(|v| v.as_str()),
                        args.get("new_string").and_then(|v| v.as_str()),
                    )
                });
            tui::PendingAction::write_file(path, patch, name)
        }
        "create_file" => {
            let path = args
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>")
                .to_string();
            let content = args
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let patch = build_permission_preview(&path, None, Some(&content));
            tui::PendingAction::write_file(path, patch, name)
        }
        "shell_exec" => {
            let command = args
                .get("command")
                .and_then(|v| v.as_str())
                .unwrap_or("<unknown>");
            tui::PendingAction::shell_cmd(command, reason, name)
        }
        "git_ops" => {
            let op = args.get("operation").and_then(|v| v.as_str()).unwrap_or("status");
            let git_args = args.get("args").and_then(|v| v.as_str()).unwrap_or("");
            let command = if git_args.is_empty() {
                format!("git {}", op)
            } else {
                format!("git {} {}", op, git_args)
            };
            tui::PendingAction::shell_cmd(command, reason, name)
        }
        _ => tui::PendingAction::shell_cmd(name, reason, name),
    }
}

fn remember_permission_scope(app: &App, request: &PendingPermissionRequest) -> anyhow::Result<String> {
    match request.name.as_str() {
        "edit_file" | "create_file" => {
            app.orchestrator.permissions.allow_edits_for_session();
            Ok("Accepted edits for the rest of this session".to_string())
        }
        "shell_exec" => {
            let command = request
                .args
                .get("command")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing shell command"))?;
            app.orchestrator.permissions.remember_shell_command(command)?;
            Ok(format!(
                "Saved local allow rule for Bash({}) in .claude/settings.local.json",
                command
            ))
        }
        "git_ops" => {
            let operation = request
                .args
                .get("operation")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("Missing git operation"))?;
            let git_args = request
                .args
                .get("args")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            app.orchestrator
                .permissions
                .remember_git_command(operation, git_args)?;
            let command = if git_args.trim().is_empty() {
                format!("git {}", operation)
            } else {
                format!("git {} {}", operation, git_args.trim())
            };
            Ok(format!(
                "Saved local allow rule for Git({}) in .claude/settings.local.json",
                command
            ))
        }
        _ => {
            app.orchestrator.permissions.auto_allow_tool(&request.name);
            Ok(format!("Auto-allowed {} for this session", request.name))
        }
    }
}

fn approval_hint(name: &str) -> &'static str {
    match name {
        "edit_file" | "create_file" => "[Y] once  [A] accept edits this session  [N] deny",
        "shell_exec" | "git_ops" => "[Y] once  [A] save local allow rule  [N] deny",
        _ => "[Y] once  [A] remember for this session  [N] deny",
    }
}

fn show_permission_prompt(app: &mut App) {
    if let Some(next) = app.pending_permission_requests.front() {
        let hint = approval_hint(&next.name).to_string();
        app.tui.permission_prompt = Some(tui::PermissionPrompt {
            title: format!("Tool: {}", next.name),
            reason: next.reason.clone(),
            hint: hint.clone(),
            queue_len: app.pending_permission_requests.len(),
        });
        app.tui.set_status(
            format!("Pending approval: '{}' [{} queued]", next.name, app.pending_permission_requests.len()),
            false,
        );
    }
}

fn queue_permission_request(
    app: &mut App,
    name: String,
    args: serde_json::Value,
    reason: String,
    tx: tokio::sync::oneshot::Sender<bool>,
) {
    let action = build_pending_action(&name, &args, &reason);
    dispatch_control_plane(
        app,
        ControlPlaneCommand::PermissionRequest {
            name: name.clone(),
            args: args.clone(),
            reason: reason.clone(),
        },
    );
    app.pending_permission_requests.push_back(PendingPermissionRequest {
        name,
        args,
        reason,
        tx,
    });
    app.tui.action_queue.push(action);
    if app.tui.action_queue.len() == 1 {
        app.tui.action_queue_selected = 0;
    } else {
        app.tui.action_queue_selected = app.tui.action_queue_selected.min(app.tui.action_queue.len() - 1);
    }
    app.tui.action_preview_scroll = 0;
    show_permission_prompt(app);
}

fn resolve_permission_request(app: &mut App, index: usize, decision: PermissionDecision) {
    if index >= app.pending_permission_requests.len() || index >= app.tui.action_queue.len() {
        return;
    }

    if let Some(request) = app.pending_permission_requests.remove(index) {
        let approval = matches!(
            decision,
            PermissionDecision::AllowOnce | PermissionDecision::AllowRemembered
        );
        let control_plane_decision = if approval {
            ControlPlanePermissionDecision::Approved
        } else {
            ControlPlanePermissionDecision::Denied
        };
        dispatch_control_plane(
            app,
            ControlPlaneCommand::PermissionResolved {
                name: request.name.clone(),
                decision: control_plane_decision,
            },
        );

        match decision {
            PermissionDecision::AllowOnce => {
                let _ = request.tx.send(true);
                app.tui
                    .add_message(ChatMessage::system(format!("Approved once: {}", request.name)));
            }
            PermissionDecision::AllowRemembered => match remember_permission_scope(app, &request) {
                Ok(summary) => {
                    let _ = request.tx.send(true);
                    app.tui.add_message(ChatMessage::system(format!(
                        "Approved and remembered: {}\n{}",
                        request.name, summary
                    )));
                }
                Err(err) => {
                    let _ = request.tx.send(true);
                    app.tui.add_message(ChatMessage::error(format!(
                        "Approved {}, but failed to persist scope: {}",
                        request.name, err
                    )));
                }
            },
            PermissionDecision::Deny => {
                let _ = request.tx.send(false);
                app.tui
                    .add_message(ChatMessage::system(format!("Denied: {}", request.name)));
            }
        }
    }

    app.tui.action_queue.remove(index);
    if app.tui.action_queue.is_empty() {
        app.tui.action_queue_selected = 0;
    } else {
        app.tui.action_queue_selected = app.tui.action_queue_selected.min(app.tui.action_queue.len() - 1);
    }
    app.tui.action_preview_scroll = 0;

    if app.pending_permission_requests.is_empty() {
        app.tui.permission_prompt = None;
        app.tui.clear_status();
        app.tui.is_thinking = true;
    } else {
        show_permission_prompt(app);
    }
}

fn deny_all_permission_requests(app: &mut App) {
    while !app.pending_permission_requests.is_empty() && !app.tui.action_queue.is_empty() {
        resolve_permission_request(app, 0, PermissionDecision::Deny);
    }
    app.tui.permission_prompt = None;
    app.tui.is_thinking = true;
}

impl App {
    fn new(resume_id: Option<String>, cli: &Cli) -> anyhow::Result<Self> {
        let mut config = Config::load();
        // Apply CLI overrides
        if let Some(m) = &cli.model { config.ollama_model = m.clone(); }
        if let Some(w) = &cli.workspace { config.workspace_root = w.clone(); }
        if let Some(u) = &cli.ollama_url { config.ollama_base_url = u.clone(); }
        if let Some(t) = cli.max_turns { config.max_iterations = t; }
        let agent = crate::providers::build_provider(&config);
        let barq = Arc::new(BarqIndex::new(&config).expect("Failed to create BarqIndex"));
        let mut tools_mut = ToolRegistry::with_barq(Arc::clone(&barq));
        tools_mut.register(Box::new(crate::tools::delegate::DelegateTask::new(
            Arc::clone(&agent),
            Arc::clone(&barq),
        )));
        let task_board = Arc::new(crate::tasks::TaskBoard::new());
        tools_mut.register(Box::new(crate::tools::task_tools::TaskCreateTool::new(Arc::clone(&task_board))));
        tools_mut.register(Box::new(crate::tools::task_tools::TaskUpdateTool::new(Arc::clone(&task_board))));
        tools_mut.register(Box::new(crate::tools::task_tools::TaskListTool::new(Arc::clone(&task_board))));

        let tools = Arc::new(tools_mut);
        let orchestrator = Orchestrator::new(
            Arc::clone(&agent),
            Arc::clone(&tools),
            Arc::clone(&barq),
            config.clone(),
        );
        let coordinator = Arc::new(CoordinatorAgent::new(
            Arc::clone(&agent),
            Arc::clone(&barq),
            tools,
        ));

        let session_store = SessionStore::new(&config.workspace_root);
        
        let session_id = resume_id.unwrap_or_else(|| {
            format!(
                "session_{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            )
        });

        // Session ID is used for JSONL transcript appending
        let _active_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Load workspace files for sidebar
        let workspace_files = collect_workspace_files(&config.workspace_root);
        let saved_sessions = collect_saved_sessions(&session_store);

        let token_limit = config.token_limit;
        let model = config.active_model_label();
        let transcript_control = TranscriptControlBridge::new()?;

        let mut tui = TuiState::new(token_limit, model, session_id.clone());
        tui.workspace_files = workspace_files;
        tui.sessions = saved_sessions;

        let mut app = Self {
            tui,
            orchestrator,
            config,
            coordinator,
            event_rx: None,
            session_store,
            session_id,
            transcript_control,
            pending_permission_requests: std::collections::VecDeque::new(),
            pending_budget_request: None,
            cost: CostTracker::new(),
            skip_permissions: cli.dangerously_skip_permissions,
            streaming_assistant_raw: String::new(),
        };
        refresh_tui_metadata(&mut app);
        Ok(app)
    }

    /// Load past session events into the UI
    fn load_session(&mut self) {
        let _ = self.transcript_control.send(ControlPlaneCommand::Reset);
        self.tui.messages.clear();
        self.tui.tool_log.clear();
        for ev in self.session_store.replay(&self.session_id) {
            replay_session_event(self, ev);
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Entry point
// ─────────────────────────────────────────────────────────────────────────────
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // LSP mode
    if cli.lsp {
        lsp::start_lsp().await;
        return Ok(());
    }

    // Doctor subcommand
    if let Some(Commands::Doctor) = &cli.command {
        return run_doctor(&cli).await;
    }

    // Sessions subcommand
    if let Some(Commands::Sessions { show, delete }) = &cli.command {
        let workspace = cli.workspace.clone().unwrap_or_else(|| ".".to_string());
        let store = SessionStore::new(&workspace);
        if let Some(sid) = show {
            for ev in store.replay(sid) {
                println!("{:?}", ev);
            }
        } else if let Some(sid) = delete {
            println!("Delete not yet implemented for session: {}", sid);
        } else {
            for m in store.list() {
                println!("{} | {} events | {}", m.id, m.event_count, m.workspace);
            }
        }
        return Ok(());
    }

    // Memory subcommand
    if let Some(Commands::Memory { add, show }) = &cli.command {
        let workspace = cli.workspace.clone().unwrap_or_else(|| ".".to_string());
        if let Some(note) = add {
            memory::Memory::append(&workspace, note)?;
            println!("Memory updated.");
        } else if *show {
            println!("{}", memory::Memory::show(&workspace));
        } else {
            println!("{}", memory::Memory::show(&workspace));
        }
        return Ok(());
    }

    // Print/headless subcommand
    if let Some(Commands::Print { prompt, json }) = &cli.command {
        return run_headless(prompt, *json, &cli).await;
    }

    // Index subcommand
    if let Some(Commands::Index { path }) = &cli.command {
        let workspace = path.clone()
            .or_else(|| cli.workspace.clone())
            .unwrap_or_else(|| ".".to_string());
        println!("Indexing workspace: {}", workspace);
        let config = Config::load();
        let barq = BarqIndex::new(&config)?;
        println!("Indexing complete ({} documents).", 0);
        return Ok(());
    }

    // Resolve resume_id from --continue or --resume
    let resume_id = if cli.r#continue {
        let workspace = cli.workspace.clone().unwrap_or_else(|| ".".to_string());
        SessionStore::new(&workspace).last_session_id()
    } else {
        cli.resume.clone()
    };

    tokio::spawn(start_health_server());

    let mut app = App::new(resume_id.clone(), &cli)?;
    if resume_id.is_some() {
        app.load_session();
    }

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture,
        crossterm::event::EnableBracketedPaste
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    let res = run_app(&mut terminal, &mut app).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture,
        crossterm::event::DisableBracketedPaste
    )?;

    if let Err(err) = res {
        eprintln!("{:?}", err);
    }

    Ok(())
}

/// Single-shot headless run — no TUI.
async fn run_headless(prompt: &str, json_mode: bool, cli: &Cli) -> anyhow::Result<()> {
    let mut config = Config::load();
    if let Some(m) = &cli.model { config.ollama_model = m.clone(); }
    if let Some(w) = &cli.workspace { config.workspace_root = w.clone(); }
    if let Some(u) = &cli.ollama_url { config.ollama_base_url = u.clone(); }
    if let Some(t) = cli.max_turns { config.max_iterations = t; }

    let agent = crate::providers::build_provider(&config);
    let barq = Arc::new(BarqIndex::new(&config)?);
    let mut tools_mut = ToolRegistry::with_barq(Arc::clone(&barq));
    tools_mut.register(Box::new(crate::tools::delegate::DelegateTask::new(
        Arc::clone(&agent),
        Arc::clone(&barq),
    )));
    let task_board = Arc::new(crate::tasks::TaskBoard::new());
    tools_mut.register(Box::new(crate::tools::task_tools::TaskCreateTool::new(Arc::clone(&task_board))));
    tools_mut.register(Box::new(crate::tools::task_tools::TaskUpdateTool::new(Arc::clone(&task_board))));
    tools_mut.register(Box::new(crate::tools::task_tools::TaskListTool::new(Arc::clone(&task_board))));
    let tools = Arc::new(tools_mut);
    let mut orch = Orchestrator::new(Arc::clone(&agent), tools, barq, config.clone());
    let rx = orch.run(prompt);
    let mut full_response = String::new();
    let mut tool_calls_used: Vec<String> = Vec::new();
    let mut rx = rx;

    while let Some(ev) = rx.recv().await {
        match ev {
            OrchestratorEvent::Token(t) => full_response.push_str(&t),
            OrchestratorEvent::ToolCall { name, .. } => tool_calls_used.push(name),
            OrchestratorEvent::Done(s) => { full_response = s; break; }
            OrchestratorEvent::Error(e) => {
                if json_mode {
                    println!("{}", serde_json::json!({"error": e}));
                } else {
                    eprintln!("Error: {}", e);
                }
                return Ok(());
            }
            _ => {}
        }
    }

    if json_mode {
        println!("{}", serde_json::json!({
            "response": full_response,
            "tool_calls": tool_calls_used,
            "model": config.active_model_label(),
        }));
    } else {
        println!("{}", full_response);
    }
    Ok(())
}

/// Doctor: check Ollama connectivity.
async fn run_doctor(cli: &Cli) -> anyhow::Result<()> {
    let mut config = Config::load();
    if let Some(u) = &cli.ollama_url { config.ollama_base_url = u.clone(); }
    if let Some(m) = &cli.model { config.ollama_model = m.clone(); }
    println!("Checking Ollama at {} ...", config.ollama_base_url);
    let url = format!("{}/api/tags", config.ollama_base_url);
    match reqwest::get(&url).await {
        Ok(r) if r.status().is_success() => println!("Ollama is reachable. Model: {}", config.ollama_model),
        Ok(r) => println!("Ollama responded with status: {}", r.status()),
        Err(e) => println!("Cannot reach Ollama: {}", e),
    }

    match check_node_runtime() {
        Ok(version) => println!("Node runtime OK: {}", version),
        Err(err) => println!("Node runtime check failed: {}", err),
    }

    match resolve_control_plane_script() {
        Ok(path) => println!("Control plane asset OK: {}", path.display()),
        Err(err) => println!("Control plane asset check failed: {}", err),
    }

    match TranscriptControlBridge::new() {
        Ok(_) => println!("Transcript control plane launch OK"),
        Err(err) => println!("Transcript control plane launch failed: {}", err),
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Main event loop — uses tokio::select! to concurrently handle:
//   1. Orchestrator events (tokens, tool calls, done, errors)
//   2. Terminal input (keyboard, mouse, paste, resize)
//   3. Tick timer (spinner animation, status timeout)
// This ensures streaming tokens render immediately without blocking on input.
// ─────────────────────────────────────────────────────────────────────────────
async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> anyhow::Result<()> {
    // Use crossterm's async EventStream so we can select! alongside channels
    let mut term_events = EventStream::new();

    // Tick interval: 50ms when thinking (20fps for smooth animation),
    // 120ms when idle (lower CPU usage)
    let mut tick_interval = tokio::time::interval(Duration::from_millis(50));
    tick_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        terminal.draw(|f| tui::draw(f, &mut app.tui))?;
        let mut refresh_metadata = false;
        let mut queued_permissions = Vec::new();

        // ── Select: wait for the FIRST of these to fire ──────────────────────
        // Take receiver out to avoid borrow conflicts in select!
        let mut taken_rx = app.event_rx.take();

        tokio::select! {
            // Branch 1: Orchestrator event (highest priority when streaming)
            Some(orch_event) = async {
                if let Some(rx) = &mut taken_rx {
                    rx.recv().await
                } else {
                    std::future::pending::<Option<OrchestratorEvent>>().await
                }
            } => {
                // Put receiver back before processing
                app.event_rx = taken_rx;
                // Process the received event
                process_orchestrator_event(
                    app,
                    orch_event,
                    &mut refresh_metadata,
                    &mut queued_permissions,
                );
                // Drain any additional buffered events
                let mut drained = Vec::new();
                if let Some(rx) = &mut app.event_rx {
                    while let Ok(ev) = rx.try_recv() {
                        drained.push(ev);
                    }
                }
                for ev in drained {
                    process_orchestrator_event(
                        app,
                        ev,
                        &mut refresh_metadata,
                        &mut queued_permissions,
                    );
                }
            }

            // Branch 2: Terminal input event
            Some(Ok(term_event)) = term_events.next() => {
                app.event_rx = taken_rx; // restore receiver
                match term_event {
                    Event::Key(key) if key.kind == KeyEventKind::Press => {
                        handle_key(app, key.code, key.modifiers);
                    }
                    Event::Paste(text) => {
                        if app.pending_permission_requests.is_empty()
                            && app.pending_budget_request.is_none()
                        {
                            if app.tui.active_tab != tui::ActiveTab::Chat {
                                app.tui.active_tab = tui::ActiveTab::Chat;
                            }
                            app.tui.focus = tui::Focus::Input;
                            app.tui.input_insert_str(&text);
                            let lc = text.lines().count().max(1);
                            app.tui.set_status(
                                format!("Pasted {} lines / {} chars", lc, text.chars().count()),
                                false,
                            );
                        }
                    }
                    Event::Mouse(m) => handle_mouse(app, m),
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }

            // Branch 3: Tick timer (animation, status timeout)
            _ = tick_interval.tick() => {
                app.event_rx = taken_rx; // restore receiver
            }
        }

        // ── Post-select housekeeping ─────────────────────────────────────────
        if refresh_metadata {
            refresh_tui_metadata(app);
        }

        for (name, args, reason, tx) in queued_permissions.drain(..) {
            queue_permission_request(app, name, args, reason, tx);
        }

        // Tick counter for spinners
        app.tui.tick = app.tui.tick.wrapping_add(1);

        if app.tui.status_message.is_some() && app.tui.tick % 50 == 0 {
            app.tui.clear_status();
        }

        // ── Quit ──────────────────────────────────────────────────────────────
        if app.tui.should_quit() {
            return Ok(());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Process a single orchestrator event
// ─────────────────────────────────────────────────────────────────────────────
fn process_orchestrator_event(
    app: &mut App,
    event: OrchestratorEvent,
    refresh_metadata: &mut bool,
    queued_permissions: &mut Vec<(String, serde_json::Value, String, tokio::sync::oneshot::Sender<bool>)>,
) {
    match event {
        OrchestratorEvent::Token(t) => {
            app.tui.is_thinking = true;
            app.streaming_assistant_raw.push_str(&t);
            dispatch_control_plane(
                app,
                ControlPlaneCommand::AssistantToken { token: t },
            );
        }
        OrchestratorEvent::ToolCall { name, args } => {
            app.tui.current_tool = Some(name.clone());
            dispatch_control_plane(
                app,
                ControlPlaneCommand::ToolCall {
                    name: name.clone(),
                    args: args.clone(),
                },
            );
            let _ = app.session_store.append(
                &app.session_id,
                &SessionEvent::tool_call(&name, args.clone()),
            );
        }
        OrchestratorEvent::ToolResult { name, result } => {
            app.tui.current_tool = None;
            dispatch_control_plane(
                app,
                ControlPlaneCommand::ToolResult {
                    name: name.clone(),
                    result: result.clone(),
                },
            );
            let _ = app.session_store.append(
                &app.session_id,
                &SessionEvent::tool_result(&name, result.clone()),
            );

            if let Some((file, patch)) = extract_edit_patch(&name, &result) {
                app.tui.update_diff(format!("Latest Diff \u{2022} {}", file), &patch);
                let _ = app.session_store.append(
                    &app.session_id,
                    &SessionEvent::edit_applied(&file, &patch),
                );
                *refresh_metadata = true;
            }
        }
        OrchestratorEvent::PermissionRequested { name, args, reason, tx } => {
            app.tui.current_tool = None;
            if app.skip_permissions {
                let _ = tx.send(true);
                app.tui.tool_log.push(format!("Auto-approved: {} (skip_permissions)", name));
                app.tui.tool_scroll = usize::MAX;
            } else {
                app.tui.is_thinking = false;
                queued_permissions.push((name, args, reason, tx));
            }
        }
        OrchestratorEvent::BudgetWarning { used_usd, cap_usd, pct } => {
            app.tui.set_status(
                format!("\u{26A0} Budget: ${:.4} / ${:.2} ({}%)", used_usd, cap_usd, pct),
                false,
            );
        }
        OrchestratorEvent::BudgetPaused { used_usd, cap_usd, tx } => {
            app.tui.is_thinking = false;
            app.tui.add_message(ChatMessage::system(format!(
                "Budget cap reached! Used ${:.4} of ${:.2}.\nPress [Y] to continue anyway or [N] to stop.",
                used_usd, cap_usd
            )));
            app.tui.set_status(
                format!("BUDGET CAP: ${:.4} / ${:.2}", used_usd, cap_usd),
                true,
            );
            app.pending_budget_request = Some(tx);
        }
        OrchestratorEvent::Done(answer) => {
            app.tui.is_thinking = false;
            app.tui.current_tool = None;
            dispatch_control_plane(
                app,
                ControlPlaneCommand::AssistantDone {
                    content: Some(answer.clone()),
                },
            );
            app.tui.set_status("Done", false);
            app.event_rx = None;
            let _ = app
                .session_store
                .append(&app.session_id, &SessionEvent::assistant(&answer));
            app.streaming_assistant_raw.clear();
            *refresh_metadata = true;
        }
        OrchestratorEvent::Error(err) => {
            app.tui.is_thinking = false;
            app.tui.current_tool = None;
            dispatch_control_plane(
                app,
                ControlPlaneCommand::ErrorMessage {
                    content: err.clone(),
                },
            );
            app.tui.set_status(format!("Error: {}", err), true);
            app.event_rx = None;
            let _ = app.session_store.append(&app.session_id, &SessionEvent::error(&err));
            app.streaming_assistant_raw.clear();
            *refresh_metadata = true;
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Key handling
// ─────────────────────────────────────────────────────────────────────────────
fn handle_key(app: &mut App, key: KeyCode, mods: KeyModifiers) {
    // ── PERMISSION GATE ─────────────────────────────────────────────────
    // When there are pending permission requests, ALL key input is
    // captured here. Y/N/A resolve the selected sandbox action, Esc denies
    // everything, and ActionQueue navigation keys stay enabled.
    if !app.pending_permission_requests.is_empty() {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let selected = if app.tui.active_tab == ActiveTab::ActionQueue {
                    app.tui.action_queue_selected.min(app.pending_permission_requests.len().saturating_sub(1))
                } else {
                    0
                };
                resolve_permission_request(app, selected, PermissionDecision::AllowOnce);
                return;
            }
            KeyCode::Char('a') | KeyCode::Char('A') => {
                let selected = if app.tui.active_tab == ActiveTab::ActionQueue {
                    app.tui.action_queue_selected.min(app.pending_permission_requests.len().saturating_sub(1))
                } else {
                    0
                };
                resolve_permission_request(app, selected, PermissionDecision::AllowRemembered);
                return;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                let selected = if app.tui.active_tab == ActiveTab::ActionQueue {
                    app.tui.action_queue_selected.min(app.pending_permission_requests.len().saturating_sub(1))
                } else {
                    0
                };
                resolve_permission_request(app, selected, PermissionDecision::Deny);
                return;
            }
            KeyCode::Esc => {
                deny_all_permission_requests(app);
                return;
            }
            KeyCode::Tab => {
                app.tui.active_tab = app.tui.active_tab.next();
                return;
            }
            KeyCode::BackTab => {
                app.tui.active_tab = app.tui.active_tab.prev();
                return;
            }
            _ => {
                if app.tui.active_tab == ActiveTab::ActionQueue {
                    handle_action_queue_keys(app, key);
                }
                return;
            }
        }
    }

    // ── BUDGET GATE ──────────────────────────────────────────────────────
    if app.pending_budget_request.is_some() {
        match key {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(tx) = app.pending_budget_request.take() {
                    let _ = tx.send(true);
                    app.tui.add_message(ChatMessage::system("Budget override: continuing."));
                    app.tui.is_thinking = true;
                }
                return;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                if let Some(tx) = app.pending_budget_request.take() {
                    let _ = tx.send(false);
                    app.tui.add_message(ChatMessage::system("Stopped. Budget cap enforced."));
                    app.event_rx = None;
                }
                return;
            }
            _ => { return; }
        }
    }

    // Global: Esc → quit
    if key == KeyCode::Esc {
        app.tui.mark_quit();
        return;
    }

    // Tab switching: Tab / Shift+Tab
    if key == KeyCode::Tab {
        app.tui.active_tab = app.tui.active_tab.next();
        return;
    }
    if key == KeyCode::BackTab {
        app.tui.active_tab = app.tui.active_tab.prev();
        return;
    }

    // Alt+S → toggle sidebar
    if mods == KeyModifiers::ALT && key == KeyCode::Char('s') {
        app.tui.sidebar_visible = !app.tui.sidebar_visible;
        return;
    }

    if key == KeyCode::F(1) {
        app.tui.focus = cycle_focus(app.tui.focus, app.tui.sidebar_visible);
        return;
    }

    // Per-tab handling
    match app.tui.active_tab {
        ActiveTab::Chat => handle_chat_keys(app, key, mods),
        ActiveTab::Diff => handle_diff_keys(app, key),
        ActiveTab::Sessions => handle_sessions_keys(app, key),
        ActiveTab::ActionQueue => handle_action_queue_keys(app, key),
    }
}

fn cycle_focus(current: Focus, sidebar_visible: bool) -> Focus {
    match current {
        Focus::Input => {
            if sidebar_visible {
                Focus::Sidebar
            } else {
                Focus::Chat
            }
        }
        Focus::Sidebar => Focus::Chat,
        Focus::Chat => Focus::ToolLog,
        Focus::ToolLog => Focus::Input,
    }
}

fn handle_chat_keys(app: &mut App, key: KeyCode, mods: KeyModifiers) {
    // ── Autocomplete interception ──
    if app.tui.is_autocomplete_active() {
        match key {
            KeyCode::Up => { app.tui.autocomplete_up(); return; }
            KeyCode::Down => { app.tui.autocomplete_down(); return; }
            KeyCode::Tab => { app.tui.autocomplete_accept(); return; }
            KeyCode::Enter => {
                app.tui.autocomplete_accept();
                if let Some(input) = app.tui.commit_input() {
                    submit_input(app, &input);
                }
                return;
            }
            KeyCode::Esc => {
                app.tui.input.clear();
                app.tui.input_cursor = 0;
                app.tui.autocomplete_idx = 0;
                return;
            }
            _ => {}
        }
    }

    match (key, mods) {
        // Shift+Enter → insert newline
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            app.tui.focus = Focus::Input;
            app.tui.input_insert('\n');
        }
        // Submit
        (KeyCode::Enter, _) => {
            if let Some(input) = app.tui.commit_input() {
                submit_input(app, &input);
            }
        }

        // Ctrl+Backspace → delete word
        (KeyCode::Backspace, m) if m.contains(KeyModifiers::CONTROL) => {
            app.tui.focus = Focus::Input;
            app.tui.input_delete_word_back();
        }
        // Ctrl+Left/Right → word navigation
        (KeyCode::Left, m) if m.contains(KeyModifiers::CONTROL) => {
            app.tui.focus = Focus::Input;
            app.tui.input_move_word_left();
        }
        (KeyCode::Right, m) if m.contains(KeyModifiers::CONTROL) => {
            app.tui.focus = Focus::Input;
            app.tui.input_move_word_right();
        }
        // Ctrl+U → clear line
        (KeyCode::Char('u'), m) if m.contains(KeyModifiers::CONTROL) => {
            app.tui.focus = Focus::Input;
            app.tui.input_clear_line();
        }
        // Ctrl+K → kill to end
        (KeyCode::Char('k'), m) if m.contains(KeyModifiers::CONTROL) => {
            app.tui.focus = Focus::Input;
            app.tui.input_kill_to_end();
        }
        // Ctrl+A → home
        (KeyCode::Char('a'), m) if m.contains(KeyModifiers::CONTROL) => {
            app.tui.focus = Focus::Input;
            app.tui.input_home();
        }
        // Ctrl+E → end
        (KeyCode::Char('e'), m) if m.contains(KeyModifiers::CONTROL) => {
            app.tui.focus = Focus::Input;
            app.tui.input_end();
        }
        // Delete key → forward delete
        (KeyCode::Delete, _) => {
            app.tui.focus = Focus::Input;
            app.tui.input_delete_forward();
        }

        // Character input
        (KeyCode::Char(c), _) => {
            app.tui.focus = Focus::Input;
            app.tui.input_insert(c);
        }

        // Editing
        (KeyCode::Backspace, _) => {
            app.tui.focus = Focus::Input;
            app.tui.input_delete_back();
        }
        (KeyCode::Left, _) => {
            app.tui.focus = Focus::Input;
            app.tui.input_move_left();
        }
        (KeyCode::Right, _) => {
            app.tui.focus = Focus::Input;
            app.tui.input_move_right();
        }
        (KeyCode::Home, _) => match app.tui.focus {
            Focus::Input => app.tui.input_home(),
            Focus::Sidebar => {
                if !app.tui.workspace_files.is_empty() {
                    app.tui.file_list_state.select(Some(0));
                    refresh_tui_previews(app);
                }
            }
            Focus::ToolLog => { app.tui.tool_follow = false; app.tui.tool_scroll = 0; }
            Focus::Chat => { app.tui.chat_follow = false; app.tui.chat_scroll = 0; }
        },
        (KeyCode::End, _) => match app.tui.focus {
            Focus::Input => app.tui.input_end(),
            Focus::Sidebar => {
                let len = app.tui.workspace_files.len();
                if len > 0 {
                    app.tui.file_list_state.select(Some(len - 1));
                    refresh_tui_previews(app);
                }
            }
            Focus::ToolLog => app.tui.follow_tool_log(),
            Focus::Chat => app.tui.follow_chat(),
        },

        // History / scroll
        (KeyCode::Up, _) => match app.tui.focus {
            Focus::Input => app.tui.history_prev(),
            Focus::Sidebar => {
                let len = app.tui.workspace_files.len();
                if len > 0 {
                    let cur = app.tui.file_list_state.selected().unwrap_or(0);
                    app.tui.file_list_state.select(Some(cur.saturating_sub(1)));
                    refresh_tui_previews(app);
                }
            }
            Focus::ToolLog => app.tui.scroll_tool_by(-1),
            Focus::Chat => app.tui.scroll_chat_by(-1),
        },
        (KeyCode::Down, _) => match app.tui.focus {
            Focus::Input => app.tui.history_next(),
            Focus::Sidebar => {
                let len = app.tui.workspace_files.len();
                if len > 0 {
                    let cur = app.tui.file_list_state.selected().unwrap_or(0);
                    app.tui.file_list_state.select(Some((cur + 1).min(len - 1)));
                    refresh_tui_previews(app);
                }
            }
            Focus::ToolLog => app.tui.scroll_tool_by(1),
            Focus::Chat => app.tui.scroll_chat_by(1),
        },

        // Page scroll
        (KeyCode::PageUp, _) => match app.tui.focus {
            Focus::ToolLog => app.tui.scroll_tool_by(-10),
            _ => app.tui.scroll_chat_by(-10),
        },
        (KeyCode::PageDown, _) => match app.tui.focus {
            Focus::ToolLog => app.tui.scroll_tool_by(10),
            _ => app.tui.scroll_chat_by(10),
        },

        _ => {}
    }
}

fn handle_diff_keys(app: &mut App, key: KeyCode) {
    match key {
        KeyCode::Up => app.tui.diff_scroll = app.tui.diff_scroll.saturating_sub(1),
        KeyCode::Down => app.tui.diff_scroll += 1,
        KeyCode::PageUp => app.tui.diff_scroll = app.tui.diff_scroll.saturating_sub(20),
        KeyCode::PageDown => app.tui.diff_scroll += 20,
        KeyCode::Home => app.tui.diff_scroll = 0,
        KeyCode::End => {
            app.tui.diff_scroll = app.tui.diff_content.len().saturating_sub(1);
        }
        _ => {}
    }
}

fn handle_sessions_keys(app: &mut App, key: KeyCode) {
    let len = app.tui.sessions.len();
    if len == 0 {
        return;
    }
    let cur = app.tui.session_list_state.selected().unwrap_or(0);
    match key {
        KeyCode::Up => {
            app.tui.session_list_state.select(Some(cur.saturating_sub(1)));
            refresh_tui_previews(app);
        }
        KeyCode::Down => {
            app.tui.session_list_state.select(Some((cur + 1).min(len - 1)));
            refresh_tui_previews(app);
        }
        KeyCode::Enter => {
            // Replay selected session — push events as chat messages
            if let Some(idx) = app.tui.session_list_state.selected() {
                if let Some(s) = app.tui.sessions.get(idx) {
                    let sid = s.id.clone();
                    let _ = app.transcript_control.send(ControlPlaneCommand::Reset);
                    app.tui.messages.clear();
                    app.tui.tool_log.clear();
                    dispatch_control_plane(
                        app,
                        ControlPlaneCommand::SystemMessage {
                            content: format!("Replaying session: {}", sid),
                        },
                    );
                    for ev in app.session_store.replay(&sid) {
                        replay_session_event(app, ev);
                    }
                    app.tui.active_tab = ActiveTab::Chat;
                }
            }
        }
        _ => {}
    }
}

fn handle_action_queue_keys(app: &mut App, key: KeyCode) {
    let len = app.tui.action_queue.len();
    if len == 0 {
        return;
    }
    match key {
        KeyCode::Up => {
            app.tui.action_queue_selected = app.tui.action_queue_selected.saturating_sub(1);
            app.tui.action_preview_scroll = 0;
        }
        KeyCode::Down => {
            app.tui.action_queue_selected = (app.tui.action_queue_selected + 1).min(len - 1);
            app.tui.action_preview_scroll = 0;
        }
        KeyCode::PageUp => {
            app.tui.action_preview_scroll = app.tui.action_preview_scroll.saturating_sub(20);
        }
        KeyCode::PageDown => {
            app.tui.action_preview_scroll += 20;
        }
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let selected = app.tui.action_queue_selected.min(app.pending_permission_requests.len().saturating_sub(1));
            resolve_permission_request(app, selected, PermissionDecision::AllowOnce);
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            let selected = app.tui.action_queue_selected.min(app.pending_permission_requests.len().saturating_sub(1));
            resolve_permission_request(app, selected, PermissionDecision::AllowRemembered);
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            let selected = app.tui.action_queue_selected.min(app.pending_permission_requests.len().saturating_sub(1));
            resolve_permission_request(app, selected, PermissionDecision::Deny);
        }
        _ => {}
    }
}

fn handle_mouse(app: &mut App, m: crossterm::event::MouseEvent) {
    match m.kind {
        MouseEventKind::ScrollUp => match app.tui.active_tab {
            ActiveTab::Chat => {
                if app.tui.focus == Focus::ToolLog {
                    app.tui.scroll_tool_by(-3);
                } else {
                    app.tui.scroll_chat_by(-3);
                }
            }
            ActiveTab::Diff => {
                app.tui.diff_scroll = app.tui.diff_scroll.saturating_sub(3);
            }
            ActiveTab::ActionQueue => {
                app.tui.action_preview_scroll = app.tui.action_preview_scroll.saturating_sub(3);
            }
            _ => {}
        },
        MouseEventKind::ScrollDown => match app.tui.active_tab {
            ActiveTab::Chat => {
                if app.tui.focus == Focus::ToolLog {
                    app.tui.scroll_tool_by(3);
                } else {
                    app.tui.scroll_chat_by(3);
                }
            }
            ActiveTab::Diff => app.tui.diff_scroll += 3,
            ActiveTab::ActionQueue => app.tui.action_preview_scroll += 3,
            _ => {}
        },
        _ => {}
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Command dispatch
// ─────────────────────────────────────────────────────────────────────────────
fn submit_input(app: &mut App, input: &str) {
    app.tui.follow_chat();
    dispatch_control_plane(
        app,
        ControlPlaneCommand::UserMessage {
            content: input.to_string(),
        },
    );
    app.streaming_assistant_raw.clear();
    let _ = app.session_store.append(&app.session_id, &SessionEvent::user(input));
    refresh_tui_metadata(app);

    if input.starts_with("/index") {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let path = if parts.len() > 1 { parts[1] } else { "." };
        app.tui.is_indexing = true;
        app.tui.barq_context.push(format!("Indexing: {}", path));
        match app.orchestrator.barq.index_repo(path) {
            Ok(_) => {
                app.tui.barq_context.push("Index complete.".to_string());
                app.tui.set_status("Index complete", false);
            }
            Err(e) => {
                app.tui.barq_context.push(format!("Error: {}", e));
                app.tui.set_status(format!("Index error: {}", e), true);
            }
        }
        app.tui.is_indexing = false;
    } else if input == "/config" {
        if let Ok(cfg_str) = toml::to_string_pretty(&app.config) {
            app.tui.add_message(ChatMessage::system(format!("Config:\n{}", cfg_str)));
        }
    } else if input == "/clear" {
        app.tui.messages.clear();
        app.tui.tool_log.clear();
        app.orchestrator.conversation.clear();
        app.streaming_assistant_raw.clear();
        let _ = app.transcript_control.send(ControlPlaneCommand::Reset);
        app.tui.add_message(ChatMessage::system("Session cleared."));
    } else if input == "/help" {
        app.tui.add_message(ChatMessage::system(HELP_TEXT));
    } else if input == "/status" {
        let status = format!(
            "Model: {}\nTokens: {}/{} ({:.0}%)\nSession: {}\nMessages: {}\nTool log: {} entries",
            app.config.active_model_label(),
            app.tui.token_count, app.tui.token_limit,
            if app.tui.token_limit > 0 { app.tui.token_count as f64 / app.tui.token_limit as f64 * 100.0 } else { 0.0 },
            app.session_id,
            app.tui.messages.len(),
            app.tui.tool_log.len(),
        );
        app.tui.add_message(ChatMessage::system(status));
    } else if input == "/doctor" {
        app.tui.add_message(ChatMessage::system(format!(
            "Checking Ollama at {}...", app.config.ollama_base_url
        )));
    } else if input.starts_with("/goal ") {
        let goal_text = input["/goal ".len()..].to_string();
        app.tui.is_thinking = true;
        app.tui.add_message(ChatMessage::system(format!(
            "Starting multi-agent goal: {}",
            goal_text
        )));
        let coordinator = Arc::clone(&app.coordinator);
        let (tx, rx) = mpsc::channel(100);
        tokio::spawn(async move {
            let _ = tx
                .send(OrchestratorEvent::Token("Coordinator analyzing…".to_string()))
                .await;
            match coordinator.execute_goal(&goal_text).await {
                Ok(_) => {
                    let _ = tx
                        .send(OrchestratorEvent::Done("Goal completed.".to_string()))
                        .await;
                }
                Err(e) => {
                    let _ = tx
                        .send(OrchestratorEvent::Error(format!("Goal failed: {}", e)))
                        .await;
                }
            }
        });
        app.event_rx = Some(rx);
    } else if input == "/diff" {
        if app.tui.diff_content.is_empty() {
            app.tui.add_message(ChatMessage::system(
                "No diff captured yet. Run a file mutation first."
            ));
        } else {
            app.tui.open_diff();
        }
    } else if input == "/sessions" {
        app.tui.active_tab = ActiveTab::Sessions;
    } else if let Some(slash_cmd) = commands::parse(input) {
        // Route all /commands through the commands module
        let result = commands::execute(
            &slash_cmd,
            &app.config.workspace_root.clone(),
            &app.session_id.clone(),
            &app.session_store,
            &app.cost,
            &app.config.active_model_label(),
        );
        match result {
            commands::CommandResult::Message(msg) => {
                app.tui.add_message(ChatMessage::system(msg));
            }
            commands::CommandResult::SwitchModel(name) => {
                app.config.ollama_model = name.clone();
                app.tui.add_message(ChatMessage::system(format!("Switched model to: {} (Ollama)", name)));
            }
            commands::CommandResult::Compaction => {
                crate::context::auto_compact(&mut app.orchestrator.conversation, 6);
                app.tui.add_message(ChatMessage::system(
                    "Conversation compacted — older messages summarised to save context."
                ));
            }
        }
    } else {
        // Regular AI prompt — track cost
        let prompt_tokens = CostTracker::estimate_tokens(input);
        app.cost.record_turn(prompt_tokens, 0, 0);
        app.tui.is_thinking = true;
        let rx = app.orchestrator.run(input);
        app.event_rx = Some(rx);
    }
}

const HELP_TEXT: &str = "\
## Commands

  /help               Show this help message
  /clear              Clear chat and conversation history
  /config             Display current runtime configuration
  /status             Show token usage, session stats
  /doctor             Check Ollama connectivity
  /index [path]       Index codebase into BarqDB for semantic search
  /goal <text>        Run multi-agent goal (Planner → Coder → Tester → Reviewer)
  /diff               Show latest diff in the Diff tab
  /sessions           Browse saved session archive
  /memory [show]      Show project memory (.barqcoder.md)
  /memory add <note>  Add a note to project memory
  /model <name>       Switch LLM model mid-session
  /compact            Compact conversation to save context window

## Keyboard Shortcuts

  **Input**
  Enter               Send message
  Shift+Enter         Insert newline (multi-line input)
  Ctrl+U              Clear entire input
  Ctrl+K              Delete from cursor to end of line
  Ctrl+A / Ctrl+E     Move to start / end of input
  Ctrl+Left/Right     Move by word
  Ctrl+Backspace      Delete word backward

  **Navigation**
  Tab / Shift+Tab     Switch tabs
  F1                  Cycle focus (Input → Chat → ToolLog → Sidebar)
  Alt+S               Toggle sidebar
  ↑/↓                 History (Input) / Scroll (Chat/ToolLog)
  PageUp/Down         Fast scroll
  Home/End            Jump to top/bottom or follow live

  **Approval Gate**
  Y                   Approve action once
  A                   Trust tool for this session
  N                   Deny action
  Esc                 Quit (or deny all pending)";

// ─────────────────────────────────────────────────────────────────────────────
// Quit flag on TuiState
// ─────────────────────────────────────────────────────────────────────────────
impl TuiState {
    pub fn should_quit(&self) -> bool {
        self._quit
    }
    pub fn mark_quit(&mut self) {
        self._quit = true;
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Health server
// ─────────────────────────────────────────────────────────────────────────────
async fn start_health_server() {
    if let Ok(listener) = TcpListener::bind("0.0.0.0:8080").await {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                if stream.read(&mut buf).await.is_ok() {
                    let req = String::from_utf8_lossy(&buf);
                    let response = if req.starts_with("GET /health") {
                        "HTTP/1.1 200 OK\r\n\r\nOK"
                    } else if req.starts_with("GET /metrics") {
                        "HTTP/1.1 200 OK\r\n\r\nbarqcoder_requests_total 0\n"
                    } else {
                        "HTTP/1.1 404 Not Found\r\n\r\n"
                    };
                    let _ = stream.write_all(response.as_bytes()).await;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        apply_control_plane_response, build_pending_action, build_permission_preview,
        normalized_to_chat_message, sanitize_assistant_text, summarize_tool_call,
        truncate_multiline, PERMISSION_PREVIEW_MAX_LINES,
    };
    use crate::control_plane::{ControlPlaneResponse, NormalizedMessage, NormalizedMessageKind};
    use crate::tui::{ActionKind, TuiState};
    use serde_json::json;

    #[test]
    fn permission_preview_formats_multiline_edits() {
        let preview = build_permission_preview("src/main.rs", Some("old\nline"), Some("new\nline"));

        assert!(preview.contains("--- a/src/main.rs"));
        assert!(preview.contains("+++ b/src/main.rs"));
        assert!(preview.contains("-old"));
        assert!(preview.contains("+new"));
    }

    #[test]
    fn shell_exec_permissions_map_to_shell_actions() {
        let action = build_pending_action(
            "shell_exec",
            &json!({ "command": "cargo check" }),
            "Run verification",
        );

        match action.kind {
            ActionKind::ShellCommand { command, reason } => {
                assert_eq!(command, "cargo check");
                assert_eq!(reason, "Run verification");
            }
            other => panic!("unexpected action kind: {:?}", other),
        }
    }

    #[test]
    fn tool_call_summary_avoids_dumping_large_file_content() {
        let large_content = "fn main() {}\n".repeat(200);
        let summary = summarize_tool_call(
            "create_file",
            &json!({
                "path": "src/generated.rs",
                "content": large_content,
            }),
        );

        assert!(summary.contains("Calling create_file for src/generated.rs"));
        assert!(summary.contains("content: 200 lines"));
        assert!(!summary.contains("fn main() {}"));
    }

    #[test]
    fn pending_action_preview_is_truncated_for_large_writes() {
        let content = (0..(PERMISSION_PREVIEW_MAX_LINES + 50))
            .map(|idx| format!("line {}", idx))
            .collect::<Vec<_>>()
            .join("\n");
        let action = build_pending_action(
            "create_file",
            &json!({
                "path": "src/generated.rs",
                "content": content,
            }),
            "Create a generated file",
        );

        match action.kind {
            ActionKind::WriteFile { patch, .. } => {
                assert!(patch.contains("… truncated"));
                assert!(!patch.contains(&format!("line {}", PERMISSION_PREVIEW_MAX_LINES + 40)));
            }
            other => panic!("unexpected action kind: {:?}", other),
        }
    }

    #[test]
    fn truncate_multiline_adds_a_truncation_notice() {
        let text = (0..20)
            .map(|idx| format!("entry {}", idx))
            .collect::<Vec<_>>()
            .join("\n");
        let truncated = truncate_multiline(&text, 5, 128);

        assert!(truncated.contains("entry 0"));
        assert!(truncated.contains("… truncated"));
        assert!(!truncated.contains("entry 19"));
    }

    #[test]
    fn sanitize_assistant_text_extracts_final_answer_from_json_wrapper() {
        let rendered = sanitize_assistant_text(
            r#"{"reasoning":"Need to inspect the project first.","tool_calls":[],"final_answer":"I found the failing test and fixed the parser."}"#,
        );

        assert_eq!(rendered, "I found the failing test and fixed the parser.");
    }

    #[test]
    fn sanitize_assistant_text_extracts_partial_streamed_final_answer() {
        let rendered = sanitize_assistant_text(
            r#"{"reasoning":"Inspecting files","final_answer":"I fixed the conversation renderer"#,
        );

        assert_eq!(rendered, "I fixed the conversation renderer");
    }

    #[test]
    fn sanitize_assistant_text_leaves_plain_text_unchanged() {
        let rendered =
            sanitize_assistant_text("I updated the TUI to use a right-side operations rail.");

        assert_eq!(rendered, "I updated the TUI to use a right-side operations rail.");
    }

    #[test]
    fn normalized_messages_map_to_titled_chat_cards() {
        let message = normalized_to_chat_message(NormalizedMessage {
            kind: NormalizedMessageKind::ToolCall,
            title: "Read File".to_string(),
            content: "Read src/main.rs".to_string(),
            status: Some("pending".to_string()),
        });

        assert_eq!(message.title, "Read File");
        assert_eq!(message.content, "Read src/main.rs");
        assert_eq!(message.status.as_deref(), Some("pending"));
    }

    #[test]
    fn control_plane_replace_updates_last_transcript_message() {
        let mut tui = TuiState::new(4096, "test-model".to_string(), "session".to_string());
        tui.messages.clear();

        apply_control_plane_response(
            &mut tui,
            ControlPlaneResponse::Append {
                message: NormalizedMessage {
                    kind: NormalizedMessageKind::Agent,
                    title: "Response".to_string(),
                    content: "Draft".to_string(),
                    status: Some("streaming".to_string()),
                },
                tool_log_entry: None,
            },
        );
        apply_control_plane_response(
            &mut tui,
            ControlPlaneResponse::ReplaceLast {
                message: NormalizedMessage {
                    kind: NormalizedMessageKind::Agent,
                    title: "Response".to_string(),
                    content: "Draft complete".to_string(),
                    status: Some("final".to_string()),
                },
                tool_log_entry: None,
            },
        );

        assert_eq!(tui.messages.len(), 1);
        assert_eq!(tui.messages[0].content, "Draft complete");
        assert_eq!(tui.messages[0].status.as_deref(), Some("final"));
    }
}
