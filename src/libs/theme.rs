//! Shared colour palette + small widget helpers so every page looks like it
//! came out of the same terminal.
//!
//! The palette is a "dev console" take on Dracula: a dark base, a neon green
//! accent for prompts/active state, and muted blue-greys for the chrome.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Base background painted behind every frame.
pub const BG: Color = Color::Rgb(23, 24, 33);
/// Slightly lighter surface used for the selected row / chips.
pub const SURFACE: Color = Color::Rgb(40, 42, 58);
/// Default foreground.
pub const FG: Color = Color::Rgb(238, 240, 248);
/// Chrome / secondary text.
pub const DIM: Color = Color::Rgb(118, 126, 158);
/// Borders and separators.
pub const FAINT: Color = Color::Rgb(71, 75, 100);

pub const GREEN: Color = Color::Rgb(97, 240, 154);
pub const CYAN: Color = Color::Rgb(126, 231, 251);
pub const BLUE: Color = Color::Rgb(122, 162, 247);
pub const PURPLE: Color = Color::Rgb(187, 154, 247);
pub const YELLOW: Color = Color::Rgb(235, 226, 138);
pub const ORANGE: Color = Color::Rgb(255, 184, 108);

/// Paints the app background over `area`.
pub fn paint(frame: &mut Frame, area: Rect) {
    frame.render_widget(Block::default().style(Style::new().bg(BG)), area);
}

fn title_line(title: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw(" "),
        Span::styled(
            title.to_string(),
            Style::new().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ])
}

/// A rounded panel with a title — the standard container for a page.
pub fn panel(title: &str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(FAINT))
        .title(title_line(title))
        .style(Style::new().bg(BG))
}

/// A panel whose title carries a right-aligned hint (counts, progress, ...).
pub fn panel_with_hint(title: &str, hint: &str) -> Block<'static> {
    panel(title).title(
        Line::from(Span::styled(format!("{hint} "), Style::new().fg(DIM)))
            .alignment(Alignment::Right),
    )
}

/// Small `[ key ]` chip used in the nav bar.
pub fn chip(label: &str, active: bool) -> Span<'static> {
    let style = if active {
        Style::new().fg(BG).bg(GREEN).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(DIM).bg(SURFACE)
    };
    Span::styled(format!(" {label} "), style)
}

/// Key-cap hint, e.g. `[↵] open`.
pub fn key_hint(key: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {key} "),
            Style::new()
                .fg(GREEN)
                .bg(SURFACE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {label} "), Style::new().fg(DIM)),
    ]
}

/// A `➜ area $ command` prompt line.
pub fn prompt(area: &str, cmd: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("➜ ", Style::new().fg(GREEN)),
        Span::styled(format!("{area} "), Style::new().fg(CYAN)),
        Span::styled("$ ", Style::new().fg(DIM)),
        Span::styled(cmd.to_string(), Style::new().fg(FG)),
    ])
}

/// A horizontal rule that fills `width` columns.
pub fn rule(width: u16) -> Line<'static> {
    Line::from(Span::styled(
        "─".repeat(width.max(1) as usize),
        Style::new().fg(FAINT),
    ))
}

/// Truncates `text` to `width` display columns, appending `…` when cut.
pub fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w >= width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

/// Renders a `<hours>:<minutes>Z` wall clock (UTC) without pulling in a date crate.
pub fn clock_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{:02}:{:02}Z", (secs / 3600) % 24, (secs / 60) % 60)
}
