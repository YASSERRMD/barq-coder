use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Tabs, Wrap,
    },
    Frame,
};

pub struct Palette;

impl Palette {
    pub const BG: Color = Color::Rgb(18, 15, 12);
    pub const PANEL: Color = Color::Rgb(29, 24, 19);
    pub const PANEL_ALT: Color = Color::Rgb(38, 31, 25);
    pub const PANEL_MUTED: Color = Color::Rgb(49, 40, 32);
    pub const BORDER: Color = Color::Rgb(111, 91, 72);
    pub const BORDER_ACTIVE: Color = Color::Rgb(228, 177, 102);
    pub const TEXT: Color = Color::Rgb(245, 236, 220);
    pub const TEXT_DIM: Color = Color::Rgb(191, 173, 151);
    pub const TEXT_MUTED: Color = Color::Rgb(149, 130, 111);
    pub const BRAND: Color = Color::Rgb(228, 177, 102);
    pub const USER: Color = Color::Rgb(141, 204, 160);
    pub const AGENT: Color = Color::Rgb(246, 212, 124);
    pub const TOOL: Color = Color::Rgb(123, 181, 235);
    pub const RESULT: Color = Color::Rgb(232, 154, 94);
    pub const ERROR: Color = Color::Rgb(240, 116, 116);
    pub const SUCCESS: Color = Color::Rgb(118, 193, 138);
    pub const WARNING: Color = Color::Rgb(244, 198, 104);
    pub const DIFF_ADD: Color = Color::Rgb(126, 206, 115);
    pub const DIFF_DEL: Color = Color::Rgb(239, 121, 121);
    pub const DIFF_HUNK: Color = Color::Rgb(124, 181, 235);
    pub const KEY: Color = Color::Rgb(172, 143, 112);
}

const SPINNER_FRAMES: &[&str] = &["-", "\\", "|", "/"];

pub fn spinner_frame(tick: usize) -> &'static str {
    SPINNER_FRAMES[tick % SPINNER_FRAMES.len()]
}

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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Input,
    Sidebar,
    Chat,
    ToolLog,
}

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
        Self {
            kind: MessageKind::User,
            content: s.into(),
        }
    }

    pub fn agent(s: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::Agent,
            content: s.into(),
        }
    }

    pub fn tool_call(s: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::ToolCall,
            content: s.into(),
        }
    }

    pub fn tool_result(s: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::ToolResult,
            content: s.into(),
        }
    }

    pub fn system(s: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::System,
            content: s.into(),
        }
    }

    pub fn error(s: impl Into<String>) -> Self {
        Self {
            kind: MessageKind::Error,
            content: s.into(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum ActionKind {
    WriteFile {
        path: String,
        patch: String,
        full_content: String,
    },
    ShellCommand {
        command: String,
        reason: String,
    },
    ApplyVerifiedPatch {
        patch: String,
        step_id: String,
    },
}

#[derive(Clone, Debug)]
pub struct PendingAction {
    pub kind: ActionKind,
    pub agent: String,
    pub approved: Option<bool>,
}

impl PendingAction {
    pub fn write_file(
        path: impl Into<String>,
        patch: impl Into<String>,
        full_content: impl Into<String>,
        agent: impl Into<String>,
    ) -> Self {
        Self {
            kind: ActionKind::WriteFile {
                path: path.into(),
                patch: patch.into(),
                full_content: full_content.into(),
            },
            agent: agent.into(),
            approved: None,
        }
    }

    pub fn shell_cmd(
        command: impl Into<String>,
        reason: impl Into<String>,
        agent: impl Into<String>,
    ) -> Self {
        Self {
            kind: ActionKind::ShellCommand {
                command: command.into(),
                reason: reason.into(),
            },
            agent: agent.into(),
            approved: None,
        }
    }

    pub fn verified_patch(
        patch: impl Into<String>,
        step_id: impl Into<String>,
        agent: impl Into<String>,
    ) -> Self {
        Self {
            kind: ActionKind::ApplyVerifiedPatch {
                patch: patch.into(),
                step_id: step_id.into(),
            },
            agent: agent.into(),
            approved: None,
        }
    }

    pub fn label(&self) -> String {
        match &self.kind {
            ActionKind::WriteFile { path, .. } => format!("Write {}", path),
            ActionKind::ShellCommand { command, .. } => {
                format!("Shell {}", trim_chars(command, 48))
            }
            ActionKind::ApplyVerifiedPatch { step_id, .. } => {
                format!("Patch {}", step_id)
            }
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
pub struct SessionEntry {
    pub id: String,
    pub created_at: u64,
    pub event_count: usize,
    pub workspace: String,
}

#[derive(Clone, Debug)]
pub struct PermissionPrompt {
    pub title: String,
    pub reason: String,
    pub hint: String,
    pub queue_len: usize,
}

pub struct TuiState {
    pub active_tab: ActiveTab,
    pub focus: Focus,
    pub messages: Vec<ChatMessage>,
    pub chat_scroll: usize,
    pub chat_follow: bool,
    pub chat_resolved_scroll: usize,
    pub input: String,
    pub input_cursor: usize,
    pub input_history: Vec<String>,
    pub input_history_idx: usize,
    pub autocomplete_idx: usize,
    pub workspace_files: Vec<String>,
    pub file_list_state: ListState,
    pub sidebar_visible: bool,
    pub tool_log: Vec<String>,
    pub tool_scroll: usize,
    pub tool_follow: bool,
    pub tool_resolved_scroll: usize,
    pub current_tool: Option<String>,
    pub barq_context: Vec<String>,
    pub diff_content: Vec<String>,
    pub diff_scroll: usize,
    pub action_queue: Vec<PendingAction>,
    pub action_queue_selected: usize,
    pub action_preview_scroll: usize,
    pub permission_prompt: Option<PermissionPrompt>,
    pub sessions: Vec<SessionEntry>,
    pub session_list_state: ListState,
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
        let mut file_list_state = ListState::default();
        file_list_state.select(Some(0));

        let mut session_list_state = ListState::default();
        session_list_state.select(Some(0));

        Self {
            active_tab: ActiveTab::Chat,
            focus: Focus::Input,
            messages: vec![ChatMessage::system(
                "BarqCoder deck online. Ask for a change, a review, or a command run.",
            )],
            chat_scroll: usize::MAX,
            chat_follow: true,
            chat_resolved_scroll: 0,
            input: String::new(),
            input_cursor: 0,
            input_history: Vec::new(),
            input_history_idx: 0,
            autocomplete_idx: 0,
            workspace_files: Vec::new(),
            file_list_state,
            sidebar_visible: true,
            tool_log: Vec::new(),
            tool_scroll: usize::MAX,
            tool_follow: true,
            tool_resolved_scroll: 0,
            current_tool: None,
            barq_context: Vec::new(),
            diff_content: Vec::new(),
            diff_scroll: 0,
            action_queue: Vec::new(),
            action_queue_selected: 0,
            action_preview_scroll: 0,
            permission_prompt: None,
            sessions: Vec::new(),
            session_list_state,
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
        if self.chat_follow {
            self.chat_scroll = usize::MAX;
        }
    }

    pub fn append_agent_token(&mut self, token: &str) {
        match self.messages.last_mut() {
            Some(last) if matches!(last.kind, MessageKind::Agent) => last.content.push_str(token),
            _ => self.messages.push(ChatMessage::agent(token)),
        }
        if self.chat_follow {
            self.chat_scroll = usize::MAX;
        }
    }

    pub fn set_diff(&mut self, patch: &str) {
        self.diff_content = patch.lines().map(|line| line.to_string()).collect();
        self.diff_scroll = 0;
        self.active_tab = ActiveTab::Diff;
    }

    pub fn add_tool_log_entry(&mut self, entry: impl Into<String>) {
        self.tool_log.push(entry.into());
        if self.tool_follow {
            self.tool_scroll = usize::MAX;
        }
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

    pub fn input_delete_back(&mut self) {
        if self.input_cursor == 0 {
            return;
        }

        if let Some((idx, _)) = self.input[..self.input_cursor].char_indices().last() {
            self.input.drain(idx..self.input_cursor);
            self.input_cursor = idx;
        }
        self.autocomplete_idx = 0;
    }

    pub fn input_move_left(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        if let Some((idx, _)) = self.input[..self.input_cursor].char_indices().last() {
            self.input_cursor = idx;
        }
    }

    pub fn input_move_right(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }

        if let Some((idx, ch)) = self.input[self.input_cursor..].char_indices().next() {
            self.input_cursor += idx + ch.len_utf8();
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

        self.input = self.input_history[self.input_history_idx].clone();
        self.input_cursor = self.input.len();
    }

    pub fn history_next(&mut self) {
        if self.input_history.is_empty() {
            return;
        }

        if self.input_history_idx + 1 < self.input_history.len() {
            self.input_history_idx += 1;
            self.input = self.input_history[self.input_history_idx].clone();
        } else {
            self.input_history_idx = self.input_history.len();
            self.input.clear();
        }
        self.input_cursor = self.input.len();
    }

    pub fn commit_input(&mut self) -> Option<String> {
        let committed = self.input.clone();
        if committed.trim().is_empty() {
            return None;
        }

        self.input_history.push(committed.clone());
        self.input_history_idx = self.input_history.len();
        self.input.clear();
        self.input_cursor = 0;
        self.autocomplete_idx = 0;
        Some(committed)
    }

    pub fn cycle_focus(&mut self) {
        self.focus = match (self.focus, self.sidebar_visible) {
            (Focus::Input, _) => Focus::Chat,
            (Focus::Chat, _) => Focus::ToolLog,
            (Focus::ToolLog, true) => Focus::Sidebar,
            (Focus::ToolLog, false) => Focus::Input,
            (Focus::Sidebar, _) => Focus::Input,
        };
    }

    pub fn follow_chat(&mut self) {
        self.chat_follow = true;
        self.chat_scroll = usize::MAX;
    }

    pub fn scroll_chat_by(&mut self, delta: isize) {
        let base = if self.chat_follow {
            self.chat_resolved_scroll
        } else {
            self.chat_scroll
        };
        self.chat_follow = false;
        self.chat_scroll = apply_scroll_delta(base, delta);
    }

    pub fn follow_tool_log(&mut self) {
        self.tool_follow = true;
        self.tool_scroll = usize::MAX;
    }

    pub fn scroll_tool_by(&mut self, delta: isize) {
        let base = if self.tool_follow {
            self.tool_resolved_scroll
        } else {
            self.tool_scroll
        };
        self.tool_follow = false;
        self.tool_scroll = apply_scroll_delta(base, delta);
    }

    pub fn latest_user_prompt_preview(&self) -> Option<String> {
        self.messages
            .iter()
            .rev()
            .find(|message| matches!(message.kind, MessageKind::User))
            .map(|message| preview_multiline(&message.content, 3, 180))
    }

    pub fn slash_commands() -> &'static [(&'static str, &'static str)] {
        &[
            ("/help", "Show built-in help"),
            ("/clear", "Clear the conversation"),
            ("/config", "Show runtime config"),
            ("/goal", "Run a multi-agent goal"),
            ("/diff", "Open the latest diff"),
            ("/sessions", "Open saved sessions"),
        ]
    }

    pub fn get_autocomplete_matches(&self) -> Vec<(&'static str, &'static str)> {
        if !self.input.starts_with('/') || self.input.is_empty() {
            return Vec::new();
        }

        Self::slash_commands()
            .iter()
            .filter(|(command, _)| command.starts_with(self.input.as_str()))
            .copied()
            .collect()
    }

    pub fn is_autocomplete_active(&self) -> bool {
        self.input.starts_with('/') && !self.get_autocomplete_matches().is_empty()
    }

    pub fn autocomplete_up(&mut self) {
        if self.autocomplete_idx > 0 {
            self.autocomplete_idx -= 1;
        }
    }

    pub fn autocomplete_down(&mut self) {
        let count = self.get_autocomplete_matches().len();
        if count > 0 && self.autocomplete_idx + 1 < count {
            self.autocomplete_idx += 1;
        }
    }

    pub fn autocomplete_accept(&mut self) {
        if let Some((command, _)) = self.get_autocomplete_matches().get(self.autocomplete_idx) {
            self.input = (*command).to_string();
            self.input_cursor = self.input.len();
            self.autocomplete_idx = 0;
        }
    }
}

pub fn draw(f: &mut Frame, state: &mut TuiState) {
    f.render_widget(Block::default().style(Style::default().bg(Palette::BG)), f.area());

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(5), Constraint::Min(0), Constraint::Length(2)])
        .split(f.area());

    draw_header(f, outer[0], state);
    draw_body(f, outer[1], state);
    draw_footer(f, outer[2], state);

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
            "BARQCODER // COMMAND DECK",
            Style::default().fg(Palette::BRAND).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "model={}  session={}",
                state.current_model,
                trim_chars(&state.session_id, 24)
            ),
            Style::default().fg(Palette::TEXT_DIM),
        ),
    ]);

    f.render_widget(Paragraph::new(headline), rows[0]);

    let mut chips = vec![
        chip(
            format!("focus {}", focus_label(state.focus)),
            Palette::BG,
            Palette::BRAND,
        ),
        Span::raw(" "),
        chip(status_label(state), status_color(state), Palette::PANEL_MUTED),
        Span::raw(" "),
        chip(
            format!("transcript {}", if state.chat_follow { "live" } else { "history" }),
            Palette::TEXT,
            Palette::PANEL_MUTED,
        ),
        Span::raw(" "),
        chip(
            format!("approvals {}", state.action_queue.len()),
            if state.action_queue.is_empty() {
                Palette::TEXT
            } else {
                Palette::WARNING
            },
            Palette::PANEL_MUTED,
        ),
        Span::raw(" "),
        chip(
            format!("messages {}", state.messages.len()),
            Palette::TEXT,
            Palette::PANEL_MUTED,
        ),
        Span::raw(" "),
        chip(
            format!("rail {}", if state.sidebar_visible { "open" } else { "hidden" }),
            Palette::TEXT,
            Palette::PANEL_MUTED,
        ),
    ];

    if let Some(message) = state.status_message.as_deref().filter(|message| !message.is_empty()) {
        chips.push(Span::raw("  "));
        chips.push(Span::styled(
            trim_chars(message, 72),
            Style::default().fg(if state.status_is_error {
                Palette::ERROR
            } else {
                Palette::TEXT_MUTED
            }),
        ));
    }

    f.render_widget(Paragraph::new(Line::from(chips)), rows[1]);

    let tabs = Tabs::new(vec![
        Line::from(" Console "),
        Line::from(" Patch Deck "),
        Line::from(" Session Log "),
        Line::from(format!(" Gate [{}] ", state.action_queue.len())),
    ])
    .select(state.active_tab as usize)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Palette::BORDER))
            .style(Style::default().bg(Palette::PANEL)),
    )
    .highlight_style(
        Style::default()
            .fg(Palette::TEXT)
            .bg(Palette::PANEL_ALT)
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
    let columns = if state.sidebar_visible {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(36)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(0)])
            .split(area)
    };

    let main = columns[0];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(12), Constraint::Length(8)])
        .split(main);

    draw_chat_history(f, rows[0], state);
    draw_input_box(f, rows[1], state);

    if state.sidebar_visible {
        draw_ops_rail(f, columns[1], state);
    }
}

fn draw_ops_rail(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),
            Constraint::Min(8),
            Constraint::Length(10),
            Constraint::Length(7),
        ])
        .split(area);

    let mut radar_lines = vec![
        Line::from(vec![
            Span::styled("Deck status", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(
                status_label(state),
                Style::default()
                    .fg(status_color(state))
                    .add_modifier(Modifier::BOLD),
            ),
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
                if state.chat_follow {
                    "live feed"
                } else {
                    "history mode"
                },
                Style::default().fg(Palette::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Current job", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(
                state.current_tool.as_deref().unwrap_or("idle"),
                Style::default().fg(Palette::TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Keys", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled("F1 focus  Alt+S rail  End live", Style::default().fg(Palette::KEY)),
        ]),
    ];

    if !state.action_queue.is_empty() {
        radar_lines.push(Line::from(vec![
            Span::styled("Gate", Style::default().fg(Palette::TEXT_DIM)),
            Span::raw("  "),
            Span::styled(
                format!("{} pending approvals", state.action_queue.len()),
                Style::default()
                    .fg(Palette::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    let radar = Paragraph::new(radar_lines)
        .block(panel_block("Session Radar", state.focus == Focus::Sidebar))
        .wrap(Wrap { trim: true });
    f.render_widget(radar, rows[0]);

    draw_tool_activity(f, rows[1], state);

    let items: Vec<ListItem> = if state.workspace_files.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "Workspace scan is still warming up.",
            Style::default().fg(Palette::TEXT_DIM),
        )))]
    } else {
        state
            .workspace_files
            .iter()
            .take(rows[2].height.saturating_sub(3) as usize)
            .map(|path| ListItem::new(Line::from(Span::styled(path.as_str(), Style::default().fg(Palette::TEXT)))))
            .collect()
    };

    let list = List::new(items)
        .block(panel_block("Working Set", state.focus == Focus::Sidebar))
        .highlight_style(
            Style::default()
                .bg(Palette::PANEL_ALT)
                .fg(Palette::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    f.render_stateful_widget(list, rows[2], &mut state.file_list_state);

    let context_lines = if state.barq_context.is_empty() {
        vec![Line::from(Span::styled(
            "Briefing notes will appear here when available.",
            Style::default().fg(Palette::TEXT_DIM),
        ))]
    } else {
        state
            .barq_context
            .iter()
            .rev()
            .take(4)
            .rev()
            .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Palette::TEXT))))
            .collect()
    };

    let context = Paragraph::new(context_lines)
        .block(panel_block("Briefing", false))
        .wrap(Wrap { trim: true });
    f.render_widget(context, rows[3]);
}

fn draw_chat_history(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let sticky_prompt = if !state.chat_follow {
        state.latest_user_prompt_preview()
    } else {
        None
    };

    let transcript_area = if let Some(prompt) = sticky_prompt {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(4), Constraint::Min(0)])
            .split(area);

        let ribbon = Paragraph::new(vec![
            Line::from(Span::styled(
                "Pinned Request",
                Style::default()
                    .fg(Palette::BRAND)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                prompt,
                Style::default().fg(Palette::TEXT_DIM),
            )),
        ])
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Palette::PANEL_MUTED))
                .style(Style::default().bg(Palette::PANEL_ALT))
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: true });
        f.render_widget(ribbon, rows[0]);
        rows[1]
    } else {
        area
    };

    let content_width = transcript_area.width.saturating_sub(5) as usize;
    let mut lines = build_transcript_lines(&state.messages, content_width.max(16));
    if state.is_thinking {
        push_message_card(
            &mut lines,
            &ChatMessage::system(format!("{} Working...", spinner_frame(state.tick))),
            content_width.max(16),
        );
    }

    let conversation_title = if state.chat_follow {
        "Command Deck"
    } else {
        "Command Deck / History"
    };

    state.chat_resolved_scroll = render_lines_panel(
        f,
        transcript_area,
        lines,
        &mut state.chat_scroll,
        panel_block(conversation_title, state.focus == Focus::Chat),
    );
}

fn draw_tool_activity(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let mut lines = Vec::new();

    if let Some(tool) = &state.current_tool {
        lines.push(Line::from(vec![
            Span::styled(
                "Active job",
                Style::default()
                    .fg(Palette::WARNING)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(tool.as_str(), Style::default().fg(Palette::TEXT)),
        ]));
        lines.push(Line::raw(""));
    }

    if state.tool_log.is_empty() {
        lines.push(Line::from(Span::styled(
            "Runs and tool activity will appear here.",
            Style::default().fg(Palette::TEXT_DIM),
        )));
    } else {
        for entry in &state.tool_log {
            for (idx, line) in wrap_message_body(entry, area.width.saturating_sub(6) as usize)
                .into_iter()
                .enumerate()
            {
                let prefix = if idx == 0 { ">> " } else { "   " };
                lines.push(Line::from(vec![
                    Span::styled(prefix, Style::default().fg(Palette::KEY)),
                    Span::styled(line, Style::default().fg(Palette::TEXT)),
                ]));
            }
            lines.push(Line::raw(""));
        }
    }

    let tool_title = if state.tool_follow {
        "Run Feed"
    } else {
        "Run Feed / History"
    };

    state.tool_resolved_scroll = render_lines_panel(
        f,
        area,
        lines,
        &mut state.tool_scroll,
        panel_block(tool_title, state.focus == Focus::ToolLog),
    );
}

fn draw_input_box(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let title = match state.status_message.as_deref() {
        Some(message) if !message.is_empty() => format!("Prompt Dock  {}", trim_chars(message, 60)),
        _ if !state.input.is_empty() => format!(
            "Prompt Dock  {} lines / {} chars",
            state.input.lines().count().max(1),
            state.input.chars().count()
        ),
        _ => "Prompt Dock".to_string(),
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
    .alignment(Alignment::Left)
    .wrap(Wrap { trim: false });

    f.render_widget(input, area);

    if state.is_autocomplete_active() {
        draw_autocomplete_popup(f, area, state);
    }
}

fn draw_autocomplete_popup(f: &mut Frame, area: Rect, state: &TuiState) {
    let matches = state.get_autocomplete_matches();
    let popup_height = (matches.len() as u16 + 2).min(8);
    let popup_width = area.width.min(44);
    let popup_area = Rect {
        x: area.x,
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
                    .fg(Palette::TEXT)
                    .bg(Palette::PANEL_MUTED)
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
        .block(
            Block::default()
                .title("Commands")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Palette::BORDER_ACTIVE))
                .style(Style::default().bg(Palette::PANEL_ALT))
                .padding(Padding::horizontal(1)),
        )
        .wrap(Wrap { trim: true });

    f.render_widget(Clear, popup_area);
    f.render_widget(popup, popup_area);
}

fn draw_diff_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.diff_content.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::from(Span::styled(
                "No diff is loaded yet.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "Run an edit or file mutation and the latest patch will land here.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ])
        .block(panel_block("Patch Deck", false))
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });
        f.render_widget(placeholder, area);
        return;
    }

    let lines: Vec<Line> = state
        .diff_content
        .iter()
        .map(|line| {
            let style = if line.starts_with('+') {
                Style::default().fg(Palette::DIFF_ADD)
            } else if line.starts_with('-') {
                Style::default().fg(Palette::DIFF_DEL)
            } else if line.starts_with("@@") {
                Style::default().fg(Palette::DIFF_HUNK).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Palette::TEXT)
            };
            Line::from(Span::styled(line.as_str(), style))
        })
        .collect();

    render_lines_panel(
        f,
        area,
        lines,
        &mut state.diff_scroll,
        panel_block("Patch Deck", false),
    );
}

fn draw_sessions_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.sessions.is_empty() {
        let placeholder = Paragraph::new("No archived sessions found.")
            .block(panel_block("Session Log", false))
            .alignment(Alignment::Center);
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
                    trim_chars(&session.id, 42),
                    Style::default().fg(Palette::TEXT).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    format!("{} events", session.event_count),
                    Style::default().fg(Palette::TEXT_DIM),
                )),
                Line::from(Span::styled(
                    trim_chars(&session.workspace, 42),
                    Style::default().fg(Palette::TEXT_MUTED),
                )),
                Line::raw(""),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(panel_block("Session Archive", false))
        .highlight_style(
            Style::default()
                .bg(Palette::PANEL_ALT)
                .fg(Palette::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    f.render_stateful_widget(list, columns[0], &mut state.session_list_state);

    let selected = state
        .session_list_state
        .selected()
        .and_then(|idx| state.sessions.get(idx));

    let details = if let Some(session) = selected {
        vec![
            Line::from(vec![
                Span::styled("Session ", Style::default().fg(Palette::TEXT_DIM)),
                Span::styled(session.id.as_str(), Style::default().fg(Palette::TEXT).add_modifier(Modifier::BOLD)),
            ]),
            Line::raw(""),
            Line::from(format!("Workspace: {}", session.workspace)),
            Line::from(format!("Created: {}", session.created_at)),
            Line::from(format!("Events: {}", session.event_count)),
            Line::raw(""),
            Line::from(Span::styled(
                "Press Enter to replay this session into the console.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ]
    } else {
        vec![Line::from("Select a session to inspect it.")]
    };

    let detail_panel = Paragraph::new(details)
        .block(panel_block("Replay Brief", false))
        .wrap(Wrap { trim: true });
    f.render_widget(detail_panel, columns[1]);
}

fn draw_action_queue_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.action_queue.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::from(Span::styled(
                "Gate is clear.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "When a tool needs approval it will appear here.",
                Style::default().fg(Palette::TEXT_MUTED),
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
        .constraints([Constraint::Percentage(36), Constraint::Percentage(64)])
        .split(area);

    let items: Vec<ListItem> = state
        .action_queue
        .iter()
        .map(|action| {
            let status = match action.approved {
                Some(true) => "approved",
                Some(false) => "denied",
                None => "pending",
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    action.label(),
                    Style::default().fg(Palette::TEXT).add_modifier(Modifier::BOLD),
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
        .block(panel_block("Gate Queue", false))
        .highlight_style(
            Style::default()
                .bg(Palette::PANEL_ALT)
                .fg(Palette::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    let mut list_state = ListState::default();
    list_state.select(Some(state.action_queue_selected.min(state.action_queue.len() - 1)));
    f.render_stateful_widget(list, columns[0], &mut list_state);

    let selected = &state.action_queue[state.action_queue_selected.min(state.action_queue.len() - 1)];
    let mut preview_lines = vec![
        Line::from(Span::styled(
            "Y approve once. A trust this tool for the run. N deny. Esc denies all pending requests.",
            Style::default().fg(Palette::WARNING),
        )),
        Line::raw(""),
    ];

    match &selected.kind {
        ActionKind::WriteFile { path, full_content, .. } => {
            preview_lines.push(Line::from(format!("Target: {}", path)));
            preview_lines.push(Line::from(format!(
                "Buffered content size: {} chars",
                full_content.chars().count()
            )));
            preview_lines.push(Line::raw(""));
        }
        ActionKind::ShellCommand { command, .. } => {
            preview_lines.push(Line::from(format!("Command: {}", command)));
            preview_lines.push(Line::raw(""));
        }
        ActionKind::ApplyVerifiedPatch { step_id, .. } => {
            preview_lines.push(Line::from(format!("Verification step: {}", step_id)));
            preview_lines.push(Line::raw(""));
        }
    }

    for line in selected.preview().lines() {
        let style = if line.starts_with('+') {
            Style::default().fg(Palette::DIFF_ADD)
        } else if line.starts_with('-') {
            Style::default().fg(Palette::DIFF_DEL)
        } else if line.starts_with("@@") {
            Style::default().fg(Palette::DIFF_HUNK)
        } else {
            Style::default().fg(Palette::TEXT)
        };
        preview_lines.push(Line::from(Span::styled(line.to_string(), style)));
    }

    render_lines_panel(
        f,
        columns[1],
        preview_lines,
        &mut state.action_preview_scroll,
        panel_block("Decision Preview", false),
    );
}

fn draw_footer(f: &mut Frame, area: Rect, state: &TuiState) {
    let content = match state.active_tab {
        ActiveTab::Chat => vec![
            Line::from(Span::styled(
                "Prompt dock: type or paste a request, Enter sends, paste keeps multiline prompts intact.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
            Line::from(Span::styled(
                "Navigation: F1 moves focus, PgUp/PgDn scroll the focused pane, End snaps transcript/feed back to live, Alt+S hides the rail, Esc quits.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ],
        ActiveTab::Diff => vec![
            Line::from(Span::styled(
                "Patch deck: Up/Down scroll, PgUp/PgDn fast scroll, Home/End jump.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
            Line::from(Span::styled(
                "Tab switches decks. Esc quits.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ],
        ActiveTab::Sessions => vec![
            Line::from(Span::styled(
                "Session log: Up/Down selects an archive entry, Enter replays it into the console.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
            Line::from(Span::styled(
                "Tab switches decks. Esc quits.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ],
        ActiveTab::ActionQueue => vec![
            Line::from(Span::styled(
                "Approval gate: Up/Down selects, PgUp/PgDn scrolls the preview, Y approves once, A trusts the tool for this run, N denies.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
            Line::from(Span::styled(
                "Esc denies all pending requests or quits when the gate is empty.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ],
    };

    let footer = Paragraph::new(content)
        .alignment(Alignment::Left)
        .style(Style::default().bg(Palette::BG));
    f.render_widget(footer, area);
}

fn draw_permission_prompt(f: &mut Frame, prompt: &PermissionPrompt) {
    let area = centered_rect(72, 11, f.area());
    let widget = Paragraph::new(vec![
        Line::from(Span::styled(
            prompt.title.as_str(),
            Style::default().fg(Palette::TEXT).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(prompt.reason.as_str(), Style::default().fg(Palette::TEXT))),
        Line::raw(""),
        Line::from(Span::styled(
            prompt.hint.as_str(),
            Style::default().fg(Palette::WARNING).add_modifier(Modifier::BOLD),
        )),
        Line::raw(""),
        Line::from(Span::styled(
            format!("Queued approvals: {}", prompt.queue_len),
            Style::default().fg(Palette::TEXT_DIM),
        )),
    ])
    .block(
        Block::default()
            .title(" Action Gate ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Palette::BORDER_ACTIVE))
            .style(Style::default().bg(Palette::PANEL_ALT))
            .padding(Padding::horizontal(1)),
    )
    .wrap(Wrap { trim: true });

    f.render_widget(Clear, area);
    f.render_widget(widget, area);
}

fn panel_block(title: &str, focused: bool) -> Block<'_> {
    Block::default()
        .title(Span::styled(
            format!(" {} ", title),
            Style::default()
                .fg(if focused { Palette::TEXT } else { Palette::TEXT_DIM })
                .add_modifier(Modifier::BOLD),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { Palette::BORDER_ACTIVE } else { Palette::BORDER }))
        .style(Style::default().bg(Palette::PANEL))
        .padding(Padding::horizontal(1))
}

fn render_lines_panel(
    f: &mut Frame,
    area: Rect,
    lines: Vec<Line>,
    scroll_state: &mut usize,
    block: Block<'_>,
) -> usize {
    let total = lines.len();
    let visible = area.height.saturating_sub(2) as usize;
    let scroll = resolve_scroll(*scroll_state, total, visible);
    *scroll_state = scroll;

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .scroll((scroll as u16, 0))
        .wrap(Wrap { trim: false });
    f.render_widget(paragraph, area);

    if total > visible && visible > 0 {
        let mut scrollbar_state = ScrollbarState::new(total.saturating_sub(visible)).position(scroll);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }

    scroll
}

fn resolve_scroll(requested: usize, total_lines: usize, visible_lines: usize) -> usize {
    if total_lines <= visible_lines {
        return 0;
    }

    let bottom = total_lines.saturating_sub(visible_lines);
    if requested == usize::MAX {
        bottom
    } else {
        requested.min(bottom)
    }
}

fn build_transcript_lines(messages: &[ChatMessage], max_width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for message in messages {
        push_message_card(&mut lines, message, max_width);
    }
    lines
}

fn push_message_card(lines: &mut Vec<Line<'static>>, message: &ChatMessage, max_width: usize) {
    let (marker, title, accent, body_color) = match message.kind {
        MessageKind::User => (">>", "Request", Palette::USER, Palette::TEXT),
        MessageKind::Agent => ("<<", "Response", Palette::AGENT, Palette::TEXT),
        MessageKind::ToolCall => ("++", "Tool Call", Palette::TOOL, Palette::TEXT),
        MessageKind::ToolResult => ("==", "Tool Output", Palette::RESULT, Palette::TEXT),
        MessageKind::System => ("--", "Status", Palette::TEXT_DIM, Palette::TEXT_DIM),
        MessageKind::Error => ("!!", "Failure", Palette::ERROR, Palette::ERROR),
    };

    lines.push(Line::from(vec![
        Span::styled(
            format!("{} {}", marker, title),
            Style::default().fg(accent).add_modifier(Modifier::BOLD),
        ),
    ]));

    for body_line in wrap_message_body(&message.content, max_width.saturating_sub(2).max(1)) {
        lines.push(Line::from(vec![
            Span::styled(": ", Style::default().fg(accent)),
            Span::styled(body_line, Style::default().fg(body_color)),
        ]));
    }

    lines.push(Line::raw(""));
}

fn wrap_message_body(text: &str, max_width: usize) -> Vec<String> {
    let mut wrapped = Vec::new();
    let width = max_width.max(1);

    for logical_line in text.lines() {
        if logical_line.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        wrapped.extend(wrap_visual_line(logical_line, width));
    }

    if wrapped.is_empty() {
        wrapped.push(String::new());
    }

    wrapped
}

fn trim_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }

    let mut trimmed = String::new();
    for ch in text.chars().take(max_chars.saturating_sub(1)) {
        trimmed.push(ch);
    }
    trimmed.push('…');
    trimmed
}

fn preview_multiline(text: &str, max_lines: usize, max_chars: usize) -> String {
    let mut preview = String::new();
    let mut used_lines = 0usize;

    for line in text.lines() {
        if used_lines == max_lines || preview.chars().count() >= max_chars {
            break;
        }
        if !preview.is_empty() {
            preview.push(' ');
        }
        preview.push_str(line.trim());
        used_lines += 1;
    }

    let collapsed = preview.split_whitespace().collect::<Vec<_>>().join(" ");
    trim_chars(&collapsed, max_chars)
}

fn apply_scroll_delta(base: usize, delta: isize) -> usize {
    if delta.is_negative() {
        base.saturating_sub(delta.unsigned_abs())
    } else {
        base.saturating_add(delta as usize)
    }
}

fn build_composer_lines(
    input: &str,
    cursor: usize,
    max_width: usize,
    max_lines: usize,
) -> Vec<Line<'static>> {
    let content_width = max_width.saturating_sub(2).max(1);
    let mut with_cursor = input.to_string();
    if cursor <= with_cursor.len() {
        with_cursor.insert(cursor, '|');
    } else {
        with_cursor.push('|');
    }

    if with_cursor.is_empty() {
        with_cursor.push('|');
    }

    let mut wrapped = Vec::new();
    for logical_line in with_cursor.split('\n') {
        wrapped.extend(wrap_visual_line(logical_line, content_width));
    }

    if wrapped.is_empty() {
        wrapped.push(String::from("|"));
    }

    let visible_lines = max_lines.max(1);
    let start = wrapped.len().saturating_sub(visible_lines);

    wrapped[start..]
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            let prefix = if start == 0 && idx == 0 {
                "> "
            } else if idx == 0 {
                "... "
            } else {
                "    "
            };
            Line::from(Span::styled(
                format!("{}{}", prefix, line),
                Style::default().fg(Palette::TEXT),
            ))
        })
        .collect()
}

fn wrap_visual_line(line: &str, max_width: usize) -> Vec<String> {
    let width = max_width.max(1);
    let mut current = String::new();
    let mut current_width = 0usize;
    let mut wrapped = Vec::new();

    if line.is_empty() {
        wrapped.push(String::new());
        return wrapped;
    }

    for ch in line.chars() {
        if current_width == width {
            wrapped.push(current);
            current = String::new();
            current_width = 0;
        }
        current.push(ch);
        current_width += 1;
    }

    wrapped.push(current);
    wrapped
}

fn status_label(state: &TuiState) -> String {
    if state.is_indexing {
        format!("{} indexing workspace", spinner_frame(state.tick))
    } else if state.is_thinking {
        match &state.current_tool {
            Some(tool) => format!("{} running {}", spinner_frame(state.tick), tool),
            None => format!("{} drafting response", spinner_frame(state.tick)),
        }
    } else if !state.action_queue.is_empty() {
        format!("{} gate waiting", state.action_queue.len())
    } else {
        "deck ready".to_string()
    }
}

fn focus_label(focus: Focus) -> &'static str {
    match focus {
        Focus::Input => "prompt dock",
        Focus::Sidebar => "ops rail",
        Focus::Chat => "command deck",
        Focus::ToolLog => "run feed",
    }
}

fn chip(label: impl Into<String>, fg: Color, bg: Color) -> Span<'static> {
    Span::styled(
        format!(" {} ", label.into()),
        Style::default()
            .fg(fg)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    )
}

#[cfg(test)]
mod tests {
    use super::{apply_scroll_delta, build_composer_lines, build_transcript_lines, ChatMessage, Focus, TuiState};

    #[test]
    fn input_insert_str_normalizes_pasted_newlines() {
        let mut state = TuiState::new(4096, "model".to_string(), "session".to_string());
        state.input_insert_str("alpha\r\nbeta\rgamma");

        assert_eq!(state.input, "alpha\nbeta\ngamma");
        assert_eq!(state.input_cursor, state.input.len());
    }

    #[test]
    fn cycle_focus_reaches_chat_and_tool_panels() {
        let mut state = TuiState::new(4096, "model".to_string(), "session".to_string());

        state.cycle_focus();
        assert_eq!(state.focus, Focus::Chat);
        state.cycle_focus();
        assert_eq!(state.focus, Focus::ToolLog);
        state.cycle_focus();
        assert_eq!(state.focus, Focus::Sidebar);
        state.cycle_focus();
        assert_eq!(state.focus, Focus::Input);
    }

    #[test]
    fn composer_lines_keep_tail_of_large_prompt_visible() {
        let prompt = "line1\nline2\nline3\nline4\nline5";
        let lines = build_composer_lines(prompt, prompt.len(), 32, 3);
        let rendered: Vec<String> = lines.into_iter().map(|line| line.to_string()).collect();

        assert_eq!(rendered.len(), 3);
        assert!(rendered[0].starts_with("... "));
        assert!(rendered.iter().any(|line| line.contains("line5|")));
    }

    #[test]
    fn scroll_chat_exits_follow_mode_from_last_resolved_position() {
        let mut state = TuiState::new(4096, "model".to_string(), "session".to_string());
        state.chat_follow = true;
        state.chat_resolved_scroll = 18;

        state.scroll_chat_by(-2);

        assert!(!state.chat_follow);
        assert_eq!(state.chat_scroll, 16);
    }

    #[test]
    fn add_message_preserves_manual_chat_position() {
        let mut state = TuiState::new(4096, "model".to_string(), "session".to_string());
        state.chat_follow = false;
        state.chat_scroll = 7;

        state.add_message(super::ChatMessage::agent("new reply"));

        assert_eq!(state.chat_scroll, 7);
    }

    #[test]
    fn latest_user_prompt_preview_uses_most_recent_user_message() {
        let mut state = TuiState::new(4096, "model".to_string(), "session".to_string());
        state.add_message(super::ChatMessage::user("first prompt"));
        state.add_message(super::ChatMessage::agent("answer"));
        state.add_message(super::ChatMessage::user("second prompt\nwith detail"));

        assert_eq!(
            state.latest_user_prompt_preview().as_deref(),
            Some("second prompt with detail")
        );
    }

    #[test]
    fn apply_scroll_delta_saturates_at_zero() {
        assert_eq!(apply_scroll_delta(2, -5), 0);
        assert_eq!(apply_scroll_delta(2, 5), 7);
    }

    #[test]
    fn transcript_lines_render_message_cards_instead_of_flat_tags() {
        let lines = build_transcript_lines(
            &[
                ChatMessage::user("Fix the failing tests"),
                ChatMessage::tool_call("Run shell command\n  command: cargo test"),
            ],
            48,
        );
        let rendered: Vec<String> = lines.into_iter().map(|line| line.to_string()).collect();

        assert!(rendered.iter().any(|line| line.contains(">> Request")));
        assert!(rendered.iter().any(|line| line.contains(": Fix the failing tests")));
        assert!(rendered.iter().any(|line| line.contains("++ Tool Call")));
        assert!(rendered.iter().any(|line| line.contains(": Run shell command")));
    }
}

fn status_color(state: &TuiState) -> Color {
    if state.status_is_error {
        Palette::ERROR
    } else if !state.action_queue.is_empty() {
        Palette::WARNING
    } else if state.is_thinking || state.is_indexing {
        Palette::BRAND
    } else {
        Palette::SUCCESS
    }
}

fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let popup_width = area.width.saturating_mul(percent_x).saturating_div(100);
    let popup_height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(popup_width) / 2,
        y: area.y + area.height.saturating_sub(popup_height) / 2,
        width: popup_width.max(1),
        height: popup_height.max(1),
    }
}
