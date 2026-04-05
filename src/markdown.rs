//! Lightweight markdown-to-Span renderer for Ratatui.
//!
//! Parses a subset of markdown (headers, bold, italic, inline code,
//! fenced code blocks, lists, blockquotes, horizontal rules) and
//! returns styled `Line<'static>` sequences suitable for the chat area.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

use crate::tui::Palette;

/// Lazy-initialized syntax highlighting state.
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

/// Convert markdown text into a sequence of styled ratatui Lines.
pub fn render_markdown(text: &str, max_width: usize) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut in_code_block = false;
    let mut code_lang = String::new();
    let mut code_buffer: Vec<String> = Vec::new();

    for raw_line in text.lines() {
        // Fenced code block toggle
        if raw_line.trim_start().starts_with("```") {
            if in_code_block {
                let highlighted = highlight_code_block(&code_buffer, &code_lang, max_width);
                lines.extend(highlighted);
                code_buffer.clear();
                code_lang.clear();
                in_code_block = false;
            } else {
                in_code_block = true;
                code_lang = raw_line.trim_start().trim_start_matches('`').trim().to_string();
                let lang_label = if code_lang.is_empty() { "code" } else { &code_lang };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!(" {} ", lang_label),
                        Style::default()
                            .fg(Palette::BG)
                            .bg(Palette::ACCENT2)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        " ".repeat(max_width.saturating_sub(lang_label.len() + 3)),
                        Style::default().bg(Color::Rgb(30, 33, 48)),
                    ),
                ]));
            }
            continue;
        }

        if in_code_block {
            code_buffer.push(raw_line.to_string());
            continue;
        }

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
            lines.push(Line::from(Span::styled(
                format!("   {}", rest),
                Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("## ") {
            lines.push(Line::from(Span::styled(
                format!("  {}", rest),
                Style::default().fg(Palette::ACCENT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("# ") {
            lines.push(Line::from(Span::styled(
                format!(" {}", rest),
                Style::default().fg(Palette::TEXT_BRIGHT).add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )));
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
            let prefix = " ".repeat(indent);
            let bullet = if indent > 0 { "\u{25E6}" } else { "\u{2022}" };
            let mut spans = vec![Span::styled(
                format!("{}  {} ", prefix, bullet),
                Style::default().fg(Palette::ACCENT2),
            )];
            spans.extend(parse_inline_markdown(rest));
            lines.push(Line::from(spans));
            continue;
        }

        // Ordered list
        if let Some(dot_pos) = trimmed.find(". ") {
            let number_part = &trimmed[..dot_pos];
            if number_part.chars().all(|c| c.is_ascii_digit()) && !number_part.is_empty() {
                let rest = &trimmed[dot_pos + 2..];
                let indent = raw_line.len() - trimmed.len();
                let prefix = " ".repeat(indent);
                let mut spans = vec![Span::styled(
                    format!("{}  {}. ", prefix, number_part),
                    Style::default().fg(Palette::ACCENT2).add_modifier(Modifier::BOLD),
                )];
                spans.extend(parse_inline_markdown(rest));
                lines.push(Line::from(spans));
                continue;
            }
        }

        // Empty line
        if trimmed.is_empty() {
            lines.push(Line::raw(""));
            continue;
        }

        // Regular paragraph with inline formatting
        let spans = parse_inline_markdown(raw_line);
        lines.push(Line::from(spans));
    }

    // Unclosed code block
    if in_code_block && !code_buffer.is_empty() {
        let highlighted = highlight_code_block(&code_buffer, &code_lang, max_width);
        lines.extend(highlighted);
    }

    lines
}

/// Parse inline markdown: **bold**, *italic*, `code`, ~~strike~~
fn parse_inline_markdown(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.char_indices().peekable();
    let mut current = String::new();
    let base_style = Style::default().fg(Palette::TEXT);

    while let Some((_i, ch)) = chars.next() {
        match ch {
            '`' => {
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), base_style));
                    current.clear();
                }
                let mut code = String::new();
                let mut closed = false;
                for (_, c) in chars.by_ref() {
                    if c == '`' { closed = true; break; }
                    code.push(c);
                }
                if closed {
                    spans.push(Span::styled(
                        format!(" {} ", code),
                        Style::default().fg(Palette::ACCENT2).bg(Color::Rgb(30, 33, 48)),
                    ));
                } else {
                    current.push('`');
                    current.push_str(&code);
                }
            }
            '*' | '_' => {
                let is_double = chars.peek().map(|(_, c)| *c == ch).unwrap_or(false);
                if is_double {
                    chars.next();
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), base_style));
                        current.clear();
                    }
                    let mut bold_text = String::new();
                    let mut closed = false;
                    while let Some((_, c)) = chars.next() {
                        if c == ch {
                            if chars.peek().map(|(_, c2)| *c2 == ch).unwrap_or(false) {
                                chars.next();
                                closed = true;
                                break;
                            }
                        }
                        bold_text.push(c);
                    }
                    if closed {
                        spans.push(Span::styled(
                            bold_text,
                            Style::default().fg(Palette::TEXT_BRIGHT).add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        current.push(ch);
                        current.push(ch);
                        current.push_str(&bold_text);
                    }
                } else {
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), base_style));
                        current.clear();
                    }
                    let mut italic_text = String::new();
                    let mut closed = false;
                    for (_, c) in chars.by_ref() {
                        if c == ch { closed = true; break; }
                        italic_text.push(c);
                    }
                    if closed {
                        spans.push(Span::styled(
                            italic_text,
                            Style::default().fg(Palette::TEXT).add_modifier(Modifier::ITALIC),
                        ));
                    } else {
                        current.push(ch);
                        current.push_str(&italic_text);
                    }
                }
            }
            '~' => {
                let is_double = chars.peek().map(|(_, c)| *c == '~').unwrap_or(false);
                if is_double {
                    chars.next();
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), base_style));
                        current.clear();
                    }
                    let mut strike_text = String::new();
                    let mut closed = false;
                    while let Some((_, c)) = chars.next() {
                        if c == '~' {
                            if chars.peek().map(|(_, c2)| *c2 == '~').unwrap_or(false) {
                                chars.next();
                                closed = true;
                                break;
                            }
                        }
                        strike_text.push(c);
                    }
                    if closed {
                        spans.push(Span::styled(
                            strike_text,
                            Style::default().fg(Palette::TEXT_DIM).add_modifier(Modifier::CROSSED_OUT),
                        ));
                    } else {
                        current.push_str("~~");
                        current.push_str(&strike_text);
                    }
                } else {
                    current.push(ch);
                }
            }
            _ => { current.push(ch); }
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, base_style));
    }
    if spans.is_empty() {
        spans.push(Span::styled("", base_style));
    }
    spans
}

/// Render a code block with syntax highlighting via syntect.
fn highlight_code_block(code_lines: &[String], lang: &str, max_width: usize) -> Vec<Line<'static>> {
    let bg = Color::Rgb(22, 24, 35);
    let code_bg_style = Style::default().bg(bg);

    HL.with(|hl| {
        let syntax = hl.syntax_set
            .find_syntax_by_token(lang)
            .or_else(|| hl.syntax_set.find_syntax_by_extension(lang))
            .unwrap_or_else(|| hl.syntax_set.find_syntax_plain_text());

        let theme = &hl.theme_set.themes["base16-ocean.dark"];
        let mut highlighter = HighlightLines::new(syntax, theme);

        let mut result: Vec<Line<'static>> = Vec::new();

        for code_line in code_lines {
            let highlighted = highlighter
                .highlight_line(code_line, &hl.syntax_set)
                .unwrap_or_default();

            let mut spans: Vec<Span<'static>> = Vec::new();
            spans.push(Span::styled("  ", code_bg_style));

            for (style, text) in highlighted {
                let fg = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                let mut ratatui_style = Style::default().fg(fg).bg(bg);
                if style.font_style.contains(syntect::highlighting::FontStyle::BOLD) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                }
                if style.font_style.contains(syntect::highlighting::FontStyle::ITALIC) {
                    ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                }
                spans.push(Span::styled(text.to_string(), ratatui_style));
            }

            let text_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
            if text_width < max_width {
                spans.push(Span::styled(
                    " ".repeat(max_width.saturating_sub(text_width)),
                    code_bg_style,
                ));
            }

            result.push(Line::from(spans));
        }

        result.push(Line::from(Span::styled(
            "\u{2500}".repeat(max_width.min(60)),
            Style::default().fg(Palette::BORDER),
        )));

        result
    })
}
