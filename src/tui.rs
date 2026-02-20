use ratatui::{
    prelude::*,
    widgets::*,
};

pub struct TuiComponents;

impl TuiComponents {
    pub fn render_spinner<'a>(tick: usize) -> Span<'a> {
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let frame = frames[tick % frames.len()];
        Span::styled(frame, Style::default().fg(Color::Cyan))
    }

    pub fn render_token_count<'a>(count: usize, max: usize) -> Span<'a> {
        let color = if count > max {
            Color::Red
        } else if count > (max * 8 / 10) {
            Color::Yellow
        } else {
            Color::Green
        };
        Span::styled(format!(" [Tokens: {}/{}] ", count, max), Style::default().fg(color))
    }

    pub fn render_diff<'a>(patch: &'a str) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        for line in patch.lines() {
            let style = if line.starts_with('+') {
                Style::default().fg(Color::Green)
            } else if line.starts_with('-') {
                Style::default().fg(Color::Red)
            } else if line.starts_with("@@") {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default()
            };
            lines.push(Line::from(Span::styled(line.to_string(), style)));
        }
        lines
    }

    pub fn draw_tree(f: &mut Frame, area: Rect, items: &[String], selected: usize) {
        let ui_items: Vec<ListItem> = items.iter().enumerate().map(|(i, s)| {
            let style = if i == selected {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            };
            ListItem::new(format!("📄 {}", s)).style(style)
        }).collect();

        let list = List::new(ui_items)
            .block(Block::default().borders(Borders::ALL).title(" Workspace Tree "));
            
        f.render_widget(list, area);
    }
}
