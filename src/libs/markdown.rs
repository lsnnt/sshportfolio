//! A small CommonMark → ratatui renderer.
//!
//! blog.lsnnt.dev serves every post as markdown (`https://blog.lsnnt.dev/<slug>.md`),
//! so the SSH client parses it once ([`parse`]) and re-flows it into terminal
//! lines ([`render`]) whenever the window width changes.
//!
//! Only the subset that is actually useful in a terminal is modelled: headings,
//! paragraphs, code blocks, lists, quotes, rules and images. Raw HTML is
//! dropped, tables are intentionally unsupported.

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::libs::theme;

/// One inline chunk of text with the styling that applies to it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
    /// Explicit colour (used by headings); `None` means "pick from the flags".
    pub color: Option<Color>,
}

impl Run {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// The terminal style for this run.
    pub fn style(&self) -> Style {
        let mut style = Style::new();
        if let Some(color) = self.color {
            style = style.fg(color);
        } else if self.code {
            style = style.fg(theme::ORANGE);
        } else if self.link.is_some() {
            style = style.fg(theme::BLUE).add_modifier(Modifier::UNDERLINED);
        } else {
            style = style.fg(theme::FG);
        }
        if self.bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if self.italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        if self.strike {
            style = style.add_modifier(Modifier::CROSSED_OUT);
        }
        style
    }
}

/// One entry of an (possibly nested) ordered/unordered list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListItem {
    /// Nesting depth, `0` for a top-level list.
    pub indent: usize,
    pub ordered: bool,
    pub number: u64,
    pub runs: Vec<Run>,
}

/// A parsed markdown block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, runs: Vec<Run> },
    Paragraph(Vec<Run>),
    Code { lang: String, lines: Vec<String> },
    List(Vec<ListItem>),
    Quote(Vec<Run>),
    Rule,
    Image { alt: String, url: String },
}

// ---------------------------------------------------------------------------
// parsing
// ---------------------------------------------------------------------------

struct ItemState {
    indent: usize,
    ordered: bool,
    number: u64,
}

/// Parses markdown into the block model above.
pub fn parse(markdown: &str) -> Vec<Block> {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    options.insert(Options::ENABLE_SMART_PUNCTUATION);

    let mut blocks: Vec<Block> = Vec::new();
    let mut runs: Vec<Run> = Vec::new();

    // Inline / block style state.
    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut strike = 0usize;
    let mut links: Vec<String> = Vec::new();
    let mut heading: Option<u8> = None;
    let mut quote = false;
    let mut code: Option<(String, String)> = None;
    let mut image: Option<(String, String)> = None;

    // List state.
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut counters: Vec<u64> = Vec::new();
    let mut item: Option<ItemState> = None;
    let mut items: Vec<ListItem> = Vec::new();

    for event in Parser::new_ext(markdown, options) {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    heading = Some(level as u8);
                    runs.clear();
                }
                Tag::Paragraph => {}
                Tag::BlockQuote(_) => {
                    quote = true;
                    runs.clear();
                }
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(info) => info
                            .split_whitespace()
                            .next()
                            .unwrap_or_default()
                            .to_string(),
                        CodeBlockKind::Indented => String::new(),
                    };
                    code = Some((lang, String::new()));
                }
                Tag::List(start) => {
                    // A nested list starts a new item at a deeper level, so the
                    // item we were in has to be closed first to keep the order.
                    if item.is_some() {
                        close_item(&mut items, &mut item, &mut runs);
                    }
                    lists.push(start);
                    counters.push(start.unwrap_or(1));
                }
                Tag::Item => {
                    let indent = lists.len().saturating_sub(1);
                    let ordered = lists.last().map(|l| l.is_some()).unwrap_or(false);
                    let number = match counters.last_mut() {
                        Some(counter) => {
                            let number = *counter;
                            *counter += 1;
                            number
                        }
                        None => 1,
                    };
                    runs.clear();
                    item = Some(ItemState {
                        indent,
                        ordered,
                        number,
                    });
                }
                Tag::Emphasis => italic += 1,
                Tag::Strong => bold += 1,
                Tag::Strikethrough => strike += 1,
                Tag::Link { dest_url, .. } => links.push(dest_url.to_string()),
                Tag::Image { dest_url, .. } => image = Some((String::new(), dest_url.to_string())),
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => {
                    if let Some(level) = heading.take() {
                        blocks.push(Block::Heading {
                            level,
                            runs: std::mem::take(&mut runs),
                        });
                    }
                }
                TagEnd::Paragraph => {
                    // Paragraph text inside a list item or a quote is flushed
                    // when the container closes, so only top-level paragraphs
                    // are emitted here.
                    if !quote && item.is_none() {
                        flush_paragraph(&mut blocks, &mut runs);
                    }
                }
                TagEnd::BlockQuote(_) => {
                    quote = false;
                    if !runs.is_empty() {
                        blocks.push(Block::Quote(std::mem::take(&mut runs)));
                    }
                }
                TagEnd::CodeBlock => {
                    if let Some((lang, body)) = code.take() {
                        let body = body.strip_suffix('\n').unwrap_or(&body);
                        blocks.push(Block::Code {
                            lang,
                            lines: body.split('\n').map(str::to_string).collect(),
                        });
                    }
                }
                TagEnd::List(_) => {
                    lists.pop();
                    counters.pop();
                    if lists.is_empty() {
                        if item.is_some() {
                            close_item(&mut items, &mut item, &mut runs);
                        }
                        blocks.push(Block::List(std::mem::take(&mut items)));
                    }
                }
                TagEnd::Item => close_item(&mut items, &mut item, &mut runs),
                TagEnd::Emphasis => italic = italic.saturating_sub(1),
                TagEnd::Strong => bold = bold.saturating_sub(1),
                TagEnd::Strikethrough => strike = strike.saturating_sub(1),
                TagEnd::Link => {
                    links.pop();
                }
                TagEnd::Image => {
                    if let Some((alt, url)) = image.take() {
                        blocks.push(Block::Image {
                            alt: alt.trim().to_string(),
                            url,
                        });
                    }
                }
                _ => {}
            },
            Event::Text(text) => {
                if let Some((_, body)) = code.as_mut() {
                    body.push_str(&text);
                } else if let Some((alt, _)) = image.as_mut() {
                    alt.push_str(&text);
                } else {
                    let run = Run {
                        text: text.to_string(),
                        bold: bold > 0,
                        italic: italic > 0,
                        strike: strike > 0,
                        link: links.last().cloned(),
                        ..Run::default()
                    };
                    push_run(&mut runs, run);
                }
            }
            Event::Code(text) => push_run(
                &mut runs,
                Run {
                    text: text.to_string(),
                    code: true,
                    bold: bold > 0,
                    italic: italic > 0,
                    strike: strike > 0,
                    link: links.last().cloned(),
                    ..Run::default()
                },
            ),
            Event::SoftBreak => {
                append_text(&mut runs, " ", inline_flags(bold, italic, strike, &links))
            }
            Event::HardBreak => {
                append_text(&mut runs, "\n", inline_flags(bold, italic, strike, &links))
            }
            Event::Rule => {
                flush_paragraph(&mut blocks, &mut runs);
                blocks.push(Block::Rule);
            }
            Event::TaskListMarker(done) => append_text(
                &mut runs,
                if done { "[x] " } else { "[ ] " },
                Run::default(),
            ),
            Event::FootnoteReference(name) => {
                append_text(&mut runs, &format!("[^{name}]"), Run::default())
            }
            Event::InlineMath(math) | Event::DisplayMath(math) => push_run(
                &mut runs,
                Run {
                    text: math.to_string(),
                    code: true,
                    ..Run::default()
                },
            ),
            // Raw HTML (both inline and block) renders as noise in a terminal.
            Event::Html(_) | Event::InlineHtml(_) => {}
        }
    }

    // Tolerate truncated markdown: flush whatever is still pending.
    if let Some((lang, body)) = code.take() {
        blocks.push(Block::Code {
            lang,
            lines: body.split('\n').map(str::to_string).collect(),
        });
    }
    if item.is_some() {
        close_item(&mut items, &mut item, &mut runs);
    }
    if !items.is_empty() {
        blocks.push(Block::List(std::mem::take(&mut items)));
    }
    flush_paragraph(&mut blocks, &mut runs);
    blocks
}

fn inline_flags(bold: usize, italic: usize, strike: usize, links: &[String]) -> Run {
    Run {
        bold: bold > 0,
        italic: italic > 0,
        strike: strike > 0,
        link: links.last().cloned(),
        ..Run::default()
    }
}

/// Appends text to the pending runs, merging into the previous run when the
/// styling matches (so a paragraph does not explode into one span per word).
fn append_text(runs: &mut Vec<Run>, text: &str, style: Run) {
    if let Some(last) = runs.last_mut() {
        let same = last.bold == style.bold
            && last.italic == style.italic
            && last.code == style.code
            && last.strike == style.strike
            && last.link == style.link
            && last.color == style.color;
        if same {
            last.text.push_str(text);
            return;
        }
    }
    runs.push(Run {
        text: text.to_string(),
        ..style
    });
}

fn push_run(runs: &mut Vec<Run>, run: Run) {
    let text = run.text.clone();
    append_text(
        runs,
        &text,
        Run {
            text: String::new(),
            ..run
        },
    );
}

fn flush_paragraph(blocks: &mut Vec<Block>, runs: &mut Vec<Run>) {
    let pending = std::mem::take(runs);
    let has_text = pending.iter().any(|r| !r.text.trim().is_empty());
    if has_text {
        blocks.push(Block::Paragraph(pending));
    }
}

fn close_item(items: &mut Vec<ListItem>, item: &mut Option<ItemState>, runs: &mut Vec<Run>) {
    if let Some(state) = item.take() {
        items.push(ListItem {
            indent: state.indent,
            ordered: state.ordered,
            number: state.number,
            runs: std::mem::take(runs),
        });
    }
}

// ---------------------------------------------------------------------------
// rendering
// ---------------------------------------------------------------------------

/// Renders parsed blocks into terminal lines that fit `width` columns.
pub fn render(blocks: &[Block], width: u16) -> Vec<Line<'static>> {
    let width = (width as usize).max(8);
    let mut out: Vec<Line<'static>> = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        if index > 0 {
            out.push(Line::default());
        }
        render_block(block, width, &mut out);
    }
    out
}

fn render_block(block: &Block, width: usize, out: &mut Vec<Line<'static>>) {
    match block {
        Block::Heading { level, runs } => {
            let (marker, color) = match level {
                1 => ("┃ ", theme::GREEN),
                2 => ("▎ ", theme::CYAN),
                3 => ("▸ ", theme::PURPLE),
                _ => ("· ", theme::DIM),
            };
            let marker_style = Style::new().fg(color).add_modifier(Modifier::BOLD);
            let styled: Vec<Run> = runs
                .iter()
                .map(|run| Run {
                    bold: true,
                    color: Some(color),
                    ..run.clone()
                })
                .collect();
            let indent = " ".repeat(marker.width());
            out.extend(wrap_prefixed(
                &styled,
                width,
                vec![Span::styled(marker, marker_style)],
                vec![Span::raw(indent)],
            ));
            if *level == 1 {
                out.push(theme::rule(width as u16));
            }
        }
        Block::Paragraph(runs) => {
            out.extend(wrap_prefixed(runs, width, vec![], vec![]));
        }
        Block::Quote(runs) => {
            let styled: Vec<Run> = runs
                .iter()
                .map(|run| Run {
                    italic: true,
                    color: Some(theme::DIM),
                    ..run.clone()
                })
                .collect();
            out.extend(wrap_prefixed(
                &styled,
                width,
                vec![Span::styled("▏ ", Style::new().fg(theme::GREEN))],
                vec![Span::styled("▏ ", Style::new().fg(theme::GREEN))],
            ));
        }
        Block::Code { lang, lines } => render_code(lang, lines, width, out),
        Block::List(items) => {
            for item in items {
                let indent = "  ".repeat(item.indent);
                let marker = if item.ordered {
                    format!("{}. ", item.number)
                } else {
                    "• ".to_string()
                };
                let (prefix, color) = (format!(" {indent}{marker}"), {
                    if item.ordered {
                        theme::CYAN
                    } else {
                        theme::GREEN
                    }
                });
                let continuation = " ".repeat(prefix.width());
                out.extend(wrap_prefixed(
                    &item.runs,
                    width,
                    vec![Span::styled(prefix, Style::new().fg(color))],
                    vec![Span::raw(continuation)],
                ));
            }
        }
        Block::Rule => out.push(theme::rule(width as u16)),
        Block::Image { alt, url } => {
            let alt = if alt.is_empty() {
                "image".to_string()
            } else {
                alt.clone()
            };
            out.push(Line::from(vec![
                Span::styled("▨ ", Style::new().fg(theme::PURPLE)),
                Span::styled(
                    alt,
                    Style::new().fg(theme::PURPLE).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("  {}", theme::truncate(url, width.saturating_sub(6))),
                    Style::new().fg(theme::DIM).add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
    }
}

fn render_code(lang: &str, lines: &[String], width: usize, out: &mut Vec<Line<'static>>) {
    let width = width.max(10);
    let label = if lang.trim().is_empty() {
        String::new()
    } else {
        format!(
            " {} ",
            theme::truncate(lang.trim(), width.saturating_sub(6))
        )
    };
    let head_fill = width.saturating_sub(3 + label.width());
    out.push(Line::from(vec![
        Span::styled("╭─", Style::new().fg(theme::FAINT)),
        Span::styled(
            label,
            Style::new().fg(theme::YELLOW).add_modifier(Modifier::BOLD),
        ),
        Span::styled("─".repeat(head_fill), Style::new().fg(theme::FAINT)),
        Span::styled("╮", Style::new().fg(theme::FAINT)),
    ]));

    let inner = width.saturating_sub(4).max(4);
    let border = Style::new().fg(theme::FAINT).bg(theme::SURFACE);
    let code_style = Style::new().fg(theme::FG).bg(theme::SURFACE);
    let fill = Style::new().bg(theme::SURFACE);

    for line in lines {
        let expanded = line.replace('\t', "    ");
        for chunk in hard_wrap(&expanded, inner) {
            out.push(Line::from(vec![
                Span::styled("│ ", border),
                Span::styled(chunk.clone(), code_style),
                Span::styled(" ".repeat(inner.saturating_sub(chunk.width())), fill),
                Span::styled("│", border),
            ]));
        }
    }

    out.push(Line::from(vec![
        Span::styled("╰", Style::new().fg(theme::FAINT)),
        Span::styled(
            "─".repeat(width.saturating_sub(2)),
            Style::new().fg(theme::FAINT),
        ),
        Span::styled("╯", Style::new().fg(theme::FAINT)),
    ]));
}

/// Greedy reflow of inline runs into lines with per-line prefixes.
fn wrap_prefixed(
    runs: &[Run],
    width: usize,
    first: Vec<Span<'static>>,
    rest: Vec<Span<'static>>,
) -> Vec<Line<'static>> {
    let mut wrapper = Wrapper::new(width, first, rest);
    for token in tokenize(runs) {
        match token {
            Token::Break => wrapper.hard_break(),
            Token::Word(run, space_before) => {
                wrapper.push_word(&run.text, run.style(), space_before)
            }
        }
    }
    wrapper.finish()
}

enum Token {
    /// A word plus whether whitespace preceded it in the source.
    Word(Run, bool),
    /// A hard line break inside the source.
    Break,
}

/// Splits inline runs into words, remembering which words were actually
/// separated by whitespace — `[link](url).` must not become `link .`.
fn tokenize(runs: &[Run]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut space_pending = false;

    for run in runs {
        let mut word = String::new();
        for character in run.text.chars() {
            if character == '\n' {
                if !word.is_empty() {
                    tokens.push(Token::Word(
                        Run {
                            text: std::mem::take(&mut word),
                            ..run.clone()
                        },
                        space_pending,
                    ));
                }
                tokens.push(Token::Break);
                space_pending = false;
            } else if character.is_whitespace() {
                if !word.is_empty() {
                    tokens.push(Token::Word(
                        Run {
                            text: std::mem::take(&mut word),
                            ..run.clone()
                        },
                        space_pending,
                    ));
                }
                space_pending = true;
            } else {
                word.push(character);
            }
        }
        if !word.is_empty() {
            tokens.push(Token::Word(
                Run {
                    text: word,
                    ..run.clone()
                },
                space_pending,
            ));
            space_pending = false;
        }
    }
    tokens
}

struct Wrapper {
    width: usize,
    first: Vec<Span<'static>>,
    rest: Vec<Span<'static>>,
    first_width: usize,
    rest_width: usize,
    current: Vec<Span<'static>>,
    current_width: usize,
    started: bool,
    out: Vec<Line<'static>>,
}

impl Wrapper {
    fn new(width: usize, first: Vec<Span<'static>>, rest: Vec<Span<'static>>) -> Self {
        let width = width.max(4);
        // Never let the prefix eat the whole line, otherwise wrapping can loop.
        let cap = width.saturating_sub(2).max(1);
        let first_width = span_width(&first).min(cap);
        let rest_width = span_width(&rest).min(cap);
        Self {
            width,
            first,
            rest,
            first_width,
            rest_width,
            current: Vec::new(),
            current_width: 0,
            started: false,
            out: Vec::new(),
        }
    }

    fn indent(&self) -> usize {
        if self.started {
            self.rest_width
        } else {
            self.first_width
        }
    }

    fn prefix(&self) -> Vec<Span<'static>> {
        if self.started {
            self.rest.clone()
        } else {
            self.first.clone()
        }
    }

    fn flush(&mut self) {
        let mut line = self.prefix();
        line.append(&mut self.current);
        self.out.push(Line::from(line));
        self.started = true;
        self.current_width = 0;
    }

    fn hard_break(&mut self) {
        if !self.current.is_empty() {
            self.flush();
        }
    }

    fn push_word(&mut self, word: &str, style: Style, space_before: bool) {
        let mut remaining = word;
        let mut separated = space_before;
        while !remaining.is_empty() {
            let word_width = remaining.width();
            if self.current.is_empty() {
                self.current_width = self.indent();
            } else if self.current_width + usize::from(separated) + word_width > self.width {
                self.flush();
                self.current_width = self.indent();
            } else if separated {
                self.current.push(Span::raw(" "));
                self.current_width += 1;
            }
            separated = false;

            let room = self.width.saturating_sub(self.current_width).max(1);
            if word_width <= room {
                self.current
                    .push(Span::styled(remaining.to_string(), style));
                self.current_width += word_width;
                return;
            }

            // Word longer than the line: hard-split it.
            let (head, tail) = split_at_width(remaining, room);
            self.current.push(Span::styled(head.to_string(), style));
            self.current_width += head.width();
            self.flush();
            remaining = tail;
        }
    }

    fn finish(mut self) -> Vec<Line<'static>> {
        if !self.current.is_empty() {
            self.flush();
        }
        self.out
    }
}

fn span_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

/// Splits `text` so that the first part is at most `max` columns wide.
fn split_at_width(text: &str, max: usize) -> (&str, &str) {
    let max = max.max(1);
    let mut used = 0usize;
    let mut end = 0usize;
    for (index, ch) in text.char_indices() {
        let width = ch.width().unwrap_or(0);
        if used + width > max {
            break;
        }
        used += width;
        end = index + ch.len_utf8();
    }
    if end == 0 {
        // A single character wider than the line: keep it, do not loop forever.
        end = text
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(text.len());
    }
    text.split_at(end)
}

/// Splits a (possibly long) single line into chunks of at most `width` columns.
fn hard_wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    if text.is_empty() {
        return vec![String::new()];
    }
    let mut chunks = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let (head, tail) = split_at_width(rest, width);
        chunks.push(head.to_string());
        rest = tail;
    }
    chunks
}

// ---------------------------------------------------------------------------
// helpers used by the post list
// ---------------------------------------------------------------------------

/// Rough word count of a parsed post.
pub fn word_count(blocks: &[Block]) -> usize {
    let mut words = 0usize;
    let count_runs = |runs: &[Run]| {
        runs.iter()
            .map(|run| run.text.split_whitespace().count())
            .sum::<usize>()
    };
    for block in blocks {
        match block {
            Block::Heading { runs, .. } | Block::Paragraph(runs) | Block::Quote(runs) => {
                words += count_runs(runs)
            }
            Block::List(items) => {
                for item in items {
                    words += count_runs(&item.runs);
                }
            }
            Block::Code { lines, .. } => {
                words += lines
                    .iter()
                    .map(|l| l.split_whitespace().count())
                    .sum::<usize>()
            }
            Block::Image { alt, .. } => words += alt.split_whitespace().count(),
            Block::Rule => {}
        }
    }
    words
}

/// A one-line teaser taken from the first paragraph (or list item) of a post.
pub fn excerpt(blocks: &[Block], max_chars: usize) -> String {
    let text = blocks
        .iter()
        .find_map(|block| match block {
            Block::Paragraph(runs) | Block::Quote(runs) => {
                let text: String = runs.iter().map(|run| run.text.as_str()).collect();
                (!text.trim().is_empty()).then_some(text)
            }
            Block::List(items) => items.first().map(|item| {
                item.runs
                    .iter()
                    .map(|run| run.text.as_str())
                    .collect::<String>()
            }),
            _ => None,
        })
        .unwrap_or_default();
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = collapsed.chars().take(max_chars).collect();
    if collapsed.chars().count() > max_chars {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(lines: &[Line<'static>]) -> Vec<String> {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn parses_headings_paragraphs_and_code() {
        let blocks = parse("# Title\n\nHello **world**.\n\n```rust\nfn main() {}\n```\n");
        assert_eq!(blocks.len(), 3);
        match &blocks[0] {
            Block::Heading { level, runs } => {
                assert_eq!(*level, 1);
                assert_eq!(runs[0].text, "Title");
            }
            other => panic!("expected heading, got {other:?}"),
        }
        match &blocks[1] {
            Block::Paragraph(runs) => {
                let text: String = runs.iter().map(|r| r.text.as_str()).collect();
                assert_eq!(text, "Hello world.");
                assert!(runs.iter().any(|r| r.text == "world" && r.bold));
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
        match &blocks[2] {
            Block::Code { lang, lines } => {
                assert_eq!(lang, "rust");
                assert_eq!(lines, &vec!["fn main() {}".to_string()]);
            }
            other => panic!("expected code, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_lists_in_order() {
        let blocks = parse("1. one\n2. two\n   - nested\n");
        let items = match &blocks[0] {
            Block::List(items) => items,
            other => panic!("expected list, got {other:?}"),
        };
        assert_eq!(items.len(), 3);
        assert!(items[0].ordered && items[0].number == 1 && items[0].indent == 0);
        assert!(items[1].ordered && items[1].number == 2);
        assert!(!items[2].ordered && items[2].indent == 1);
    }

    #[test]
    fn parses_links_images_quotes_and_inline_code() {
        let blocks = parse(
            "> note\n\nA [link](https://example.com) and `code`.\n\n![shot](https://x.test/a.png)\n",
        );
        assert!(matches!(blocks[0], Block::Quote(_)));
        match &blocks[1] {
            Block::Paragraph(runs) => {
                assert!(runs.iter().any(|r| r.code && r.text == "code"));
                assert!(
                    runs.iter()
                        .any(|r| r.link.as_deref() == Some("https://example.com"))
                );
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
        match &blocks[2] {
            Block::Image { alt, url } => {
                assert_eq!(alt, "shot");
                assert_eq!(url, "https://x.test/a.png");
            }
            other => panic!("expected image, got {other:?}"),
        }
    }

    #[test]
    fn renders_heading_marker_and_code_box() {
        let blocks = parse("# Section\n\n```rust\nlet x = 1;\n```\n");
        let lines = text_of(&render(&blocks, 40));
        assert!(lines[0].starts_with("┃ Section"), "{lines:?}");
        assert!(
            lines
                .iter()
                .any(|l| l.contains("rust") && l.starts_with('╭'))
        );
        assert!(lines.iter().any(|l| l.contains("│ let x = 1;")));
        assert!(lines.iter().any(|l| l.starts_with('╰')));
    }

    #[test]
    fn wraps_long_paragraphs_and_never_loops() {
        let long = "word ".repeat(60);
        let blocks = parse(&long);
        for width in [8u16, 20, 40, 80] {
            let lines = render(&blocks, width);
            assert!(!lines.is_empty());
            for line in &lines {
                let text = text_of(std::slice::from_ref(line));
                assert!(
                    text[0].width() <= width.max(8) as usize,
                    "line {:?} wider than {width}",
                    text[0]
                );
            }
        }
    }

    #[test]
    fn list_items_are_indented_and_prefixed() {
        let blocks = parse("- alpha\n- beta\n");
        let lines = text_of(&render(&blocks, 40));
        assert!(lines[0].starts_with(" • alpha"), "{lines:?}");
        assert!(lines[1].starts_with(" • beta"), "{lines:?}");
    }

    #[test]
    fn excerpt_and_word_count_come_from_the_body() {
        let blocks = parse("## Head\n\nA short intro paragraph here.\n\nmore words\n");
        assert_eq!(excerpt(&blocks, 40), "A short intro paragraph here.");
        assert!(word_count(&blocks) >= 7);
    }

    #[test]
    fn no_space_is_invented_between_inline_spans() {
        let blocks = parse("See [blackhole](https://x.test). Then `code`.\n");
        let lines = text_of(&render(&blocks, 80));
        assert_eq!(lines[0], "See blackhole. Then code.");
    }

    #[test]
    fn hard_breaks_end_the_line() {
        let blocks = parse("first line\\\nsecond line\n");
        let lines = text_of(&render(&blocks, 80));
        assert_eq!(lines[0], "first line");
        assert_eq!(lines[1], "second line");
    }
}
