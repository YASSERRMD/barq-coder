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
    pub const BG: Color = Color::Rgb(14, 18, 24);
    pub const PANEL: Color = Color::Rgb(23, 28, 36);
    pub const PANEL_ALT: Color = Color::Rgb(29, 35, 44);
    pub const PANEL_MUTED: Color = Color::Rgb(33, 41, 52);
    pub const BORDER: Color = Color::Rgb(74, 86, 102);
    pub const BORDER_ACTIVE: Color = Color::Rgb(94, 164, 255);
    pub const TEXT: Color = Color::Rgb(229, 235, 242);
    pub const TEXT_DIM: Color = Color::Rgb(150, 161, 174);
    pub const TEXT_MUTED: Color = Color::Rgb(117, 128, 141);
    pub const BRAND: Color = Color::Rgb(94, 164, 255);
    pub const USER: Color = Color::Rgb(92, 214, 163);
    pub const AGENT: Color = Color::Rgb(255, 219, 102);
    pub const TOOL: Color = Color::Rgb(147, 197, 253);
    pub const RESULT: Color = Color::Rgb(244, 162, 97);
    pub const ERROR: Color = Color::Rgb(255, 107, 107);
    pub const SUCCESS: Color = Color::Rgb(80, 200, 120);
    pub const WARNING: Color = Color::Rgb(255, 201, 107);
    pub const DIFF_ADD: Color = Color::Rgb(109, 208, 130);
    pub const DIFF_DEL: Color = Color::Rgb(255, 120, 120);
    pub const DIFF_HUNK: Color = Color::Rgb(132, 196, 255);
    pub const KEY: Color = Color::Rgb(110, 123, 140);
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
                "Welcome to BarqCoder. Type a prompt or /help to see commands.",
            )],
            chat_scroll: usize::MAX,
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
        self.chat_scroll = usize::MAX;
    }

    pub fn append_agent_token(&mut self, token: &str) {
        match self.messages.last_mut() {
            Some(last) if matches!(last.kind, MessageKind::Agent) => last.content.push_str(token),
            _ => self.messages.push(ChatMessage::agent(token)),
        }
        self.chat_scroll = usize::MAX;
    }

    pub fn set_diff(&mut self, patch: &str) {
        self.diff_content = patch.lines().map(|line| line.to_string()).collect();
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
        .constraints([Constraint::Length(4), Constraint::Min(0), Constraint::Length(1)])
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
        .constraints([Constraint::Length(1), Constraint::Length(3)])
        .split(area);

    let headline = Line::from(vec![
        Span::styled("BarqCoder", Style::default().fg(Palette::BRAND).add_modifier(Modifier::BOLD)),
        Span::styled(
            format!("  model={}  session={}  ", state.current_model, trim_chars(&state.session_id, 24)),
            Style::default().fg(Palette::TEXT_DIM),
        ),
        Span::styled(status_label(state), Style::default().fg(status_color(state)).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(
            state.status_message.as_deref().unwrap_or(""),
            Style::default().fg(if state.status_is_error { Palette::ERROR } else { Palette::TEXT_MUTED }),
        ),
    ]);

    f.render_widget(Paragraph::new(headline), rows[0]);

    let tabs = Tabs::new(vec![
        Line::from(" Chat "),
        Line::from(" Diff "),
        Line::from(" Sessions "),
        Line::from(format!(" Approvals [{}] ", state.action_queue.len())),
    ])
    .select(state.active_tab as usize)
    .block(
        Block::default()
            .borders(Borders::TOP | Borders::BOTTOM)
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

    f.render_widget(tabs, rows[1]);
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
            .constraints([Constraint::Length(30), Constraint::Min(0)])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(0), Constraint::Min(0)])
            .split(area)
    };

    if state.sidebar_visible {
        draw_sidebar(f, columns[0], state);
    }

    let main = columns[1];
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(7), Constraint::Length(3)])
        .split(main);

    draw_chat_history(f, rows[0], state);
    draw_tool_activity(f, rows[1], state);
    draw_input_box(f, rows[2], state);
}

fn draw_sidebar(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(0), Constraint::Length(6)])
        .split(area);

    let help_lines = vec![
        Line::from(vec![
            Span::styled("Enter", Style::default().fg(Palette::KEY).add_modifier(Modifier::BOLD)),
            Span::styled(" send prompt", Style::default().fg(Palette::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("Tab", Style::default().fg(Palette::KEY).add_modifier(Modifier::BOLD)),
            Span::styled(" switch tabs", Style::default().fg(Palette::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("Alt+S", Style::default().fg(Palette::KEY).add_modifier(Modifier::BOLD)),
            Span::styled(" toggle sidebar", Style::default().fg(Palette::TEXT)),
        ]),
        Line::from(vec![
            Span::styled("F1", Style::default().fg(Palette::KEY).add_modifier(Modifier::BOLD)),
            Span::styled(" cycle focus", Style::default().fg(Palette::TEXT)),
        ]),
    ];

    let help = Paragraph::new(help_lines)
        .block(panel_block("Quick Help", state.focus == Focus::Sidebar))
        .wrap(Wrap { trim: true });
    f.render_widget(help, rows[0]);

    let items: Vec<ListItem> = if state.workspace_files.is_empty() {
        vec![ListItem::new(Line::from(Span::styled(
            "No workspace files indexed yet.",
            Style::default().fg(Palette::TEXT_DIM),
        )))]
    } else {
        state
            .workspace_files
            .iter()
            .map(|path| ListItem::new(Line::from(Span::styled(path.as_str(), Style::default().fg(Palette::TEXT)))))
            .collect()
    };

    let list = List::new(items)
        .block(panel_block("Workspace", state.focus == Focus::Sidebar))
        .highlight_style(
            Style::default()
                .bg(Palette::PANEL_ALT)
                .fg(Palette::TEXT)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    f.render_stateful_widget(list, rows[1], &mut state.file_list_state);

    let context_lines = if state.barq_context.is_empty() {
        vec![Line::from(Span::styled(
            "BARQ context will appear here when available.",
            Style::default().fg(Palette::TEXT_DIM),
        ))]
    } else {
        state
            .barq_context
            .iter()
            .rev()
            .take(3)
            .rev()
            .map(|line| Line::from(Span::styled(line.as_str(), Style::default().fg(Palette::TEXT))))
            .collect()
    };

    let context = Paragraph::new(context_lines)
        .block(panel_block("Context", false))
        .wrap(Wrap { trim: true });
    f.render_widget(context, rows[2]);
}

fn draw_chat_history(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let mut lines = Vec::new();

    for message in &state.messages {
        let (label, label_color, body_color) = match message.kind {
            MessageKind::User => ("You", Palette::USER, Palette::TEXT),
            MessageKind::Agent => ("Barq", Palette::AGENT, Palette::TEXT),
            MessageKind::ToolCall => ("Tool", Palette::TOOL, Palette::TOOL),
            MessageKind::ToolResult => ("Result", Palette::RESULT, Palette::RESULT),
            MessageKind::System => ("Note", Palette::TEXT_DIM, Palette::TEXT_DIM),
            MessageKind::Error => ("Error", Palette::ERROR, Palette::ERROR),
        };

        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", label),
                Style::default().fg(label_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                trim_chars(message.content.lines().next().unwrap_or(""), 120),
                Style::default().fg(body_color),
            ),
        ]));

        for line in message.content.lines().skip(1) {
            lines.push(Line::from(Span::styled(
                format!("      {}", line),
                Style::default().fg(body_color),
            )));
        }
        lines.push(Line::raw(""));
    }

    if state.is_thinking {
        lines.push(Line::from(vec![
            Span::styled(
                format!("[{}] ", spinner_frame(state.tick)),
                Style::default().fg(Palette::BRAND).add_modifier(Modifier::BOLD),
            ),
            Span::styled("Working...", Style::default().fg(Palette::TEXT_DIM)),
        ]));
    }

    render_lines_panel(
        f,
        area,
        lines,
        &mut state.chat_scroll,
        panel_block("Conversation", state.focus == Focus::Chat),
    );
}

fn draw_tool_activity(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let mut lines = Vec::new();

    if let Some(tool) = &state.current_tool {
        lines.push(Line::from(vec![
            Span::styled("Active ", Style::default().fg(Palette::WARNING).add_modifier(Modifier::BOLD)),
            Span::styled(tool.as_str(), Style::default().fg(Palette::TEXT)),
        ]));
        lines.push(Line::raw(""));
    }

    if state.tool_log.is_empty() {
        lines.push(Line::from(Span::styled(
            "Tool activity will appear here.",
            Style::default().fg(Palette::TEXT_DIM),
        )));
    } else {
        for entry in &state.tool_log {
            lines.push(Line::from(Span::styled(entry.as_str(), Style::default().fg(Palette::TEXT))));
        }
    }

    render_lines_panel(
        f,
        area,
        lines,
        &mut state.tool_scroll,
        panel_block("Tool Activity", state.focus == Focus::ToolLog),
    );
}

fn draw_input_box(f: &mut Frame, area: Rect, state: &mut TuiState) {
    let before = &state.input[..state.input_cursor];
    let after = &state.input[state.input_cursor..];
    let cursor_width = after.chars().next().map(|ch| ch.len_utf8()).unwrap_or(0);
    let cursor = if cursor_width == 0 { " " } else { &after[..cursor_width] };
    let after_rest = if cursor_width == 0 { "" } else { &after[cursor_width..] };

    let title = match state.status_message.as_deref() {
        Some(message) if !message.is_empty() => format!("Composer  {}", trim_chars(message, 60)),
        _ => "Composer".to_string(),
    };

    let input = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::default().fg(Palette::BRAND).add_modifier(Modifier::BOLD)),
        Span::styled(before, Style::default().fg(Palette::TEXT)),
        Span::styled(
            cursor,
            Style::default()
                .fg(Palette::PANEL)
                .bg(Palette::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(after_rest, Style::default().fg(Palette::TEXT)),
    ]))
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
                "Run an edit or file mutation and the latest patch will appear here.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ])
        .block(panel_block("Diff", false))
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

    render_lines_panel(f, area, lines, &mut state.diff_scroll, panel_block("Diff", false));
}

fn draw_sessions_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.sessions.is_empty() {
        let placeholder = Paragraph::new("No saved sessions found.")
            .block(panel_block("Sessions", false))
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
        .block(panel_block("Saved Sessions", false))
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
                "Press Enter to replay this session into the chat tab.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ]
    } else {
        vec![Line::from("Select a session to inspect it.")]
    };

    let detail_panel = Paragraph::new(details)
        .block(panel_block("Session Details", false))
        .wrap(Wrap { trim: true });
    f.render_widget(detail_panel, columns[1]);
}

fn draw_action_queue_tab(f: &mut Frame, area: Rect, state: &mut TuiState) {
    if state.action_queue.is_empty() {
        let placeholder = Paragraph::new(vec![
            Line::from(Span::styled(
                "No pending approvals.",
                Style::default().fg(Palette::TEXT_DIM),
            )),
            Line::raw(""),
            Line::from(Span::styled(
                "When a tool needs approval it will appear here.",
                Style::default().fg(Palette::TEXT_MUTED),
            )),
        ])
        .block(panel_block("Approvals", false))
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
        .block(panel_block("Pending Approvals", false))
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
            "Approve with Y, deny with N. Esc denies all pending requests.",
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
        panel_block("Approval Preview", false),
    );
}

fn draw_footer(f: &mut Frame, area: Rect, state: &TuiState) {
    let content = match state.active_tab {
        ActiveTab::Chat => "Enter send  Up/Down history  F1 focus  Tab next tab  Shift+Tab prev tab  Alt+S toggle sidebar  Esc quit",
        ActiveTab::Diff => "Up/Down scroll  PageUp/PageDown fast scroll  Home/End jump  Tab switch tabs  Esc quit",
        ActiveTab::Sessions => "Up/Down select  Enter replay session  Tab switch tabs  Esc quit",
        ActiveTab::ActionQueue => "Up/Down select  PageUp/PageDown preview scroll  Y approve once  A allow this tool  N deny  Esc deny all or quit",
    };

    let footer = Paragraph::new(Span::styled(content, Style::default().fg(Palette::TEXT_DIM)))
        .alignment(Alignment::Left)
        .style(Style::default().bg(Palette::BG));
    f.render_widget(footer, area);
}

fn draw_permission_prompt(f: &mut Frame, prompt: &PermissionPrompt) {
    let area = centered_rect(68, 10, f.area());
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
            .title(" Permission Required ")
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
) {
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

fn status_label(state: &TuiState) -> String {
    if state.is_indexing {
        format!("{} indexing", spinner_frame(state.tick))
    } else if state.is_thinking {
        match &state.current_tool {
            Some(tool) => format!("{} running {}", spinner_frame(state.tick), tool),
            None => format!("{} thinking", spinner_frame(state.tick)),
        }
    } else if !state.action_queue.is_empty() {
        format!("{} approval waiting", state.action_queue.len())
    } else {
        "ready".to_string()
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
