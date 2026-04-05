//! Lightweight markdown-to-Span renderer for Ratatui.
//!
//! Parses: headers, bold, italic, inline code, fenced code blocks with
//! syntax highlighting, lists, blockquotes, horizontal rules.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

use crate::tui::Palette;

struct HighlightState {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl HighlightState {
    fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }
}

thread_local! {
    static HL: HighlightState = HighlightState::new();
}

/// Convert markdown text into styled ratatui Lines.
pub fn render_markdown(text: &str, max_width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buffer: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        if raw_line.trim_start().starts_with("```") {
            if in_code_block {
                lines.extend(highlight_code_block(&code_buffer, &code_lang, max_width));
                code_buffer.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = raw_line.trim_start().trim_start_matches('`').trim().to_string();
                let label = if code_lang.is_empty() { "code" } else { &code_lang };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {} ", label),
                        Style::default().fg(Palette::BG).bg(Palette::ACCENT2).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " ".repeat(max_width.saturating_sub(label.len() + 3)),
                        Style::default().bg(Color::Rgb(30, 33, 48)),
                    ),
                ]));
            }
            continue;
        }
        if in_code_block { code_buffer.push(raw_line.to_string()); continue; }

        let trimmed = raw_line.trim_start();

        // Horizontal rule
        if trimmed.len() >= 3
            && (trimmed.chars().all(|c| c == '-' || c == ' ')
                || trimmed.chars().all(|c| c == '*' || c == ' ')
                || trimmed.chars().all(|c| c == '_' || c == ' '))
            && trimmed.chars().filter(|c| !c.is_whitespace()).count() >= 3
        {
            lines.push(Line::from(Span::styled(
                "\u{2500}".repeat(max_width.min(60)),
                Style::default().fg(Palette::BORDER),
            )));
            continue;
        }

        // Headers
        if let Some(rest) = trimmed.strip_prefix("### ") {
            lines.push(Line::from(Span::styled(format!("   {}", rest),
                Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD))));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(format!("  {}", rest),
                Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(format!(" {}", rest),
                Style::default().fg(Palette::TEXT_BRIGHT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))));
            continue;
        }

        // Blockquote
        if let Some(rest) = trimmed.strip_prefix("> ") {
            lines.push(Line::from(vec![
                Span::styled(" | ", Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(rest.to_string(), Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::ITALIC)),
            ]));
            continue;
        }

        // Unordered list
        if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            let indent = raw_line.len() - trimmed.len();
            let bullet = if indent > 0 { "\u{25E6}" } else { "\u{2022}" };
            let mut spans = vec![Span::styled(
                format!("{}  {} ", " ".repeat(indent), bullet),
                Style::default().fg(Palette::ACCENT2),
            )];
            spans.extend(parse_inline(rest));
            lines.push(Line::from(spans));
            continue;
        }

        // Ordered list
        if let Some(dot_pos) = trimmed.find(". ") {
            let num = &trimmed[..dot_pos];
            if num.chars().all(|c| c.is_ascii_digit()) && !num.is_empty() {
                let rest = &trimmed[dot_pos + 2..];
                let indent = raw_line.len() - trimmed.len();
                let mut spans = vec![Span::styled(
                    format!("{}  {}. ", " ".repeat(indent), num),
                    Style::default().fg(Palette::ACCENT2).add_modifier(Modifier::BOLD),
                )];
                spans.extend(parse_inline(rest));
                lines.push(Line::from(spans));
                continue;
            }
        }

        if trimmed.is_empty() { lines.push(Line::raw("")); continue; }

        lines.push(Line::from(parse_inline(raw_line)));
    }

    if in_code_block && !code_buffer.is_empty() {
        lines.extend(highlight_code_block(&code_buffer, &code_lang, max_width));
    }
    lines
}

/// Parse inline markdown: **bold**, *italic*, `code`, ~~strike~~
fn parse_inline(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut buf = String::new();
    let base = Style::default().fg(Palette::TEXT);

    while let Some((_i, ch)) = chars.next() {
        match ch {
            '`' => {
                if !buf.is_empty() { spans.push(Span::styled(buf.clone(), base)); buf.clear(); }
                let mut code = String::new();
                let mut closed = false;
                for (_, c) in chars.by_ref() { if c == '`' { closed = true; break; } code.push(c); }
                if closed {
                    spans.push(Span::styled(format!(" {} ", code),
                        Style::default().fg(Palette::ACCENT2).bg(Color::Rgb(30, 33, 48))));
                } else { buf.push('`'); buf.push_str(&code); }
            }
            '*' | '_' => {
                let double = chars.peek().map(|(_, c)| *c == ch).unwrap_or(false);
                if double {
                    chars.next();
                    if !buf.is_empty() { spans.push(Span::styled(buf.clone(), base)); buf.clear(); }
                    let mut t = String::new(); let mut closed = false;
                    while let Some((_, c)) = chars.next() {
                        if c == ch { if chars.peek().map(|(_, c2)| *c2 == ch).unwrap_or(false) { chars.next(); closed = true; break; } }
                        t.push(c);
                    }
                    if closed { spans.push(Span::styled(t, Style::default().fg(Palette::TEXT_BRIGHT).add_modifier(Modifier::BOLD))); }
                    else { buf.push(ch); buf.push(ch); buf.push_str(&t); }
                } else {
                    if !buf.is_empty() { spans.push(Span::styled(buf.clone(), base)); buf.clear(); }
                    let mut t = String::new(); let mut closed = false;
                    for (_, c) in chars.by_ref() { if c == ch { closed = true; break; } t.push(c); }
                    if closed { spans.push(Span::styled(t, Style::default().fg(Palette::TEXT).add_modifier(Modifier::ITALIC))); }
                    else { buf.push(ch); buf.push_str(&t); }
                }
            }
            '~' if chars.peek().map(|(_, c)| *c == '~').unwrap_or(false) => {
                chars.next();
                if !buf.is_empty() { spans.push(Span::styled(buf.clone(), base)); buf.clear(); }
                let mut t = String::new(); let mut closed = false;
                while let Some((_, c)) = chars.next() {
                    if c == '~' { if chars.peek().map(|(_, c2)| *c2 == '~').unwrap_or(false) { chars.next(); closed = true; break; } }
                    t.push(c);
                }
                if closed { spans.push(Span::styled(t, Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::CROSSED_OUT))); }
                else { buf.push_str("~~"); buf.push_str(&t); }
            }
            _ => buf.push(ch),
        }
    }
    if !buf.is_empty() { spans.push(Span::styled(buf, base)); }
    if spans.is_empty() { spans.push(Span::styled("", base)); }
    spans
}

/// Syntax-highlight a code block via syntect.
fn highlight_code_block(lines: &[String], lang: &str, max_width: usize) -> Vec<Line<'static>> {
    let bg = Color::Rgb(22, 24, 35);
    let bg_style = Style::default().bg(bg);

    HL.with(|hl| {
        let syntax = hl.syntax_set.find_syntax_by_token(lang)
            .or_else(|| hl.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| hl.syntax_set.find_syntax_plain_text());
        let theme = &hl.theme_set.themes["base16-ocean.dark"];
        let mut h = HighlightLines::new(syntax, theme);
        let mut result = Vec::new();

        for line in lines {
            let hl_spans = h.highlight_line(line, &hl.syntax_set).unwrap_or_default();
            let mut spans: Vec<Span<'static>> = vec![Span::styled("  ", bg_style)];
            for (style, text) in hl_spans {
                let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                let mut s = Style::default().fg(fg).bg(bg);
                if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) { s = s.add_modifier(Modifier::BOLD); }
                if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) { s = s.add_modifier(Modifier::ITALIC); }
                spans.push(Span::styled(text.to_string(), s));
            }
            let w: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            if w < max_width { spans.push(Span::styled(" ".repeat(max_width - w), bg_style)); }
            result.push(Line::from(spans));
        }
        result.push(Line::from(Span::styled("\u{2500}".repeat(max_width.min(60)), Style::default().fg(Palette::BORDER))));
        result
    })
}
