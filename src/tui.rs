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

// ─────────────────────────────────────────────
// Palette
// ─────────────────────────────────────────────
pub struct Palette;
impl Palette {
    pub const BG: Color = Color::Rgb(13, 14, 20);
    pub const SURFACE: Color = Color::Rgb(22, 24, 35);
    pub const SURFACE2: Color = Color::Rgb(30, 33, 48);
    pub const BORDER: Color = Color::Rgb(50, 56, 82);
    pub const BORDER_ACTIVE: Color = Color::Rgb(100, 120, 220);
    pub const ACCENT: Color = Color::Rgb(110, 130, 255);
    pub const ACCENT2: Color = Color::Rgb(80, 200, 180);
    pub const TEXT: Color = Color::Rgb(210, 215, 240);
    pub const TEXT_DIM: Color = Color::Rgb(100, 108, 140);
    pub const TEXT_BRIGHT: Color = Color::White;
    pub const USER_MSG: Color = Color::Rgb(130, 210, 255);
    pub const AGENT_MSG: Color = Color::Rgb(170, 255, 190);
    pub const ERROR_MSG: Color = Color::Rgb(255, 100, 110);
    pub const WARN_MSG: Color = Color::Rgb(255, 200, 80);
    pub const TOOL_CALL: Color = Color::Rgb(200, 160, 255);
    pub const TOOL_RESULT: Color = Color::Rgb(255, 180, 100);
    pub const DIFF_ADD: Color = Color::Rgb(80, 200, 120);
    pub const DIFF_DEL: Color = Color::Rgb(240, 80, 90);
    pub const DIFF_HUNK: Color = Color::Rgb(100, 180, 255);
    pub const STATUS_OK: Color = Color::Rgb(80, 220, 140);
    pub const STATUS_ERR: Color = Color::Rgb(255, 80, 100);
    pub const STATUS_WARN: Color = Color::Rgb(255, 200, 60);
    pub const KEY_HINT: Color = Color::Rgb(90, 100, 140);
    pub const KEY_LABEL: Color = Color::Rgb(140, 155, 210);
}

// ─────────────────────────────────────────────
// Spinner
// ─────────────────────────────────────────────
const SPINNER_FRAMES: &[&str] = &["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
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
        full_content: String,
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
    pub fn write_file(path: impl Into<String>, patch: impl Into<String>, full_content: impl Into<String>, agent: impl Into<String>) -> Self {
        Self { kind: ActionKind::WriteFile { path: path.into(), patch: patch.into(), full_content: full_content.into() }, agent: agent.into(), approved: None }
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

    // Sidebar / file tree
    pub workspace_files: Vec<String>,
    pub file_list_state: ListState,
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

    // Action Sandbox (Phase 3)
    pub action_queue: Vec<PendingAction>,
    pub action_queue_selected: usize,
    pub action_preview_scroll: usize,

    // Sessions
    pub sessions: Vec<SessionEntry>,
    pub session_list_state: ListState,

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
                "Welcome to BarqCoder ⚡  Type a prompt or /help for commands.",
            )],
            chat_scroll: 0,
            input: String::new(),
            input_cursor: 0,
            input_history: Vec::new(),
            input_history_idx: 0,
            workspace_files: Vec::new(),
            file_list_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            sidebar_visible: true,
            tool_log: Vec::new(),
            tool_scroll: 0,
            current_tool: None,
            barq_context: Vec::new(),
            diff_content: Vec::new(),
            diff_scroll: 0,
            action_queue: Vec::new(),
            action_queue_selected: 0,
            action_preview_scroll: 0,
            sessions: Vec::new(),
            session_list_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
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
        self.chat_scroll = usize::MAX;
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
        self.chat_scroll = usize::MAX;
    }

    pub fn set_diff(&mut self, patch: &str) {
        self.diff_content = patch.lines().map(|l| l.to_string()).collect();
        self.diff_scroll = 0;
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
        Some(trimmed)
    }
}

// ─────────────────────────────────────────────
// Main draw function
// ─────────────────────────────────────────────
pub fn draw(f: &mut Frame, state: &mut TuiState) {
    // Root background
    f.render_widget(
        Block::default().style(Style::default().bg(Palette::BG)),
        f.area(),
    );

    let full = f.area();

    // Outer vertical split: [header | body | keybindings]
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / tabs
            Constraint::Min(0),    // body
            Constraint::Length(1), // keybindings bar
        ])
        .split(full);

    draw_header(f, outer[0], state);
    draw_body(f, outer[1], state);
    draw_keys(f, outer[2], state);
}

// ─────────────────────────────────────────────
// Header: logo + tab bar + session info
// ─────────────────────────────────────────────
fn draw_header(f: &mut Frame, area: Rect, state: &TuiState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22), // logo
            Constraint::Min(0),     // tab bar
            Constraint::Length(36), // session info
        ])
        .split(area);

    // Logo
    let logo = Paragraph::new(Span::styled(
        " ⚡ BarqCoder ",
        Style::default()
            .fg(Palette::ACCENT)
            .add_modifier(Modifier::BOLD),
    ))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Palette::BORDER))
            .style(Style::default().bg(Palette::SURFACE)),
    )
    .alignment(Alignment::Center);
    f.render_widget(logo, cols[0]);

    // Tab bar
    let tab_titles: Vec<Line> = vec![
        Line::from(vec![
            Span::raw(" "),
            Span::styled("󰭻 Chat", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" "),
            Span::styled(" Diff", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" "),
            Span::styled("󱁻 Sessions", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(" "),
        ]),
        Line::from(vec![
            Span::raw(" "),
            Span::styled("󰒄 Sandbox", Style::default().add_modifier(Modifier::BOLD)),
            Span::raw(format!(" [{}]", state.action_queue.len())),
        ]),
    ];

    let tabs = Tabs::new(tab_titles)
        .select(state.active_tab as usize)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .highlight_style(
            Style::default()
                .fg(Palette::ACCENT)
                .bg(Palette::SURFACE2)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::styled("│", Style::default().fg(Palette::BORDER)));
    f.render_widget(tabs, cols[1]);

    // Session / model info
    let tok_color = if state.token_count > state.token_limit {
        Palette::STATUS_ERR
    } else if state.token_count > state.token_limit * 8 / 10 {
        Palette::STATUS_WARN
    } else {
        Palette::STATUS_OK
    };

    let state_icon = if state.is_thinking {
        format!("{} Thinking", spinner_frame(state.tick))
    } else if state.is_indexing {
        format!("{} Indexing", spinner_frame(state.tick))
    } else {
        "󰗡 Ready".to_string()
    };

    let info_lines = vec![
        Line::from(vec![
            Span::styled("  ", Style::default().fg(Palette::TEXT_DIM)),
            Span::styled(
                state.current_model.as_str(),
                Style::default().fg(Palette::ACCENT2).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Tokens: ", Style::default().fg(Palette::TEXT_DIM)),
            Span::styled(
                format!("{}/{}", state.token_count, state.token_limit),
                Style::default().fg(tok_color),
            ),
            Span::styled("  ", Style::default().fg(Palette::TEXT_DIM)),
            Span::styled(
                &state_icon,
                Style::default()
                    .fg(if state.is_thinking || state.is_indexing {
                        Palette::ACCENT
                    } else {
                        Palette::STATUS_OK
                    })
                    .add_modifier(Modifier::BOLD),
            ),
        ]),
    ];

    let info = Paragraph::new(info_lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .alignment(Alignment::Left);
    f.render_widget(info, cols[2]);
}

// ─────────────────────────────────────────────
// Body dispatcher
// ─────────────────────────────────────────────
fn draw_body(f: &mut Frame, area: Rect, state: &mut TuiState) {
    match state.active_tab {
        ActiveTab::Chat => draw_chat_tab(f, area, state),
        ActiveTab::Diff => draw_diff_tab(f, area, state),
        ActiveTab::Sessions => draw_sessions_tab(f, area, state),
        ActiveTab::ActionQueue => draw_action_queue_tab(f, area, state),
    }
}

// ─────────────────────────────────────────────
// Chat tab
// ─────────────────────────────────────────────
fn draw_chat_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    // Horizontal: sidebar | main
    let h_chunks = if state.sidebar_visible {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(28), Constraint::Min(0)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(0), Constraint::Min(0)])
            .split(area)
    };

    if state.sidebar_visible {
        draw_sidebar(f, h_chunks[0], state);
    }

    // Main area: chat history | tool log | input
    let main_area = h_chunks[1];
    let v_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),    // chat
            Constraint::Length(8), // tool log
            Constraint::Length(3), // input
        ])
        .split(main_area);

    draw_chat_area(f, v_chunks[0], state);
    draw_tool_log(f, v_chunks[1], state);
    draw_input(f, v_chunks[2], state);
}

// ─────────────────────────────────────────────
// Sidebar: file tree + context info
// ─────────────────────────────────────────────
fn draw_sidebar(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let is_focused = state.focus == Focus::Sidebar;
    let border_color = if is_focused {
        Palette::BORDER_ACTIVE
    } else {
        Palette::BORDER
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // file list
            Constraint::Length(6), // barq context summary
        ])
        .split(area);

    // File list
    let files: Vec<ListItem> = state
        .workspace_files
        .iter()
        .map(|f| {
            let icon = if f.ends_with(".rs") {
                "󱘗 "
            } else if f.ends_with(".toml") {
                " "
            } else if f.ends_with(".md") {
                "󰍔 "
            } else {
                "󰈔 "
            };
            ListItem::new(Line::from(vec![
                Span::styled(icon, Style::default().fg(Palette::ACCENT2)),
                Span::styled(f.as_str(), Style::default().fg(Palette::TEXT)),
            ]))
        })
        .collect();

    let file_list = List::new(files)
        .block(
            Block::default()
                .title(Line::from(vec![
                    Span::styled("  ", Style::default().fg(Palette::ACCENT)),
                    Span::styled(
                        "Workspace",
                        Style::default()
                            .fg(Palette::TEXT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
                .borders(Borders::ALL)
                .border_type(if is_focused {
                    BorderType::Double
                } else {
                    BorderType::Rounded
                })
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(Palette::SURFACE))
                .padding(Padding::horizontal(1)),
        )
        .highlight_style(
            Style::default()
                .bg(Palette::SURFACE2)
                .fg(Palette::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(file_list, chunks[0], &mut state.file_list_state);

    // Context summary
    let ctx_lines: Vec<Line> = state
        .barq_context
        .iter()
        .rev()
        .take(3)
        .map(|l| {
            Line::from(Span::styled(
                format!("  {} ", l),
                Style::default().fg(Palette::TEXT_DIM),
            ))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let ctx_p = Paragraph::new(ctx_lines)
        .block(
            Block::default()
                .title(Line::from(vec![
                    Span::styled("  ", Style::default().fg(Palette::ACCENT2)),
                    Span::styled(
                        "BARQ Context",
                        Style::default()
                            .fg(Palette::TEXT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE))
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: true });
    f.render_widget(ctx_p, chunks[1]);
}

// ─────────────────────────────────────────────
// Chat area
// ─────────────────────────────────────────────
fn draw_chat_area(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let is_focused = state.focus == Focus::Chat;
    let border_color = if is_focused {
        Palette::BORDER_ACTIVE
    } else {
        Palette::BORDER
    };

    // Build rich lines
    let mut lines: Vec<Line> = Vec::new();
    for msg in &state.messages {
        match &msg.kind {
            MessageKind::User => {
                lines.push(Line::from(vec![
                    Span::styled(
                        " ▶ You  ",
                        Style::default()
                            .fg(Palette::USER_MSG)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "─".repeat(area.width.saturating_sub(12) as usize),
                        Style::default().fg(Palette::BORDER),
                    ),
                ]));
                for l in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("   {} ", l),
                        Style::default().fg(Palette::USER_MSG),
                    )));
                }
                lines.push(Line::raw(""));
            }
            MessageKind::Agent => {
                lines.push(Line::from(vec![
                    Span::styled(
                        " 󰭻 BarqCoder  ",
                        Style::default()
                            .fg(Palette::AGENT_MSG)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "─".repeat(area.width.saturating_sub(16) as usize),
                        Style::default().fg(Palette::BORDER),
                    ),
                ]));
                for l in msg.content.lines() {
                    lines.push(Line::from(Span::styled(
                        format!("   {} ", l),
                        Style::default().fg(Palette::TEXT),
                    )));
                }
                lines.push(Line::raw(""));
            }
            MessageKind::ToolCall => {
                for l in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  TOOL CALL  ",
                            Style::default().fg(Palette::TOOL_CALL).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{} ", l),
                            Style::default().fg(Palette::TOOL_CALL),
                        ),
                    ]));
                }
            }
            MessageKind::ToolResult => {
                for l in msg.content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled(
                            "  RESULT     ",
                            Style::default().fg(Palette::TOOL_RESULT).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("{} ", l),
                            Style::default().fg(Palette::TOOL_RESULT),
                        ),
                    ]));
                }
            }
            MessageKind::System => {
                lines.push(Line::from(Span::styled(
                    format!("  ◆ {} ", msg.content),
                    Style::default()
                        .fg(Palette::TEXT_DIM)
                        .add_modifier(Modifier::ITALIC),
                )));
                lines.push(Line::raw(""));
            }
            MessageKind::Error => {
                lines.push(Line::from(vec![
                    Span::styled(
                        "  ERROR  ",
                        Style::default()
                            .fg(Palette::ERROR_MSG)
                            .add_modifier(Modifier::BOLD | Modifier::RAPID_BLINK),
                    ),
                    Span::styled(
                        format!("{} ", msg.content),
                        Style::default().fg(Palette::ERROR_MSG),
                    ),
                ]));
                lines.push(Line::raw(""));
            }
        }
    }

    // If thinking, append animated "..." line
    if state.is_thinking {
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {} ", spinner_frame(state.tick)),
                Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "BarqCoder is thinking…",
                Style::default()
                    .fg(Palette::TEXT_DIM)
                    .add_modifier(Modifier::ITALIC),
            ),
        ]));
    }

    let total_lines = lines.len();
    let available_height = area.height.saturating_sub(2) as usize;
    let scroll = if total_lines > available_height {
        // auto-follow bottom unless user has scrolled up
        if state.chat_scroll == usize::MAX || state.chat_scroll + available_height >= total_lines {
            // Update state so internal offset isn't left at MAX
            let bottom = total_lines.saturating_sub(available_height);
            state.chat_scroll = bottom;
            bottom
        } else {
            state.chat_scroll
        }
    } else {
        0
    };

    // Scrollbar
    let mut scrollbar_state = ScrollbarState::new(total_lines.saturating_sub(available_height))
        .position(scroll);

    let spinner_title = if state.is_thinking {
        format!(" {} BarqCoder Chat ", spinner_frame(state.tick))
    } else {
        " 󰭻 BarqCoder Chat ".to_string()
    };

    let chat_p = Paragraph::new(Text::from(lines))
        .block(
            Block::default()
                .title(Line::from(vec![
                    Span::styled(
                        spinner_title,
                        Style::default()
                            .fg(Palette::ACCENT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]))
                .borders(Borders::ALL)
                .border_type(if is_focused {
                    BorderType::Double
                } else {
                    BorderType::Rounded
                })
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(chat_p, area);

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

// ─────────────────────────────────────────────
// Tool log
// ─────────────────────────────────────────────
fn draw_tool_log(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let is_focused = state.focus == Focus::ToolLog;
    let border_color = if is_focused {
        Palette::BORDER_ACTIVE
    } else {
        Palette::BORDER
    };

    let tool_title = match &state.current_tool {
        Some(t) => Line::from(vec![
            Span::styled(
                "  Tool Activity ",
                Style::default()
                    .fg(Palette::TOOL_CALL)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("[Active: {}] ", t),
                Style::default()
                    .fg(Palette::WARN_MSG)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{}", spinner_frame(state.tick)),
                Style::default().fg(Palette::ACCENT),
            ),
        ]),
        None => Line::from(Span::styled(
            "  Tool Activity ",
            Style::default()
                .fg(Palette::TOOL_CALL)
                .add_modifier(Modifier::BOLD),
        )),
    };

    let log_lines: Vec<Line> = state
        .tool_log
        .iter()
        .map(|entry| {
            let (icon, color) = if entry.contains("Calling") {
                ("▶ ", Palette::TOOL_CALL)
            } else if entry.contains("Result") {
                ("◀ ", Palette::TOOL_RESULT)
            } else {
                ("  ", Palette::TEXT_DIM)
            };
            Line::from(vec![
                Span::styled(icon, Style::default().fg(color).add_modifier(Modifier::BOLD)),
                Span::styled(entry.as_str(), Style::default().fg(color)),
            ])
        })
        .collect();

    let total = log_lines.len();
    let height = area.height.saturating_sub(2) as usize;
    let scroll = if total > height {
        (total - height) as u16
    } else {
        0
    };

    let tool_p = Paragraph::new(Text::from(log_lines))
        .block(
            Block::default()
                .title(tool_title)
                .borders(Borders::ALL)
                .border_type(if is_focused {
                    BorderType::Double
                } else {
                    BorderType::Rounded
                })
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(Palette::SURFACE))
                .padding(Padding::horizontal(1)),
        )
        .scroll((scroll, 0))
        .wrap(Wrap { trim: true });
    f.render_widget(tool_p, area);
}

// ─────────────────────────────────────────────
// Input box
// ─────────────────────────────────────────────
fn draw_input(f: &mut Frame, area: Rect, state: &TuiState) {
    let is_focused = state.focus == Focus::Input;
    let border_color = if is_focused {
        Palette::BORDER_ACTIVE
    } else {
        Palette::BORDER
    };

    // Render cursor by splitting input at cursor position
    let before_cursor = &state.input[..state.input_cursor];
    let after_cursor = &state.input[state.input_cursor..];

    let cursor_char = if after_cursor.is_empty() { " " } else { &after_cursor[..after_cursor.chars().next().map(|c| c.len_utf8()).unwrap_or(1)] };
    let after_cursor_rest = if after_cursor.len() > cursor_char.len() { &after_cursor[cursor_char.len()..] } else { "" };


    // Combine prompt + input
    let full_input = Line::from(vec![
        if state.is_thinking {
            Span::styled(
                format!(" {} › ", spinner_frame(state.tick)),
                Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(" ❯ ", Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD))
        },
        Span::styled(before_cursor, Style::default().fg(Palette::TEXT_BRIGHT)),
        Span::styled(
            cursor_char,
            Style::default()
                .fg(Palette::BG)
                .bg(Palette::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(after_cursor_rest, Style::default().fg(Palette::TEXT_BRIGHT)),
    ]);

    let input_p = Paragraph::new(full_input)
        .block(
            Block::default()
                .title(Line::from(vec![
                    Span::styled(
                        " Input ",
                        Style::default()
                            .fg(Palette::TEXT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "— /help for commands ",
                        Style::default().fg(Palette::TEXT_DIM),
                    ),
                ]))
                .borders(Borders::ALL)
                .border_type(if is_focused {
                    BorderType::Double
                } else {
                    BorderType::Rounded
                })
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(input_p, area);

    // Status bar inside input area (top-right corner)
    if let Some(ref msg) = state.status_message {
        let status_color = if state.status_is_error {
            Palette::STATUS_ERR
        } else {
            Palette::STATUS_OK
        };
        let overlay = Paragraph::new(Span::styled(
            format!(" {} ", msg),
            Style::default().fg(status_color).add_modifier(Modifier::BOLD),
        ));
        // Render in a small overlay at bottom-right of chat area
        let status_area = Rect {
            x: area.x + area.width.saturating_sub(msg.len() as u16 + 6),
            y: area.y,
            width: (msg.len() as u16 + 4).min(area.width / 2),
            height: 1,
        };
        f.render_widget(Clear, status_area);
        f.render_widget(overlay, status_area);
    }
}

// ─────────────────────────────────────────────
// Diff tab
// ─────────────────────────────────────────────
fn draw_diff_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let total = state.diff_content.len();

    if total == 0 {
        let placeholder = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  No diff loaded yet.",
                Style::default()
                    .fg(Palette::TEXT_DIM)
                    .add_modifier(Modifier::ITALIC),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  Run a code edit from the Chat tab and the diff will appear here.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
        ])
        .block(
            Block::default()
                .title(Line::from(Span::styled(
                    "  Diff View ",
                    Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
                )))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .alignment(Alignment::Center);
        f.render_widget(placeholder, area);
        return;
    }

    let diff_lines: Vec<Line> = state
        .diff_content
        .iter()
        .map(|line| {
            if line.starts_with('+') {
                Line::from(vec![
                    Span::styled("+ ", Style::default().fg(Palette::DIFF_ADD).add_modifier(Modifier::BOLD)),
                    Span::styled(&line[1..], Style::default().fg(Palette::DIFF_ADD)),
                ])
            } else if line.starts_with('-') {
                Line::from(vec![
                    Span::styled("- ", Style::default().fg(Palette::DIFF_DEL).add_modifier(Modifier::BOLD)),
                    Span::styled(&line[1..], Style::default().fg(Palette::DIFF_DEL)),
                ])
            } else if line.starts_with("@@") {
                Line::from(Span::styled(
                    line.as_str(),
                    Style::default().fg(Palette::DIFF_HUNK).add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(Span::styled(
                    format!("  {}", line),
                    Style::default().fg(Palette::TEXT_DIM),
                ))
            }
        })
        .collect();

    let height = area.height.saturating_sub(2) as usize;
    let scroll = state.diff_scroll.min(total.saturating_sub(height));
    let mut scrollbar_state = ScrollbarState::new(total.saturating_sub(height)).position(scroll);

    // Legend row at top
    let _legend_area = Rect { x: area.x, y: area.y, width: area.width, height: 1 };
    let diff_area = Rect { x: area.x, y: area.y, width: area.width, height: area.height };

    let diff_p = Paragraph::new(Text::from(diff_lines))
        .block(
            Block::default()
                .title(Line::from(vec![
                    Span::styled(
                        "  Diff View ",
                        Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{} lines] ", total),
                        Style::default().fg(Palette::TEXT_DIM),
                    ),
                ]))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE))
                .padding(Padding::horizontal(1)),
        )
        .scroll((scroll as u16, 0));
    f.render_widget(diff_p, diff_area);

    f.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"))
            .thumb_symbol("█")
            .style(Style::default().fg(Palette::BORDER)),
        diff_area.inner(Margin { vertical: 1, horizontal: 0 }),
        &mut scrollbar_state,
    );
}

// ─────────────────────────────────────────────
// Sessions tab
// ─────────────────────────────────────────────
fn draw_sessions_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.sessions.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  No saved sessions found.",
                Style::default()
                    .fg(Palette::TEXT_DIM)
                    .add_modifier(Modifier::ITALIC),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "  Sessions are saved in <workspace>/.barqcoder/sessions/",
                Style::default().fg(Palette::TEXT_DIM),
            )),
        ])
        .block(
            Block::default()
                .title(Line::from(Span::styled(
                    " 󱁻 Sessions ",
                    Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
                )))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .alignment(Alignment::Center);
        f.render_widget(placeholder, area);
        return;
    }

    let items: Vec<ListItem> = state
        .sessions
        .iter()
        .map(|s| {
            let time = format_timestamp(s.created_at);
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled("  ", Style::default().fg(Palette::ACCENT2)),
                    Span::styled(
                        s.id.as_str(),
                        Style::default()
                            .fg(Palette::TEXT_BRIGHT)
                            .add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(
                        format!("{} • {} events • {}", time, s.event_count, s.workspace),
                        Style::default().fg(Palette::TEXT_DIM),
                    ),
                ]),
                Line::raw(""),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(Line::from(Span::styled(
                    " 󱁻 Sessions ",
                    Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
                )))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE))
                .padding(Padding::horizontal(1)),
        )
        .highlight_style(
            Style::default()
                .bg(Palette::SURFACE2)
                .fg(Palette::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, area, &mut state.session_list_state);
}

// ─────────────────────────────────────────────
// Action Queue (Sandbox) tab
// ─────────────────────────────────────────────
fn draw_action_queue_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.action_queue.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::raw(""),
            Line::from(Span::styled(
                "  No pending actions in the sandbox.",
                Style::default()
                    .fg(Palette::TEXT_DIM)
                    .add_modifier(Modifier::ITALIC),
            )),
            Line::raw(""),
        ])
        .block(
            Block::default()
                .title(Line::from(Span::styled(
                    " 󰒄 Sandbox ",
                    Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
                )))
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .alignment(Alignment::Center);
        f.render_widget(placeholder, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(area);

    let items: Vec<ListItem> = state.action_queue.iter().map(|a| {
        let icon = match a.kind {
            ActionKind::WriteFile { .. } => "📝",
            ActionKind::ShellCommand { .. } => "💻",
            ActionKind::ApplyVerifiedPatch { .. } => "✅",
        };
        let status = match a.approved {
            Some(true) => Span::styled(" [Approved]", Style::default().fg(Palette::STATUS_OK)),
            Some(false) => Span::styled(" [Rejected]", Style::default().fg(Palette::STATUS_ERR)),
            None => Span::styled(" [Pending]", Style::default().fg(Palette::STATUS_WARN)),
        };
        ListItem::new(Line::from(vec![
            Span::raw(format!("{} ", icon)),
            Span::styled(a.label(), Style::default().fg(Palette::TEXT_BRIGHT)),
            status,
        ]))
    }).collect();

    let list = List::new(items)
        .block(
            Block::default()
                .title(" Pending Actions ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::SURFACE)),
        )
        .highlight_style(
            Style::default()
                .bg(Palette::SURFACE2)
                .fg(Palette::ACCENT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    let mut list_state = ListState::default();
    list_state.select(Some(state.action_queue_selected));
    f.render_stateful_widget(list, chunks[0], &mut list_state);

    let preview_text = state.action_queue[state.action_queue_selected].preview().lines().enumerate().map(|(i, l)| {
        let style = if l.starts_with('+') {
            Style::default().fg(Palette::DIFF_ADD)
        } else if l.starts_with('-') {
            Style::default().fg(Palette::DIFF_DEL)
        } else if l.starts_with("@@") {
            Style::default().fg(Palette::DIFF_HUNK)
        } else {
            Style::default().fg(Palette::TEXT)
        };
        Line::from(Span::styled(l, style))
    }).collect::<Vec<_>>();

    let preview = Paragraph::new(preview_text)
        .block(
            Block::default()
                .title(" Preview ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Palette::BORDER))
                .style(Style::default().bg(Palette::BG)),
        )
        .scroll((state.action_preview_scroll as u16, 0));

    f.render_widget(preview, chunks[1]);
}

// ─────────────────────────────────────────────
// Keybindings bar
// ─────────────────────────────────────────────
fn draw_keys(f: &mut Frame, area: Rect, state: &TuiState) {
    let keys: &[(&str, &str)] = match state.active_tab {
        ActiveTab::Chat => &[
            ("Enter", "Send"),
            ("↑/↓", "History"),
            ("Tab", "Next Tab"),
            ("Alt+S", "Toggle Sidebar"),
            ("PgUp/Dn", "Scroll Chat"),
            ("Esc", "Quit"),
        ],
        ActiveTab::Diff => &[
            ("↑/↓", "Scroll Diff"),
            ("Tab", "Next Tab"),
            ("Esc", "Quit"),
        ],
        ActiveTab::Sessions => &[
            ("↑/↓", "Select"),
            ("Enter", "Replay"),
            ("Tab", "Next Tab"),
            ("Esc", "Quit"),
        ],
        ActiveTab::ActionQueue => &[
            ("↑/↓", "Select"),
            ("PgUp/Dn", "Scroll Preview"),
            ("Y", "Approve"),
            ("N", "Reject"),
            ("Tab", "Next Tab"),
            ("Esc", "Quit"),
        ],
    };

    let mut spans: Vec<Span> = vec![Span::styled(" ", Style::default())];
    for (key, desc) in keys {
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default()
                .fg(Palette::BG)
                .bg(Palette::KEY_LABEL)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {} ", desc),
            Style::default().fg(Palette::KEY_HINT),
        ));
        spans.push(Span::styled("  ", Style::default()));
    }

    let keys_bar = Paragraph::new(Line::from(spans))
        .style(Style::default().bg(Palette::SURFACE2));
    f.render_widget(keys_bar, area);
}

// ─────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────
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
