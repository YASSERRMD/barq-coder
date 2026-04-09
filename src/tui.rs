use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, BorderType, Borders, Clear, List, ListItem, ListState, Padding, Paragraph,
        Scrollbar, ScrollbarOrientation, ScrollbarState, Tabs, Wrap,
    },
    Frame,
};

use crate::markdown;

// ─────────────────────────────────────────────
// Palette
// ─────────────────────────────────────────────
pub struct Palette;
impl Palette {
    pub const BG: Color = Color::Rgb(16, 14, 12);
    pub const SURFACE: Color = Color::Rgb(24, 21, 18);
    pub const SURFACE2: Color = Color::Rgb(32, 28, 24);
    pub const BORDER: Color = Color::Rgb(78, 70, 62);
    pub const BORDER_ACTIVE: Color = Color::Rgb(220, 165, 86);
    pub const ACCENT: Color = Color::Rgb(220, 165, 86);
    pub const ACCENT2: Color = Color::Rgb(124, 188, 165);
    pub const TEXT: Color = Color::Rgb(234, 228, 219);
    pub const TEXT_DIM: Color = Color::Rgb(150, 138, 123);
    pub const TEXT_BRIGHT: Color = Color::Rgb(250, 246, 240);
    pub const USER_MSG: Color = Color::Rgb(135, 204, 196);
    pub const AGENT_MSG: Color = Color::Rgb(244, 216, 138);
    pub const ERROR_MSG: Color = Color::Rgb(239, 121, 121);
    pub const WARN_MSG: Color = Color::Rgb(245, 190, 93);
    pub const TOOL_CALL: Color = Color::Rgb(140, 178, 230);
    pub const TOOL_RESULT: Color = Color::Rgb(232, 155, 100);
    pub const DIFF_ADD: Color = Color::Rgb(126, 206, 115);
    pub const DIFF_DEL: Color = Color::Rgb(239, 121, 121);
    pub const DIFF_HUNK: Color = Color::Rgb(124, 181, 235);
    pub const STATUS_OK: Color = Color::Rgb(120, 193, 137);
    pub const STATUS_ERR: Color = Color::Rgb(239, 121, 121);
    pub const STATUS_WARN: Color = Color::Rgb(245, 190, 93);
    pub const KEY_HINT: Color = Color::Rgb(147, 132, 115);
    pub const KEY_LABEL: Color = Color::Rgb(197, 167, 128);
}

// ─────────────────────────────────────────────
// Spinner
// ─────────────────────────────────────────────
const SPINNER_FRAMES: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

const BOUNCE_FRAMES: &[&str] = &[
    "●∙∙∙∙", "∙●∙∙∙", "∙∙●∙∙", "∙∙∙●∙", "∙∙∙∙●", "∙∙∙●∙", "∙∙●∙∙", "∙●∙∙∙",
];

pub fn bounce_frame(tick: usize) -> &'static str {
    BOUNCE_FRAMES[tick % BOUNCE_FRAMES.len()]
}

// ─────────────────────────────────────────────
// Tab enum
// ─────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActiveTab {
    Chat = 0,
    Diff = 1,
    Sessions = 2,
    ActionQueue = 3,
}

impl ActiveTab {
    pub fn next(self) -> Self {
        match self {
            Self::Chat => Self::Diff,
            Self::Diff => Self::Sessions,
            Self::Sessions => Self::ActionQueue,
            Self::ActionQueue => Self::Chat,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::Chat => Self::ActionQueue,
            Self::Diff => Self::Chat,
            Self::Sessions => Self::Diff,
            Self::ActionQueue => Self::Sessions,
        }
    }
}

// ─────────────────────────────────────────────
// Focus enum
// ─────────────────────────────────────────────
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Input,
    Sidebar,
    Chat,
    ToolLog,
}

// ─────────────────────────────────────────────
// Chat message types
// ─────────────────────────────────────────────
#[derive(Clone, Debug)]
pub enum MessageKind {
    User,
    Agent,
    ToolCall,
    ToolResult,
    System,
    Error,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub kind: MessageKind,
    pub content: String,
}

impl ChatMessage {
    pub fn user(s: impl Into<String>) -> Self {
        Self { kind: MessageKind::User, content: s.into() }
    }
    pub fn agent(s: impl Into<String>) -> Self {
        Self { kind: MessageKind::Agent, content: s.into() }
    }
    pub fn tool_call(s: impl Into<String>) -> Self {
        Self { kind: MessageKind::ToolCall, content: s.into() }
    }
    pub fn tool_result(s: impl Into<String>) -> Self {
        Self { kind: MessageKind::ToolResult, content: s.into() }
    }
    pub fn system(s: impl Into<String>) -> Self {
        Self { kind: MessageKind::System, content: s.into() }
    }
    pub fn error(s: impl Into<String>) -> Self {
        Self { kind: MessageKind::Error, content: s.into() }
    }
}

// ─────────────────────────────────────────────
// Pending agent action — queued for user review
// ─────────────────────────────────────────────
#[derive(Clone, Debug)]
pub enum ActionKind {
    /// Write or overwrite a file with the given content.
    WriteFile {
        path: String,
        patch: String,
    },
    /// Run a shell command (requires explicit user approval).
    ShellCommand { command: String, reason: String },
    /// Apply a verified patch from the VerificationGate.
    ApplyVerifiedPatch { patch: String, step_id: String },
}

#[derive(Clone, Debug)]
pub struct PendingAction {
    pub kind: ActionKind,
    pub agent: String,
    pub approved: Option<bool>,
}

impl PendingAction {
    pub fn write_file(path: impl Into<String>, patch: impl Into<String>, agent: impl Into<String>) -> Self {
        Self { kind: ActionKind::WriteFile { path: path.into(), patch: patch.into() }, agent: agent.into(), approved: None }
    }

    pub fn shell_cmd(command: impl Into<String>, reason: impl Into<String>, agent: impl Into<String>) -> Self {
        Self { kind: ActionKind::ShellCommand { command: command.into(), reason: reason.into() }, agent: agent.into(), approved: None }
    }

    pub fn verified_patch(patch: impl Into<String>, step_id: impl Into<String>, agent: impl Into<String>) -> Self {
        Self { kind: ActionKind::ApplyVerifiedPatch { patch: patch.into(), step_id: step_id.into() }, agent: agent.into(), approved: None }
    }

    pub fn label(&self) -> String {
        match &self.kind {
            ActionKind::WriteFile { path, .. } => format!("Write: {}", path),
            ActionKind::ShellCommand { command, .. } => format!("Shell: {}", &command[..command.len().min(60)]),
            ActionKind::ApplyVerifiedPatch { step_id, .. } => format!("Patch (verified): {}", step_id),
        }
    }

    pub fn preview(&self) -> &str {
        match &self.kind {
            ActionKind::WriteFile { patch, .. } => patch,
            ActionKind::ShellCommand { reason, .. } => reason,
            ActionKind::ApplyVerifiedPatch { patch, .. } => patch,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PermissionPrompt {
    pub title: String,
    pub reason: String,
    pub hint: String,
    pub queue_len: usize,
}

// ─────────────────────────────────────────────
// Session entry for the Sessions tab
// ─────────────────────────────────────────────
#[derive(Clone, Debug)]
pub struct SessionEntry {
    pub id: String,
    pub created_at: u64,
    pub event_count: usize,
    pub workspace: String,
}

// ─────────────────────────────────────────────
// Full TUI state
// ─────────────────────────────────────────────
pub struct TuiState {
    // Tabs
    pub active_tab: ActiveTab,
    pub focus: Focus,

    // Chat
    pub messages: Vec<ChatMessage>,
    pub chat_scroll: usize,

    // Input
    pub input: String,
    pub input_cursor: usize,
    pub input_history: Vec<String>,
    pub input_history_idx: usize,
    pub autocomplete_idx: usize,

    // Sidebar / file tree
    pub workspace_files: Vec<String>,
    pub file_list_state: ListState,
    pub file_preview_title: String,
    pub file_preview: Vec<String>,
    pub sidebar_visible: bool,

    // Tool log
    pub tool_log: Vec<String>,
    pub tool_scroll: usize,
    pub current_tool: Option<String>,

    // Barq context
    pub barq_context: Vec<String>,

    // Diff view
    pub diff_content: Vec<String>,
    pub diff_scroll: usize,
    pub diff_title: String,

    // Action Sandbox (Phase 3)
    pub action_queue: Vec<PendingAction>,
    pub action_queue_selected: usize,
    pub action_preview_scroll: usize,
    pub permission_prompt: Option<PermissionPrompt>,

    // Sessions
    pub sessions: Vec<SessionEntry>,
    pub session_list_state: ListState,
    pub session_preview_title: String,
    pub session_preview: Vec<String>,

    // Follow mode
    pub chat_follow: bool,
    pub tool_follow: bool,

    // Status
    pub is_thinking: bool,
    pub is_indexing: bool,
    pub tick: usize,
    pub token_count: u32,
    pub token_limit: u32,
    pub current_model: String,
    pub session_id: String,
    pub status_message: Option<String>,
    pub status_is_error: bool,
    pub _quit: bool,
}

impl TuiState {
    pub fn new(token_limit: u32, model: String, session_id: String) -> Self {
        Self {
            active_tab: ActiveTab::Chat,
            focus: Focus::Input,
            messages: vec![ChatMessage::system(
                "BarqCoder console online. Ask for a change, a review, or a command run.",
            )],
            chat_scroll: 0,
            input: String::new(),
            input_cursor: 0,
            input_history: Vec::new(),
            input_history_idx: 0,
            autocomplete_idx: 0,
            workspace_files: Vec::new(),
            file_list_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            file_preview_title: "File Preview".to_string(),
            file_preview: Vec::new(),
            sidebar_visible: true,
            tool_log: Vec::new(),
            tool_scroll: usize::MAX,
            current_tool: None,
            barq_context: Vec::new(),
            diff_content: Vec::new(),
            diff_scroll: 0,
            diff_title: "Latest Diff".to_string(),
            action_queue: Vec::new(),
            action_queue_selected: 0,
            action_preview_scroll: 0,
            permission_prompt: None,
            sessions: Vec::new(),
            session_list_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            session_preview_title: "Session Preview".to_string(),
            session_preview: Vec::new(),
            chat_follow: true,
            tool_follow: true,
            is_thinking: false,
            is_indexing: false,
            tick: 0,
            token_count: 0,
            token_limit,
            current_model: model,
            session_id,
            status_message: None,
            status_is_error: false,
            _quit: false,
        }
    }

    pub fn add_message(&mut self, msg: ChatMessage) {
        self.messages.push(msg);
        // auto-scroll to bottom
        if self.chat_follow {
            self.chat_scroll = usize::MAX;
        }
    }

    pub fn append_agent_token(&mut self, token: &str) {
        match self.messages.last_mut() {
            Some(m) if matches!(m.kind, MessageKind::Agent) => {
                m.content.push_str(token);
            }
            _ => {
                self.messages.push(ChatMessage::agent(token));
            }
        }
        if self.chat_follow {
            self.chat_scroll = usize::MAX;
        }
    }

    pub fn update_diff(&mut self, title: impl Into<String>, patch: &str) {
        self.diff_title = title.into();
        self.diff_content = patch.lines().map(|l| l.to_string()).collect();
        self.diff_scroll = 0;
    }

    pub fn set_diff(&mut self, patch: &str) {
        self.update_diff("Latest Diff", patch);
        self.active_tab = ActiveTab::Diff;
    }

    pub fn open_diff(&mut self) {
        self.active_tab = ActiveTab::Diff;
    }

    pub fn set_status(&mut self, msg: impl Into<String>, is_error: bool) {
        self.status_message = Some(msg.into());
        self.status_is_error = is_error;
    }

    pub fn clear_status(&mut self) {
        self.status_message = None;
    }

    pub fn input_insert(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
        self.autocomplete_idx = 0;
    }

    pub fn input_insert_str(&mut self, text: &str) {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        self.input.insert_str(self.input_cursor, &normalized);
        self.input_cursor += normalized.len();
        self.autocomplete_idx = 0;
    }

    pub fn input_delete_word_back(&mut self) {
        if self.input_cursor == 0 { return; }
        let before = &self.input[..self.input_cursor];
        let trimmed = before.trim_end();
        let word_start = trimmed.rfind(|c: char| c.is_whitespace() || c == '/' || c == '.')
            .map(|i| i + 1).unwrap_or(0);
        self.input.drain(word_start..self.input_cursor);
        self.input_cursor = word_start;
        self.autocomplete_idx = 0;
    }

    pub fn input_move_word_left(&mut self) {
        if self.input_cursor == 0 { return; }
        let before = &self.input[..self.input_cursor];
        let trimmed = before.trim_end();
        self.input_cursor = trimmed.rfind(|c: char| c.is_whitespace() || c == '/' || c == '.')
            .map(|i| i + 1).unwrap_or(0);
    }

    pub fn input_move_word_right(&mut self) {
        if self.input_cursor >= self.input.len() { return; }
        let after = &self.input[self.input_cursor..];
        let skip_word = after.find(|c: char| c.is_whitespace() || c == '/' || c == '.')
            .unwrap_or(after.len());
        let rest = &after[skip_word..];
        let skip_ws = rest.find(|c: char| !c.is_whitespace()).unwrap_or(rest.len());
        self.input_cursor += skip_word + skip_ws;
    }

    pub fn input_delete_forward(&mut self) {
        if self.input_cursor < self.input.len() {
            let ch_len = self.input[self.input_cursor..].chars().next()
                .map(|c| c.len_utf8()).unwrap_or(0);
            self.input.drain(self.input_cursor..self.input_cursor + ch_len);
        }
    }

    pub fn input_clear_line(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
        self.autocomplete_idx = 0;
    }

    pub fn input_kill_to_end(&mut self) {
        self.input.truncate(self.input_cursor);
    }

    pub fn add_tool_log_entry(&mut self, entry: impl Into<String>) {
        self.tool_log.push(entry.into());
        if self.tool_follow {
            self.tool_scroll = usize::MAX;
        }
    }

    pub fn follow_chat(&mut self) {
        self.chat_follow = true;
        self.chat_scroll = usize::MAX;
    }

    pub fn scroll_chat_by(&mut self, delta: isize) {
        self.chat_follow = false;
        if delta < 0 {
            self.chat_scroll = self.chat_scroll.saturating_sub(delta.unsigned_abs());
        } else {
            self.chat_scroll = self.chat_scroll.saturating_add(delta as usize);
        }
    }

    pub fn follow_tool_log(&mut self) {
        self.tool_follow = true;
        self.tool_scroll = usize::MAX;
    }

    pub fn scroll_tool_by(&mut self, delta: isize) {
        self.tool_follow = false;
        if delta < 0 {
            self.tool_scroll = self.tool_scroll.saturating_sub(delta.unsigned_abs());
        } else {
            self.tool_scroll = self.tool_scroll.saturating_add(delta as usize);
        }
    }

    pub fn input_delete_back(&mut self) {
        if self.input_cursor > 0 {
            let c_start = self
                .input
                .char_indices()
                .rev()
                .find(|(i, _)| *i < self.input_cursor)
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.drain(c_start..self.input_cursor);
            self.input_cursor = c_start;
        }
        self.autocomplete_idx = 0;
    }

    pub fn input_move_left(&mut self) {
        if self.input_cursor > 0 {
            self.input_cursor = self
                .input
                .char_indices()
                .rev()
                .find(|(i, _)| *i < self.input_cursor)
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn input_move_right(&mut self) {
        if self.input_cursor < self.input.len() {
            if let Some((i, c)) = self.input[self.input_cursor..].char_indices().next() {
                self.input_cursor += i + c.len_utf8();
            }
        }
    }

    pub fn input_home(&mut self) {
        self.input_cursor = 0;
    }

    pub fn input_end(&mut self) {
        self.input_cursor = self.input.len();
    }

    pub fn history_prev(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        if self.input_history_idx > 0 {
            self.input_history_idx -= 1;
        }
        let entry = self.input_history[self.input_history_idx].clone();
        self.input = entry;
        self.input_cursor = self.input.len();
    }

    pub fn history_next(&mut self) {
        if self.input_history.is_empty() {
            return;
        }
        if self.input_history_idx + 1 < self.input_history.len() {
            self.input_history_idx += 1;
            let entry = self.input_history[self.input_history_idx].clone();
            self.input = entry;
        } else {
            self.input_history_idx = self.input_history.len();
            self.input.clear();
        }
        self.input_cursor = self.input.len();
    }

    pub fn commit_input(&mut self) -> Option<String> {
        let trimmed = self.input.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        self.input_history.push(trimmed.clone());
        self.input_history_idx = self.input_history.len();
        self.input.clear();
        self.input_cursor = 0;
        self.autocomplete_idx = 0;
        Some(trimmed)
    }

    // ── Autocomplete helpers ──

    /// The canonical list of slash commands for autocomplete.
    pub fn slash_commands() -> &'static [(&'static str, &'static str)] {
        &[
            ("/help",     "Show help & keybindings"),
            ("/clear",    "Clear conversation history"),
            ("/config",   "Display runtime config"),
            ("/goal",     "Run multi-agent goal plan"),
            ("/diff",     "Show latest diff patch"),
            ("/sessions", "Browse session archive"),
            ("/memory",   "View/add project memory"),
            ("/doctor",   "Check Ollama connectivity"),
            ("/index",    "Index workspace into BarqDB"),
            ("/status",   "Show token usage & budget"),
        ]
    }

    /// Returns the filtered list of commands matching the current input.
    pub fn get_autocomplete_matches(&self) -> Vec<(&'static str, &'static str)> {
        if !self.input.starts_with('/') || self.input.is_empty() {
            return Vec::new();
        }
        Self::slash_commands()
            .iter()
            .filter(|(cmd, _)| cmd.starts_with(&self.input.as_str()))
            .copied()
            .collect()
    }

    /// Whether the autocomplete popup is currently visible.
    pub fn is_autocomplete_active(&self) -> bool {
        self.input.starts_with('/') && !self.get_autocomplete_matches().is_empty()
    }

    /// Move selection up in the autocomplete list.
    pub fn autocomplete_up(&mut self) {
        if self.autocomplete_idx > 0 {
            self.autocomplete_idx -= 1;
        }
    }

    /// Move selection down in the autocomplete list.
    pub fn autocomplete_down(&mut self) {
        let count = self.get_autocomplete_matches().len();
        if count > 0 && self.autocomplete_idx + 1 < count {
            self.autocomplete_idx += 1;
        }
    }

    /// Accept the currently selected autocomplete item, replacing input.
    pub fn autocomplete_accept(&mut self) {
        let matches = self.get_autocomplete_matches();
        if let Some((cmd, _)) = matches.get(self.autocomplete_idx) {
            self.input = cmd.to_string();
            self.input_cursor = self.input.len();
            self.autocomplete_idx = 0;
        }
    }
}

// ─────────────────────────────────────────────
// Main draw function
// ─────────────────────────────────────────────
pub fn draw(f: &mut Frame, state: &mut TuiState) {
    f.render_widget(Block::default().style(Style::default().bg(Palette::BG)), f.area());

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0), Constraint::Length(2)])
        .split(f.area());

    draw_header(f, outer[0], state);
    draw_body(f, outer[1], state);
    draw_keys(f, outer[2], state);

    if let Some(prompt) = &state.permission_prompt {
        draw_permission_prompt(f, prompt);
    }
}

fn draw_header(f: &mut Frame, area: Rect, state: &TuiState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1), Constraint::Length(3)])
        .split(area);

    let headline = Line::from(vec![
        Span::styled(
            "BARQCODER // CONTROL ROOM",
            Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("session {}", trim_inline(&state.session_id, 24)),
            Style::default().fg(Palette::TEXT_DIM),
        ),
    ]);
    f.render_widget(Paragraph::new(headline), rows[0]);

    let mut chips = vec![
        chip(
            format!("focus {}", focus_label(state.focus)),
            Palette::BG,
            Palette::ACCENT,
        ),
        Span::raw(" "),
        chip(status_label(state), status_color(state), Palette::SURFACE2),
        Span::raw(" "),
        chip(
            format!("tokens {}/{}", state.token_count, state.token_limit),
            Palette::TEXT,
            Palette::SURFACE2,
        ),
        Span::raw(" "),
        chip(
            format!("approvals {}", state.action_queue.len()),
            if state.action_queue.is_empty() {
                Palette::TEXT
            } else {
                Palette::WARN_MSG
            },
            Palette::SURFACE2,
        ),
        Span::raw(" "),
        chip(
            format!("rail {}", if state.sidebar_visible { "open" } else { "hidden" }),
            Palette::TEXT,
            Palette::SURFACE2,
        ),
    ];

    if let Some(message) = state.status_message.as_deref().filter(|message| !message.is_empty()) {
        chips.push(Span::raw("  "));
        chips.push(Span::styled(
            trim_inline(message, 68),
            Style::default().fg(if state.status_is_error {
                Palette::STATUS_ERR
            } else {
                Palette::TEXT_DIM
            }),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(chips)), rows[1]);

    let tabs = Tabs::new(vec![
        Line::from(" Console "),
        Line::from(" Patch Deck "),
        Line::from(" Session Log "),
        Line::from(format!(" Approval Gate [{}] ", state.action_queue.len())),
    ])
    .select(state.active_tab as usize)
    .block(panel_block("Operations", false))
    .highlight_style(
        Style::default()
            .fg(Palette::TEXT_BRIGHT)
            .bg(Palette::SURFACE2)
            .add_modifier(Modifier::BOLD),
    )
    .divider(Span::styled(" ", Style::default().fg(Palette::TEXT_DIM)));
    f.render_widget(tabs, rows[2]);
}

fn draw_body(f: &mut Frame, area: Rect, state: &mut TuiState) {
    match state.active_tab {
        ActiveTab::Chat => draw_chat_tab(f, area, state),
        ActiveTab::Diff => draw_diff_tab(f, area, state),
        ActiveTab::Sessions => draw_sessions_tab(f, area, state),
        ActiveTab::ActionQueue => draw_action_queue_tab(f, area, state),
    }
}

fn draw_chat_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let show_sidebar = state.sidebar_visible && area.width >= 96;
    let rail_width = if show_sidebar {
        if area.width < 124 { 34 } else { 38 }
    } else {
        0
    };

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(rail_width)])
        .split(area);

    let input_height = (state.input.lines().count() as u16 + 4).clamp(6, 12);
    let main_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(12), Constraint::Length(input_height)])
        .split(columns[0]);

    draw_chat_area(f, main_rows[0], state);
    draw_input(f, main_rows[1], state);

    if show_sidebar {
        draw_sidebar(f, columns[1], state);
    }
}

fn draw_sidebar(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(9),
            Constraint::Length(10),
            Constraint::Length(9),
        ])
        .split(area);

    let mut radar_lines = vec![
        Line::from(vec![
            Span::styled("State", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(status_label(state), Style::default().fg(status_color(state)).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("Focus", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(focus_label(state.focus), Style::default().fg(Palette::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("Transcript", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(
                if state.chat_follow { "live follow" } else { "history review" },
                Style::default().fg(Palette::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Model", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(
                trim_inline(&state.current_model, 22),
                Style::default().fg(Palette::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Tool", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(
                trim_inline(state.current_tool.as_deref().unwrap_or("idle"), 22),
                Style::default().fg(Palette::TEXT),
            ),
        ]),
    ];

    if let Some(context) = state.barq_context.last() {
        radar_lines.push(Line::from(vec![
            Span::styled("Context", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(trim_inline(context, 22), Style::default().fg(Palette::TEXT_DIM)),
        ]));
    }

    let radar = Paragraph::new(radar_lines)
        .block(panel_block("Session Brief", false))
        .wrap(Wrap { trim: true });
    f.render_widget(radar, rows[0]);

    draw_tool_log(f, rows[1], state);
    draw_working_set(f, rows[2], state);
    draw_file_preview(f, rows[3], state);
}

fn draw_chat_area(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let sections = if !state.chat_follow && area.height >= 8 {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Min(0)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0)])
            .split(area)
    };

    let transcript_area = if sections.len() == 2 {
        if let Some(summary) = sticky_prompt_summary(&state.messages) {
            let ribbon = Paragraph::new(Line::from(vec![
                Span::styled(
                    " Pinned Request ",
                    Style::default().fg(Palette::BG).bg(Palette::ACCENT2).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    trim_inline(&summary, sections[0].width.saturating_sub(20) as usize),
                    Style::default().fg(Palette::TEXT),
                ),
            ]))
            .block(panel_block("History Anchor", false));
            f.render_widget(ribbon, sections[0]);
        }
        sections[1]
    } else {
        sections[0]
    };

    let content_width = transcript_area.width.saturating_sub(6) as usize;
    let mut lines = build_transcript_lines(&state.messages, content_width.max(18));
    if state.is_thinking {
        lines.push(Line::from(vec![
            Span::styled(" Drafting ", Style::default().fg(Palette::BG).bg(Palette::ACCENT).add_modifier(Modifier::BOLD)),
            Span::raw(" "),
            Span::styled(
                format!("{} drafting reply", spinner_frame(state.tick)),
                Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
            ),
        ]));
    }

    let scroll = render_lines_panel(
        f,
        transcript_area,
        lines,
        &mut state.chat_scroll,
        panel_block(
            if state.chat_follow {
                "Transcript"
            } else {
                "Transcript / Review"
            },
            state.focus == Focus::Chat,
        ),
    );
    if state.chat_follow {
        state.chat_scroll = scroll;
    }
}

fn draw_tool_log(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let mut lines = Vec::new();

    if let Some(tool) = &state.current_tool {
        lines.push(Line::from(vec![
            Span::styled("Active", Style::default().fg(Palette::WARN_MSG).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(trim_inline(tool, 28), Style::default().fg(Palette::TEXT)),
        ]));
        lines.push(Line::raw(""));
    }

    if state.tool_log.is_empty() {
        lines.push(Line::from(Span::styled(
            "No tool activity yet.",
            Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
        )));
    } else {
        for entry in &state.tool_log {
            for (idx, line) in wrap_plain_text(entry, area.width.saturating_sub(6) as usize)
                .into_iter()
                .enumerate()
            {
                let prefix = if idx == 0 { "• " } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Palette::KEY_LABEL)),
                    Span::styled(line, Style::default().fg(Palette::TEXT)),
                ]));
            }
            lines.push(Line::raw(""));
        }
    }

    render_lines_panel(
        f,
        area,
        lines,
        &mut state.tool_scroll,
        panel_block(
            if state.tool_follow {
                "Run Feed"
            } else {
                "Run Feed / History"
            },
            state.focus == Focus::ToolLog,
        ),
    );
}

fn draw_working_set(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let items: Vec<ListItem> = if state.workspace_files.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No indexed files available.",
            Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
        )))]
    } else {
        state.workspace_files.iter().map(|path| {
            ListItem::new(Line::from(vec![
                Span::styled("· ", Style::default().fg(Palette::ACCENT2)),
                Span::styled(trim_inline(path, area.width.saturating_sub(8) as usize), Style::default().fg(Palette::TEXT)),
            ]))
        }).collect()
    };

    let list = List::new(items)
        .block(panel_block("Workspace", state.focus == Focus::Sidebar))
        .highlight_style(
            Style::default()
                .bg(Palette::SURFACE2)
                .fg(Palette::TEXT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut state.file_list_state);
}

fn draw_file_preview(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let lines: Vec<Line> = if state.file_preview.is_empty() {
        vec![Line::from(Span::styled(
            "Select a file to inspect its preview.",
            Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        state
            .file_preview
            .iter()
            .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Palette::TEXT))))
            .collect()
    };

    let preview = Paragraph::new(lines)
        .block(panel_block(&state.file_preview_title, false))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, area);
}

fn draw_input(f: &mut Frame, area: Rect, state: &TuiState) {
    let title = if state.input.is_empty() {
        "Composer".to_string()
    } else {
        format!(
            "Composer  {} lines / {} chars",
            state.input.lines().count().max(1),
            state.input.chars().count()
        )
    };

    let content_width = area.width.saturating_sub(4) as usize;
    let content_height = area.height.saturating_sub(2) as usize;
    let input = Paragraph::new(Text::from(build_composer_lines(
        &state.input,
        state.input_cursor,
        content_width,
        content_height,
    )))
    .block(panel_block(&title, state.focus == Focus::Input))
    .wrap(Wrap { trim: false });
    f.render_widget(input, area);

    if state.focus == Focus::Input && state.is_autocomplete_active() {
        let matches = state.get_autocomplete_matches();
        let popup_height = (matches.len() as u16 + 2).min(8);
        let popup_width = area.width.min(44);
        let popup_area = Rect {
            x: area.x + 1,
            y: area.y.saturating_sub(popup_height),
            width: popup_width,
            height: popup_height,
        };

        let lines: Vec<Line> = matches
            .iter()
            .enumerate()
            .map(|(idx, (command, desc))| {
                let style = if idx == state.autocomplete_idx {
                    Style::default()
                        .fg(Palette::BG)
                        .bg(Palette::ACCENT)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Palette::TEXT)
                };

                Line::from(vec![
                    Span::styled(format!("{:<12}", command), style),
                    Span::styled(desc.to_string(), style),
                ])
            })
            .collect();

        let popup = Paragraph::new(lines)
            .block(panel_block("Slash Commands", true))
            .wrap(Wrap { trim: true });
        f.render_widget(Clear, popup_area);
        f.render_widget(popup, popup_area);
    }
}

fn draw_diff_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.diff_content.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::from(Span::styled(
                "No diff is loaded.",
                Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Run a file mutation from the console and the latest patch will appear here.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
        ])
        .block(panel_block("Patch Deck", false))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        f.render_widget(placeholder, area);
        return;
    }

    let additions = state.diff_content.iter().filter(|line| line.starts_with('+')).count();
    let deletions = state.diff_content.iter().filter(|line| line.starts_with('-')).count();
    let title = format!("{}  +{} / -{}", state.diff_title, additions, deletions);

    let lines: Vec<Line> = state
        .diff_content
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let gutter = Span::styled(format!("{:>4} ", idx + 1), Style::default().fg(Palette::TEXT_DIM));
            let style = if line.starts_with('+') {
                Style::default().fg(Palette::DIFF_ADD)
            } else if line.starts_with('-') {
                Style::default().fg(Palette::DIFF_DEL)
            } else if line.starts_with("@@") {
                Style::default().fg(Palette::DIFF_HUNK).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Palette::TEXT)
            };
            Line::from(vec![gutter, Span::styled(line.clone(), style)])
        })
        .collect();

    render_lines_panel(f, area, lines, &mut state.diff_scroll, panel_block(&title, false));
}

fn draw_sessions_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.sessions.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::from(Span::styled(
                "No saved sessions found.",
                Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Sessions are stored in <workspace>/.barqcoder/sessions/.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
        ])
        .block(panel_block("Session Log", false))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        f.render_widget(placeholder, area);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(42), Constraint::Percentage(58)])
        .split(area);

    let items: Vec<ListItem> = state
        .sessions
        .iter()
        .map(|session| {
            ListItem::new(vec![
                Line::from(Span::styled(
                    trim_inline(&session.id, 42),
                    Style::default().fg(Palette::TEXT_BRIGHT).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{} • {} events", format_timestamp(session.created_at), session.event_count),
                    Style::default().fg(Palette::TEXT_DIM),
                )),
                Line::from(Span::styled(
                    trim_inline(&session.workspace, 42),
                    Style::default().fg(Palette::TEXT_DIM),
                )),
                Line::raw(""),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(panel_block("Session Log", false))
        .highlight_style(
            Style::default()
                .bg(Palette::SURFACE2)
                .fg(Palette::TEXT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    f.render_stateful_widget(list, columns[0], &mut state.session_list_state);

    let preview_lines: Vec<Line> = if state.session_preview.is_empty() {
        vec![Line::from(Span::styled(
            "Select a session to inspect its recent activity.",
            Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        state
            .session_preview
            .iter()
            .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Palette::TEXT))))
            .collect()
    };

    let preview = Paragraph::new(preview_lines)
        .block(panel_block(&state.session_preview_title, false))
        .wrap(Wrap { trim: false });
    f.render_widget(preview, columns[1]);
}

fn draw_action_queue_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.action_queue.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::from(Span::styled(
                "No pending actions in the sandbox.",
                Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "When a tool needs approval, its preview will appear here.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
        ])
        .block(panel_block("Approval Gate", false))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        f.render_widget(placeholder, area);
        return;
    }

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(32), Constraint::Percentage(68)])
        .split(area);

    let items: Vec<ListItem> = state
        .action_queue
        .iter()
        .map(|action| {
            let status = match action.approved {
                Some(true) => "approved",
                Some(false) => "rejected",
                None => "pending",
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    action.label(),
                    Style::default().fg(Palette::TEXT_BRIGHT).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{} via {}", status, action.agent),
                    Style::default().fg(Palette::TEXT_DIM),
                )),
                Line::raw(""),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(panel_block("Pending Actions", false))
        .highlight_style(
            Style::default()
                .bg(Palette::SURFACE2)
                .fg(Palette::TEXT_BRIGHT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");

    let mut list_state = ListState::default();
    list_state.select(Some(state.action_queue_selected.min(state.action_queue.len() - 1)));
    f.render_stateful_widget(list, columns[0], &mut list_state);

    let mut preview_lines = vec![
        Line::from(Span::styled(
            "Y approve once. A remember for the session. N reject.",
            Style::default().fg(Palette::WARN_MSG).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
    ];

    preview_lines.extend(
        state.action_queue[state.action_queue_selected.min(state.action_queue.len() - 1)]
            .preview()
            .lines()
            .map(|line| {
                let style = if line.starts_with('+') {
                    Style::default().fg(Palette::DIFF_ADD)
                } else if line.starts_with('-') {
                    Style::default().fg(Palette::DIFF_DEL)
                } else if line.starts_with("@@") {
                    Style::default().fg(Palette::DIFF_HUNK)
                } else {
                    Style::default().fg(Palette::TEXT)
                };
                Line::from(Span::styled(line.to_string(), style))
            }),
    );

    render_lines_panel(
        f,
        columns[1],
        preview_lines,
        &mut state.action_preview_scroll,
        panel_block("Action Preview", false),
    );
}

fn draw_permission_prompt(f: &mut Frame, prompt: &PermissionPrompt) {
    let area = centered_rect(72, 11, f.area());
    let widget = Paragraph::new(vec![
        Line::from(Span::styled(
            prompt.title.as_str(),
            Style::default().fg(Palette::TEXT_BRIGHT).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(prompt.reason.as_str(), Style::default().fg(Palette::TEXT))),
        Line::raw(""),
        Line::from(Span::styled(
            prompt.hint.as_str(),
            Style::default().fg(Palette::WARN_MSG).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            format!("Queued approvals: {}", prompt.queue_len),
            Style::default().fg(Palette::TEXT_DIM),
        )),
    ])
    .block(panel_block("Approval Required", true))
    .wrap(Wrap { trim: true });

    f.render_widget(Clear, area);
    f.render_widget(widget, area);
}

fn draw_keys(f: &mut Frame, area: Rect, state: &TuiState) {
    let lines = match state.active_tab {
        ActiveTab::Chat => vec![
            key_hint_line(&[
                ("Enter", "Send"),
                ("Shift+Enter", "Newline"),
                ("F1", "Focus"),
                ("Tab", "View"),
                ("Esc", "Quit"),
            ]),
            key_hint_line(&[
                ("PgUp/PgDn", "Scroll"),
                ("End", "Live"),
                ("Alt+S", "Rail"),
                ("/", "Commands"),
            ]),
        ],
        ActiveTab::Diff => vec![
            key_hint_line(&[
                ("Up/Down", "Scroll"),
                ("PgUp/PgDn", "Jump"),
                ("Home/End", "Edges"),
                ("Tab", "View"),
                ("Esc", "Quit"),
            ]),
            key_hint_line(&[("Console", "Return to prompt stream")]),
        ],
        ActiveTab::Sessions => vec![
            key_hint_line(&[
                ("Up/Down", "Select"),
                ("Enter", "Replay"),
                ("Tab", "View"),
                ("Esc", "Quit"),
            ]),
            key_hint_line(&[("Preview", "Inspect archived transcript")]),
        ],
        ActiveTab::ActionQueue => vec![
            key_hint_line(&[
                ("Up/Down", "Select"),
                ("PgUp/PgDn", "Preview"),
                ("Y", "Allow Once"),
                ("A", "Allow Session"),
                ("N", "Reject"),
            ]),
            key_hint_line(&[("Tab", "View"), ("Esc", "Quit")]),
        ],
    };

    let footer = Paragraph::new(lines).style(Style::default().bg(Palette::SURFACE2));
    f.render_widget(footer, area);
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────
fn key_hint_line(items: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (index, (key, label)) in items.iter().enumerate() {
        if index > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(chip(*key, Palette::BG, Palette::KEY_LABEL));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            label.to_string(),
            Style::default().fg(Palette::KEY_HINT),
        ));
    }
    Line::from(spans)
}

fn panel_block(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .title(Line::from(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(if focused { Palette::TEXT_BRIGHT } else { Palette::TEXT })
                .add_modifier(Modifier::BOLD),
        )))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            Palette::BORDER_ACTIVE
        } else {
            Palette::BORDER
        }))
        .style(Style::default().bg(Palette::SURFACE))
        .padding(Padding::horizontal(1))
}

fn chip(label: impl Into<String>, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.into()),
        Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
    )
}

fn status_label(state: &TuiState) -> String {
    if state.permission_prompt.is_some() {
        "approval needed".to_string()
    } else if state.is_thinking {
        format!("thinking {}", spinner_frame(state.tick))
    } else if state.is_indexing {
        format!("indexing {}", spinner_frame(state.tick))
    } else {
        "ready".to_string()
    }
}

fn status_color(state: &TuiState) -> Color {
    if state.permission_prompt.is_some() {
        Palette::STATUS_WARN
    } else if state.status_is_error {
        Palette::STATUS_ERR
    } else if state.is_thinking || state.is_indexing {
        Palette::ACCENT
    } else {
        Palette::STATUS_OK
    }
}

fn focus_label(focus: Focus) -> &'static str {
    match focus {
        Focus::Input => "composer",
        Focus::Sidebar => "workspace",
        Focus::Chat => "transcript",
        Focus::ToolLog => "run feed",
    }
}

fn trim_inline(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }

    let char_count = text.chars().count();
    if char_count <= max_chars {
        return text.to_string();
    }

    let head: String = text.chars().take(max_chars.saturating_sub(3)).collect();
    format!("{}...", head)
}

fn render_lines_panel(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line<'static>>,
    scroll: &mut usize,
    block: Block<'static>,
) -> usize {
    let content_height = area.height.saturating_sub(2) as usize;
    let total_lines = lines.len().max(1);
    let max_scroll = total_lines.saturating_sub(content_height);
    let resolved_scroll = resolve_scroll(*scroll, max_scroll);
    *scroll = resolved_scroll;

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .scroll((resolved_scroll.min(u16::MAX as usize) as u16, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);

    if max_scroll > 0 {
        let mut scrollbar_state = ScrollbarState::new(max_scroll).position(resolved_scroll);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"))
                .track_symbol(Some("│"))
                .thumb_symbol("█")
                .style(Style::default().fg(Palette::BORDER)),
            area.inner(Margin { vertical: 1, horizontal: 0 }),
            &mut scrollbar_state,
        );
    }

    max_scroll
}

fn resolve_scroll(requested: usize, max_scroll: usize) -> usize {
    if requested == usize::MAX {
        max_scroll
    } else {
        requested.min(max_scroll)
    }
}

fn build_transcript_lines(messages: &[ChatMessage], width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    for message in messages {
        match message.kind {
            MessageKind::User => push_message_card(
                &mut lines,
                "REQUEST",
                Palette::USER_MSG,
                &message.content,
                &message.kind,
                width,
            ),
            MessageKind::Agent => push_message_card(
                &mut lines,
                "ASSISTANT",
                Palette::AGENT_MSG,
                &message.content,
                &message.kind,
                width,
            ),
            MessageKind::ToolCall => push_message_card(
                &mut lines,
                "TOOL",
                Palette::TOOL_CALL,
                &message.content,
                &message.kind,
                width,
            ),
            MessageKind::ToolResult => push_message_card(
                &mut lines,
                "RESULT",
                Palette::TOOL_RESULT,
                &message.content,
                &message.kind,
                width,
            ),
            MessageKind::System => push_message_card(
                &mut lines,
                "STATUS",
                Palette::ACCENT2,
                &message.content,
                &message.kind,
                width,
            ),
            MessageKind::Error => push_message_card(
                &mut lines,
                "ERROR",
                Palette::ERROR_MSG,
                &message.content,
                &message.kind,
                width,
            ),
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            "No messages yet.",
            Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC),
        )));
    }

    lines
}

fn push_message_card(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    accent: Color,
    body: &str,
    kind: &MessageKind,
    width: usize,
) {
    let rule_width = width.saturating_sub(label.len() + 6).max(4);
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {} ", label),
            Style::default().fg(Palette::BG).bg(accent).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled("─".repeat(rule_width), Style::default().fg(Palette::BORDER)),
    ]));

    match kind {
        MessageKind::Agent => {
            let body_width = width.saturating_sub(3).max(12);
            for markdown_line in markdown::render_markdown(body, body_width) {
                for wrapped in wrap_styled_line(markdown_line, body_width) {
                    let mut spans = vec![Span::styled("  ", Style::default())];
                    spans.extend(wrapped.spans);
                    lines.push(Line::from(spans));
                }
            }
        }
        _ => {
            let style = match kind {
                MessageKind::User => Style::default().fg(Palette::USER_MSG),
                MessageKind::ToolCall => Style::default().fg(Palette::TOOL_CALL),
                MessageKind::ToolResult => Style::default().fg(Palette::TOOL_RESULT),
                MessageKind::System => {
                    Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC)
                }
                MessageKind::Error => Style::default().fg(Palette::ERROR_MSG),
                MessageKind::Agent => Style::default().fg(Palette::TEXT),
            };

            for line in wrap_plain_text(body, width.saturating_sub(3).max(12)) {
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(line, style),
                ]));
            }
        }
    }

    lines.push(Line::raw(""));
}

fn sticky_prompt_summary(messages: &[ChatMessage]) -> Option<String> {
    messages.iter().rev().find_map(|message| {
        if !matches!(message.kind, MessageKind::User) {
            return None;
        }

        let non_empty: Vec<&str> = message
            .content
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if non_empty.is_empty() {
            return None;
        }

        let mut summary = non_empty.first().copied().unwrap_or_default().to_string();
        if non_empty.len() > 1 {
            summary.push_str(&format!(" [+{} lines]", non_empty.len() - 1));
        }
        Some(summary)
    })
}

fn wrap_plain_text(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        lines.extend(wrap_visual_line(line, width));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn wrap_styled_line(line: Line<'static>, width: usize) -> Vec<Line<'static>> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    let mut current_spans: Vec<Span<'static>> = Vec::new();
    let mut current_width = 0usize;

    if line.spans.is_empty() {
        return vec![Line::from("")];
    }

    for span in line.spans {
        let style = span.style;
        let text = span.content.into_owned();
        if text.is_empty() {
            continue;
        }

        let chars: Vec<char> = text.chars().collect();
        let mut index = 0usize;

        while index < chars.len() {
            if current_width == width {
                wrapped.push(Line::from(current_spans));
                current_spans = Vec::new();
                current_width = 0;
            }

            let remaining = width.saturating_sub(current_width).max(1);
            let take = remaining.min(chars.len() - index);
            let chunk: String = chars[index..index + take].iter().collect();
            current_spans.push(Span::styled(chunk, style));
            current_width += take;
            index += take;

            if current_width == width && index < chars.len() {
                wrapped.push(Line::from(current_spans));
                current_spans = Vec::new();
                current_width = 0;
            }
        }
    }

    if current_spans.is_empty() {
        wrapped.push(Line::from(""));
    } else {
        wrapped.push(Line::from(current_spans));
    }

    wrapped
}

fn wrap_visual_line(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if line.is_empty() {
        return vec![String::new()];
    }

    let chars: Vec<char> = line.chars().collect();
    let mut wrapped = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let end = (index + width).min(chars.len());
        wrapped.push(chars[index..end].iter().collect());
        index = end;
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
}

fn build_composer_lines(
    input: &str,
    cursor: usize,
    max_width: usize,
    max_lines: usize,
) -> Vec<Line<'static>> {
    let marker = '\0';
    let mut with_cursor = input.to_string();
    let safe_cursor = cursor.min(with_cursor.len());
    with_cursor.insert(safe_cursor, marker);

    let content_width = max_width.saturating_sub(2).max(1);
    let mut wrapped_lines = Vec::new();
    for line in with_cursor.split('\n') {
        wrapped_lines.extend(wrap_visual_line(line, content_width));
    }

    if wrapped_lines.is_empty() {
        wrapped_lines.push(marker.to_string());
    }

    let visible = max_lines.max(1);
    let start = wrapped_lines.len().saturating_sub(visible);
    wrapped_lines[start..]
        .iter()
        .enumerate()
        .map(|(index, line)| composer_line(index == 0, line, input.is_empty(), marker))
        .collect()
}

fn composer_line(is_first: bool, line: &str, empty_input: bool, marker: char) -> Line<'static> {
    let prefix = if is_first { "› " } else { "  " };
    let mut spans = vec![Span::styled(prefix, Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD))];

    if let Some(marker_index) = line.find(marker) {
        let before = &line[..marker_index];
        let after = &line[marker_index + marker.len_utf8()..];
        let mut after_chars = after.chars();
        let cursor_char = after_chars.next().map(|ch| ch.to_string()).unwrap_or_else(|| " ".to_string());
        let after_rest: String = after_chars.collect();

        if !before.is_empty() {
            spans.push(Span::styled(
                before.to_string(),
                Style::default().fg(Palette::TEXT_BRIGHT),
            ));
        }

        spans.push(Span::styled(
            cursor_char,
            Style::default()
                .fg(Palette::BG)
                .bg(Palette::ACCENT)
                .add_modifier(Modifier::BOLD),
        ));

        if empty_input {
            spans.push(Span::styled(
                "Type a request, paste a task, or use /help".to_string(),
                Style::default().fg(Palette::TEXT_DIM),
            ));
        } else if !after_rest.is_empty() {
            spans.push(Span::styled(
                after_rest,
                Style::default().fg(Palette::TEXT_BRIGHT),
            ));
        }
    } else {
        spans.push(Span::styled(
            line.to_string(),
            Style::default().fg(Palette::TEXT_BRIGHT),
        ));
    }

    Line::from(spans)
}

fn format_timestamp(ts: u64) -> String {
    // Very simple: just show as relative description
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

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let width = area.width.saturating_mul(percent_x).saturating_div(100);
    let x = area.x + area.width.saturating_sub(width) / 2;
    let y = area.y + area.height.saturating_sub(height) / 2;

    Rect {
        x,
        y,
        width,
        height: height.min(area.height),
    }
}
