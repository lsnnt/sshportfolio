//! Everything that ends up on the client's screen.
//!
//! Layout is always the same three bands: a nav bar, the body and a footer with
//! context-aware key hints. The blog body has two modes — the selectable post
//! list and the post reader.

use std::collections::HashMap;

use ratatui::backend::Backend;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Frame, Line, Modifier, Span, Style};
use ratatui::widgets::{
    Block, BorderType, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};
use textwrap::{Options, wrap};
use tui_big_text::{BigText, PixelSize};

use crate::libs::mblog;
use crate::libs::theme;
use crate::libs::view::{Page, View};
use crate::models::hblogs::BlogPost;
use crate::{AppState, ClientState};

const LANGUAGES: &[&str] = &["Go", "Rust", "Python", "C++ / Java"];
const INFRA: &[&str] = &["Linux Kernel", "Docker", "AWS S3", "Wireguard / Bird"];
const SECURITY: &[&str] = &["Reverse Engineering", "PCAPDroid", "Burp Suite", "Frida"];

/// Below this the UI cannot be laid out sensibly.
const MIN_WIDTH: u16 = 32;
const MIN_HEIGHT: u16 = 10;
/// Width at which the nav bar can afford section names instead of just keys.
const WIDE_NAV: u16 = 96;
/// Width at which the footer can afford descriptive key hints.
const WIDE_FOOTER: u16 = 84;
/// Width at which the home page can afford the big banner font.
const WIDE_BANNER: u16 = 72;

/// Draws one client and records which data revision it has seen.
pub fn draw_client<B: Backend>(client: &mut ClientState<B>, state: &AppState) {
    let ClientState {
        terminal,
        view,
        drawn_revision,
        ..
    } = client;
    if terminal.draw(|frame| draw_ui(frame, view, state)).is_ok() {
        *drawn_revision = state.revision;
    }
}

/// Repaints every client that has not seen the current data revision, so an
/// idle connection costs nothing.
pub fn redraw_all<B: Backend>(clients: &mut HashMap<usize, ClientState<B>>, state: &AppState) {
    for client in clients.values_mut() {
        if client.drawn_revision != state.revision {
            draw_client(client, state);
        }
    }
}

/// Entry point used by both the key handler and the periodic refresh.
pub fn draw_ui(frame: &mut Frame, view: &mut View, state: &AppState) {
    let area = frame.area();
    theme::paint(frame, area);

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let message = vec![
            Line::default(),
            Line::from(Span::styled(
                "terminal too small",
                Style::new().fg(theme::ORANGE).add_modifier(Modifier::BOLD),
            ))
            .alignment(Alignment::Center),
            Line::from(Span::styled(
                format!(
                    "{}x{} — resize to at least {}x{}",
                    area.width, area.height, MIN_WIDTH, MIN_HEIGHT
                ),
                Style::new().fg(theme::DIM),
            ))
            .alignment(Alignment::Center),
        ];
        frame.render_widget(Paragraph::new(message), area);
        return;
    }

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // nav bar
            Constraint::Min(0),    // body
            Constraint::Length(3), // key hints
        ])
        .split(area);

    draw_nav(frame, rows[0], view, state);
    draw_body(frame, rows[1], view, state);
    draw_footer(frame, rows[2], view, state);
}

// ---------------------------------------------------------------------------
// nav bar
// ---------------------------------------------------------------------------

fn draw_nav(frame: &mut Frame, area: Rect, view: &View, state: &AppState) {
    let hint = format!("{} · {} posts ", theme::clock_utc(), state.posts.len());
    let block = theme::panel_with_hint("ssh://lsnnt.dev", &hint);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let wide = inner.width >= WIDE_NAV;
    let chips: Vec<Span<'static>> = Page::ALL
        .iter()
        .flat_map(|page| {
            let label = if wide {
                format!("[{}] {}", page.key(), page.label())
            } else {
                format!("[{}]", page.key())
            };
            [theme::chip(&label, *page == view.page), Span::raw(" ")]
        })
        .collect();
    let chips_width: u16 = chips
        .iter()
        .map(|span| span.content.chars().count() as u16)
        .sum::<u16>();

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(12),
            Constraint::Length(chips_width.min(inner.width)),
        ])
        .split(inner);

    let path = match (view.page, view.blog.open) {
        (Page::Blog, Some(index)) => match state.posts.get(index) {
            Some(post) => format!("~/blog/{}", post.slug),
            None => "~/blog".to_string(),
        },
        (page, _) => page.path().to_string(),
    };
    frame.render_widget(
        Paragraph::new(theme::prompt("root@nnt", &path)).style(Style::new().bg(theme::BG)),
        cols[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(chips))
            .alignment(Alignment::Right)
            .style(Style::new().bg(theme::BG)),
        cols[1],
    );
}

// ---------------------------------------------------------------------------
// body
// ---------------------------------------------------------------------------

fn draw_body(frame: &mut Frame, area: Rect, view: &mut View, state: &AppState) {
    // A refresh can replace the post list under an open reader; never render a
    // stale index.
    if view
        .blog
        .open
        .is_some_and(|index| index >= state.posts.len())
    {
        view.blog.close();
    }
    let reading = view.blog.is_reading();
    match view.page {
        Page::Home => draw_home(frame, area, state),
        Page::About => draw_about(frame, area),
        Page::Projects => draw_projects(frame, area, view, state),
        Page::Timeline => draw_timeline(frame, area, view, state),
        Page::Blog if reading => draw_post(frame, area, view, state),
        Page::Blog => draw_blogs(frame, area, view, state),
        Page::Contact => draw_contact(frame, area),
    }
}

fn draw_home(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = theme::panel_with_hint("~/home", "● ONLINE");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The banner font needs a wide terminal; on a narrow one it degrades to a
    // single line, so do not reserve eight rows for it.
    let wide_banner = inner.width >= WIDE_BANNER;
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),                               // prompt
            Constraint::Length(if wide_banner { 8 } else { 1 }), // banner
            Constraint::Min(0),                                  // facts
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(theme::prompt("~", "./init_profile.sh")),
        rows[0],
    );

    if wide_banner {
        let banner = BigText::builder()
            .pixel_size(PixelSize::Quadrant)
            .style(Style::new().fg(theme::GREEN))
            .lines(vec!["NITYANAND_THAKUR".into()])
            .build();
        frame.render_widget(banner, rows[1]);
    } else {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "NITYANAND THAKUR",
                Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD),
            ))),
            rows[1],
        );
    }

    let key =
        |name: &'static str| Span::styled(format!("  {name:<10}"), Style::new().fg(theme::DIM));
    let value = |text: String| Span::styled(text, Style::new().fg(theme::FG));
    // Keep the status line to a single row on tiny terminals.
    let counts = if inner.width >= 60 {
        format!(
            "  ({} repos · {} posts)",
            state.repos.len(),
            state.posts.len()
        )
    } else {
        String::new()
    };
    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  ✔ ", Style::new().fg(theme::GREEN)),
            Span::styled("profile loaded", Style::new().fg(theme::DIM)),
            Span::styled(counts, Style::new().fg(theme::FAINT)),
        ]),
        Line::default(),
        Line::from(vec![
            key("user"),
            value("Nityanand Thakur  (nnt)".to_string()),
        ]),
        Line::from(vec![
            key("role"),
            value("SRE · infrastructure · systems engineering".to_string()),
        ]),
        Line::from(vec![
            key("education"),
            value("B.Tech CSE @ VIT Vellore '28".to_string()),
        ]),
        Line::from(vec![
            key("focus"),
            value("kernels, BGP, and backend services in Go & Rust".to_string()),
        ]),
        Line::from(vec![
            key("leet/cf"),
            Span::styled(
                format!(
                    "{} solved · {} (max {})",
                    state.leetcode_problems, state.rating, state.max_rating
                ),
                Style::new().fg(theme::ORANGE),
            ),
        ]),
        Line::default(),
        Line::from(vec![
            Span::styled("  ● ", Style::new().fg(theme::GREEN)),
            Span::styled(
                "STATUS: ",
                Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled("available for work", Style::new().fg(theme::GREEN)),
        ]),
        Line::from(vec![
            Span::styled("  ↳ ", Style::new().fg(theme::FAINT)),
            Span::styled("press ", Style::new().fg(theme::DIM)),
            Span::styled(
                "b",
                Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " to read the blog — posts render right here",
                Style::new().fg(theme::DIM),
            ),
        ]),
    ])
    .wrap(Wrap { trim: false });
    frame.render_widget(info, rows[2]);
}

fn draw_about(frame: &mut Frame, area: Rect) {
    let block = theme::panel_with_hint("~/about", "cat about.txt");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // bio
            Constraint::Length(1), // rule
            Constraint::Min(0),    // skills
        ])
        .split(inner);

    let bio = Paragraph::new(
        "B.Tech CSE student @ VIT Vellore (Class of 2028). I build systems from the metal \
         up — compiling Linux kernels, wiring BGP, and shipping backend services in Go and \
         Rust. I run most of my own infrastructure, reverse-engineer consumer apps to \
         understand how they tick, and document the journey in writing. Off-keyboard: \
         fountain pens and cooking.",
    )
    .style(Style::new().fg(theme::FG))
    .wrap(Wrap { trim: true });
    frame.render_widget(bio, rows[0]);
    frame.render_widget(Paragraph::new(theme::rule(inner.width)), rows[1]);

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(rows[2]);

    draw_skill_column(frame, cols[0], "LANGUAGES", LANGUAGES);
    draw_skill_column(frame, cols[1], "INFRA", INFRA);
    draw_skill_column(frame, cols[2], "SECURITY", SECURITY);
}

fn draw_skill_column(frame: &mut Frame, area: Rect, label: &str, items: &[&str]) {
    let mut lines = vec![Line::from(vec![
        Span::styled("// ", Style::new().fg(theme::GREEN)),
        Span::styled(
            label.to_string(),
            Style::new().fg(theme::CYAN).add_modifier(Modifier::BOLD),
        ),
    ])];
    lines.push(Line::default());
    for item in items {
        lines.push(Line::from(vec![
            Span::styled("  ▪ ", Style::new().fg(theme::FAINT)),
            Span::styled(item.to_string(), Style::new().fg(theme::FG)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

fn draw_projects(frame: &mut Frame, area: Rect, view: &mut View, state: &AppState) {
    let hint = if state.repos.is_empty() {
        "loading…".to_string()
    } else {
        format!("{} pinned", state.repos.len())
    };
    let block = theme::panel_with_hint("~/projects", &hint);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    view.text_viewport = inner.height;

    let mut lines: Vec<Line<'static>> = vec![
        theme::prompt("~/projects", "cat projects.log"),
        Line::default(),
    ];

    if state.repos.is_empty() {
        lines.push(Line::from(Span::styled(
            "  ⠿  fetching pinned repositories from the GitHub API",
            Style::new().fg(theme::DIM),
        )));
        lines.push(Line::from(Span::styled(
            "     (set GT_TOKEN to a GitHub token if this stays empty)",
            Style::new().fg(theme::FAINT),
        )));
    } else {
        let desc_width = inner.width.saturating_sub(6) as usize;
        for repo in &state.repos {
            lines.push(Line::from(vec![
                Span::styled("  ▸ ", Style::new().fg(theme::GREEN)),
                Span::styled(
                    repo.name.clone(),
                    Style::new().fg(theme::CYAN).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    format!("★ {}", repo.stargazer_count),
                    Style::new().fg(theme::YELLOW),
                ),
            ]));

            if !repo.description.trim().is_empty() {
                let options = Options::new(desc_width.max(10))
                    .initial_indent("    ")
                    .subsequent_indent("    ");
                for line in wrap(&repo.description, options) {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::new().fg(theme::FG),
                    )));
                }
            }

            let language = repo
                .primary_language
                .as_ref()
                .map(|language| language.name.as_str())
                .unwrap_or("—");
            lines.push(Line::from(vec![
                Span::styled("    ", Style::new()),
                Span::styled(
                    language.to_string(),
                    Style::new().fg(theme::ORANGE).add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    repo.url.clone(),
                    Style::new()
                        .fg(theme::BLUE)
                        .add_modifier(Modifier::UNDERLINED),
                ),
            ]));
            lines.push(Line::default());
        }
    }

    let total = lines.len();
    let max_scroll = total.saturating_sub(inner.height as usize);
    let scroll = (view.projects_scroll as usize).min(max_scroll);
    view.projects_scroll = scroll as u16;

    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);

    if max_scroll > 0 {
        let mut state = ScrollbarState::new(max_scroll).position(scroll);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::new().fg(theme::FAINT))
                .thumb_style(Style::new().fg(theme::GREEN)),
            inner,
            &mut state,
        );
    }
}

fn draw_timeline(frame: &mut Frame, area: Rect, view: &mut View, state: &AppState) {
    let block = theme::panel_with_hint("~/timeline", "git log --oneline");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    view.text_viewport = inner.height;

    let items = [
        (
            "Jul 2024",
            "HEAD~5",
            "Leetcode Journey Begins",
            format!("Registered on Leetcode. Solved {} problems.", state.leetcode_problems),
        ),
        (
            "Oct 2024",
            "HEAD~4",
            "Codeforces Journey Begins",
            format!("Current rating {}. Max rating {}.", state.rating, state.max_rating),
        ),
        (
            "Nov 2025",
            "HEAD~3",
            "Maithili TTS Models",
            "Failed attempt to fine-tuned MMS-TTS for Maithili.".to_string(),
        ),
        (
            "Jan 2026",
            "HEAD~2",
            "Self-Hosting & Blogging",
            "Launched blog.lsnnt.dev. Published custom DDns Script in Rust and Vande Bharat train data extraction write-ups."
                .to_string(),
        ),
        (
            "Jun 2026",
            "HEAD~1",
            "Systems Engineering Deep Dive",
            "Compiled Linux kernel + BusyBox webserver. Updated ntranscrybe (Rust) for sub-second macOS captions."
                .to_string(),
        ),
    ];

    let mut lines: Vec<Line<'static>> = vec![
        Line::from(Span::styled(
            "  COMMIT_HISTORY",
            Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD),
        )),
        Line::default(),
    ];

    for (index, (date, head, title, description)) in items.iter().enumerate() {
        let is_last = index + 1 == items.len();
        let rail = if is_last { "○" } else { "●" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {rail} "), Style::new().fg(theme::GREEN)),
            Span::styled(
                format!("[{date}] {head}"),
                Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD),
            ),
        ]));
        let bar = if is_last { " " } else { "│" };
        lines.push(Line::from(vec![
            Span::styled(format!("  {bar} "), Style::new().fg(theme::FAINT)),
            Span::styled(
                title.to_string(),
                Style::new().fg(theme::FG).add_modifier(Modifier::BOLD),
            ),
        ]));
        for line in textwrap::wrap(description, inner.width.saturating_sub(8) as usize) {
            lines.push(Line::from(vec![
                Span::styled(format!("  {bar} "), Style::new().fg(theme::FAINT)),
                Span::styled(line.to_string(), Style::new().fg(theme::DIM)),
            ]));
        }
        lines.push(Line::default());
    }

    let total = lines.len();
    let max_scroll = total.saturating_sub(inner.height as usize);
    let scroll = (view.timeline_scroll as usize).min(max_scroll);
    view.timeline_scroll = scroll as u16;

    frame.render_widget(Paragraph::new(lines).scroll((scroll as u16, 0)), inner);

    if max_scroll > 0 {
        let mut scrollbar = ScrollbarState::new(max_scroll).position(scroll);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::new().fg(theme::FAINT))
                .thumb_style(Style::new().fg(theme::GREEN)),
            inner,
            &mut scrollbar,
        );
    }
}

fn draw_contact(frame: &mut Frame, area: Rect) {
    let block = theme::panel_with_hint("~/contact", "ready to connect");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // label
            Constraint::Length(3), // email
            Constraint::Length(1), // label
            Constraint::Length(3), // gpg
            Constraint::Length(1), // label
            Constraint::Min(6),    // socials
        ])
        .split(inner);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  # best way to reach me",
            Style::new().fg(theme::DIM),
        ))),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(theme::prompt("~", "mail tnityanand523@gmail.com"))
            .block(theme::panel("mail"))
            .style(Style::new().bg(theme::BG)),
        rows[1],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  # private messages, encrypted",
            Style::new().fg(theme::DIM),
        ))),
        rows[2],
    );
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  gpg --auto-key-locate keyserver --locate-keys tnityanand523@gmail.com",
            Style::new().fg(theme::GREEN),
        )))
        .block(theme::panel("gpg"))
        .style(Style::new().bg(theme::BG)),
        rows[3],
    );

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "  # find me elsewhere",
            Style::new().fg(theme::DIM),
        ))),
        rows[4],
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[5]);

    let top = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(rows[0]);
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(rows[1]);

    let socials = [
        ("GITHUB", "github.com/lsnnt"),
        ("LINKEDIN", "in.linkedin.com/in/lsnnt"),
        ("BLOG", "blog.lsnnt.dev"),
        ("HUGGINGFACE", "huggingface.co/lsnnt"),
        ("CODEFORCES", "codeforces.com/profile/lsnnt"),
        ("LEETCODE", "leetcode.com/u/governer/"),
    ];
    let areas = [top[0], top[1], top[2], bottom[0], bottom[1], bottom[2]];

    for ((name, link), area) in socials.iter().zip(areas) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(theme::FAINT))
            .style(Style::new().bg(theme::BG));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("[ {name} ]"),
                    Style::new().fg(theme::CYAN).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    theme::truncate(link, inner.width as usize),
                    Style::new().fg(theme::BLUE),
                )),
            ])
            .alignment(Alignment::Center),
            inner,
        );
    }
}

// ---------------------------------------------------------------------------
// blog: post list
// ---------------------------------------------------------------------------

fn draw_blogs(frame: &mut Frame, area: Rect, view: &mut View, state: &AppState) {
    let posts = &state.posts;
    let hint = if posts.is_empty() {
        "syncing…".to_string()
    } else {
        format!("{} posts · ⏎ read", posts.len())
    };
    let block = theme::panel_with_hint("~/blog", &hint);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if posts.is_empty() {
        let message = vec![
            Line::default(),
            Line::from(Span::styled(
                format!("  ⠿  GET {}", mblog::SITEMAP_URL),
                Style::new().fg(theme::DIM),
            )),
            Line::from(Span::styled(
                "     fetching posts and their markdown bodies…",
                Style::new().fg(theme::FAINT),
            )),
        ];
        frame.render_widget(Paragraph::new(message), inner);
        return;
    }

    view.blog.clamp_selection(posts.len());
    let item_width = inner.width.saturating_sub(2) as usize; // highlight symbol column
    let items: Vec<ListItem<'static>> = posts
        .iter()
        .enumerate()
        .map(|(index, post)| blog_list_item(index, post, item_width))
        .collect();

    let list = List::new(items)
        .highlight_symbol(Span::styled(
            "▌",
            Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD),
        ))
        .highlight_style(Style::new().bg(theme::SURFACE).add_modifier(Modifier::BOLD))
        .style(Style::new().bg(theme::BG));

    frame.render_stateful_widget(list, inner, &mut view.blog.list);

    if posts.len() > 1 {
        let mut scrollbar =
            ScrollbarState::new(posts.len()).position(view.blog.selected(posts.len()));
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::new().fg(theme::FAINT))
                .thumb_style(Style::new().fg(theme::GREEN)),
            inner,
            &mut scrollbar,
        );
    }
}

fn blog_list_item(index: usize, post: &BlogPost, width: usize) -> ListItem<'static> {
    let number = format!("{:02} ", index + 1);
    let date = post.date.clone();
    let title_width = width
        .saturating_sub(number.len() + date.chars().count() + 3)
        .max(8);
    let title = theme::truncate(&post.title, title_width);
    let pad = width
        .saturating_sub(number.chars().count() + title.chars().count() + date.chars().count() + 1)
        .max(1);

    let head = Line::from(vec![
        Span::styled(number, Style::new().fg(theme::FAINT)),
        Span::styled(title, Style::new().fg(theme::FG)),
        Span::raw(" ".repeat(pad)),
        Span::styled(date, Style::new().fg(theme::GREEN)),
    ]);

    let meta = Line::from(vec![
        Span::raw("    "),
        Span::styled("○ ", Style::new().fg(theme::DIM)),
        Span::styled(
            format!("{} min read · {} words", post.minutes, compact(post.words)),
            Style::new().fg(theme::DIM),
        ),
    ]);

    let excerpt = Line::from(vec![
        Span::raw("    "),
        Span::styled(
            theme::truncate(&post.excerpt, width.saturating_sub(4)),
            Style::new().fg(theme::FAINT).add_modifier(Modifier::ITALIC),
        ),
    ]);

    ListItem::new(vec![head, meta, excerpt])
}

/// `1132` → `1.1k`
fn compact(count: usize) -> String {
    if count >= 1000 {
        format!("{:.1}k", count as f64 / 1000.0)
    } else {
        count.to_string()
    }
}

// ---------------------------------------------------------------------------
// blog: post reader
// ---------------------------------------------------------------------------

fn draw_post(frame: &mut Frame, area: Rect, view: &mut View, state: &AppState) {
    let Some(index) = view.blog.open else {
        return;
    };
    let Some(post) = state.posts.get(index) else {
        // The post list was refreshed while reading: fall back to the list.
        view.blog.close();
        return;
    };

    // The reader's inner area does not depend on the panel title, so lay it out
    // first: the body has to be re-flowed *before* the scroll progress (which
    // lives in the title) can be computed.
    let inner = theme::panel("").inner(area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // command
            Constraint::Length(1), // meta
            Constraint::Length(1), // rule
            Constraint::Min(1),    // body
        ])
        .split(inner);

    let body = rows[3];
    view.blog.viewport = body.height;
    view.blog.refresh(post, body.width);

    let hint = format!("{}% · {} min", view.blog.progress(), post.minutes);
    frame.render_widget(theme::panel_with_hint(&post.title, &hint), area);

    frame.render_widget(
        Paragraph::new(theme::prompt("~/blog", &format!("cat {}.md", post.slug))),
        rows[0],
    );

    // Meta line: keep the permalink inside the panel instead of letting it run
    // into the border.
    let date = format!("  ○ {}  ", post.date);
    let stats = format!(
        "{} min read · {} words · {} lines  ",
        post.minutes,
        compact(post.words),
        view.blog.lines.len()
    );
    let used = date.chars().count() + stats.chars().count();
    let url = theme::truncate(
        &post.url,
        (inner.width as usize).saturating_sub(used).max(4),
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                date,
                Style::new().fg(theme::GREEN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(stats, Style::new().fg(theme::DIM)),
            Span::styled(
                url,
                Style::new()
                    .fg(theme::BLUE)
                    .add_modifier(Modifier::UNDERLINED),
            ),
        ])),
        rows[1],
    );
    frame.render_widget(Paragraph::new(theme::rule(inner.width)), rows[2]);

    let max_scroll = view.blog.max_scroll();
    let scroll = view.blog.post_scroll;
    frame.render_widget(
        Paragraph::new(view.blog.lines.as_slice()).scroll((scroll, 0)),
        body,
    );

    if max_scroll > 0 {
        let mut scrollbar = ScrollbarState::new(max_scroll as usize).position(scroll as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::new().fg(theme::FAINT))
                .thumb_style(Style::new().fg(theme::GREEN)),
            body,
            &mut scrollbar,
        );
    }
}

// ---------------------------------------------------------------------------
// footer
// ---------------------------------------------------------------------------

fn draw_footer(frame: &mut Frame, area: Rect, view: &View, state: &AppState) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(theme::FAINT))
        .style(Style::new().bg(theme::BG));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let wide = inner.width >= WIDE_FOOTER;
    let mut spans: Vec<Span<'static>> = Vec::new();
    if view.page == Page::Blog && view.blog.is_reading() {
        spans.extend(theme::key_hint("j / k", if wide { "scroll" } else { "" }));
        spans.extend(theme::key_hint("space", if wide { "page" } else { "" }));
        spans.extend(theme::key_hint(
            "g / G",
            if wide { "top / end" } else { "" },
        ));
        spans.extend(theme::key_hint("⌫", if wide { "back to list" } else { "" }));
        spans.extend(theme::key_hint("q", if wide { "quit" } else { "" }));
    } else if view.page == Page::Blog {
        spans.extend(theme::key_hint("j / k", if wide { "navigate" } else { "" }));
        spans.extend(theme::key_hint("↵", if wide { "read post" } else { "" }));
        spans.extend(theme::key_hint(
            "g / G",
            if wide { "top / end" } else { "" },
        ));
        spans.extend(theme::key_hint("q", if wide { "quit" } else { "" }));
    } else {
        for page in Page::ALL {
            let label = if wide {
                format!("[{}] {}", page.key(), page.label())
            } else {
                format!("[{}]", page.key())
            };
            spans.push(theme::chip(&label, page == view.page));
            spans.push(Span::raw(" "));
        }
        spans.extend(theme::key_hint("j / k", if wide { "scroll" } else { "" }));
        spans.extend(theme::key_hint("q", if wide { "quit" } else { "" }));
    }

    let status = if view.page == Page::Blog && view.blog.is_reading() {
        format!("{}% ", view.blog.progress())
    } else if view.page == Page::Blog && !state.posts.is_empty() {
        format!(
            "{}/{} ",
            view.blog.selected(state.posts.len()) + 1,
            state.posts.len()
        )
    } else {
        format!("sync {} ", state.last_update)
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(20),
            Constraint::Length(status.chars().count() as u16 + 1),
        ])
        .split(inner);

    frame.render_widget(Paragraph::new(Line::from(spans)), cols[0]);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            status,
            Style::new().fg(theme::DIM),
        )))
        .alignment(Alignment::Right),
        cols[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libs::markdown;
    use crate::models::githubstructs::{Language, Repo};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn demo_state(posts: usize) -> AppState {
        let mut state = AppState {
            rating: 1234,
            max_rating: 1400,
            leetcode_problems: 186,
            revision: 7,
            last_update: "12:00Z".to_string(),
            ..AppState::default()
        };
        state.posts = (0..posts)
            .map(|index| BlogPost {
                slug: format!("post-{index}"),
                title: format!("Post number {index}"),
                date: "01 Jul 2026".to_string(),
                url: format!("https://blog.lsnnt.dev/post-{index}"),
                words: 1180,
                minutes: 6,
                excerpt: "A short teaser of the post body.".to_string(),
                blocks: markdown::parse(
                    "# Post\n\nHello **world**, and a paragraph long enough to need \
                     re-flowing into whatever width the terminal happens to have.\n\n\
                     ```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\n- one\n- two\n",
                ),
            })
            .collect();
        state.repos = vec![Repo {
            name: "sshportfolio".to_string(),
            description: "A portfolio you visit over ssh.".to_string(),
            stargazer_count: 3,
            url: "https://github.com/lsnnt/sshportfolio".to_string(),
            primary_language: Some(Language {
                name: "Rust".to_string(),
            }),
        }];
        state
    }

    /// Renders one frame and flattens the buffer back into text.
    fn screen(view: &mut View, state: &AppState, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| draw_ui(frame, view, state))
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn every_page_draws_at_every_supported_size() {
        let state = demo_state(5);
        for (width, height) in [(40u16, 12u16), (80, 24), (120, 40), (200, 60)] {
            for page in Page::ALL {
                let mut view = View {
                    page,
                    ..View::default()
                };
                let rendered = screen(&mut view, &state, width, height);
                assert!(
                    rendered.trim().len() > 20,
                    "{page:?} rendered nothing at {width}x{height}"
                );
            }
        }
    }

    #[test]
    fn blog_list_shows_posts_metadata_and_selection_gutter() {
        let state = demo_state(4);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        let rendered = screen(&mut view, &state, 100, 30);
        assert!(rendered.contains("Post number 0"), "{rendered}");
        assert!(rendered.contains("▌"), "no selection gutter:\n{rendered}");
        assert!(rendered.contains("01 Jul 2026"), "{rendered}");
        assert!(rendered.contains("6 min read"), "{rendered}");
    }

    #[test]
    fn selected_post_is_the_one_that_opens() {
        let state = demo_state(4);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        view.blog.list.select(Some(2));
        view.blog.open_selected(state.posts.len());
        let rendered = screen(&mut view, &state, 100, 30);
        assert!(rendered.contains("cat post-2.md"), "{rendered}");
        assert!(rendered.contains("Hello world"), "{rendered}");
        assert!(rendered.contains("fn main()"), "{rendered}");
        assert!(
            rendered.contains("├") || rendered.contains("╭"),
            "{rendered}"
        );
    }

    #[test]
    fn narrow_home_still_shows_the_profile_facts() {
        let state = demo_state(2);
        let mut view = View::default();
        let rendered = screen(&mut view, &state, 60, 20);
        assert!(rendered.contains("NITYANAND THAKUR"), "{rendered}");
        // The one-line banner must not eat the rows the facts need.
        assert!(rendered.contains("education"), "{rendered}");
        assert!(rendered.contains("B.Tech"), "{rendered}");
    }

    #[test]
    fn tiny_terminals_get_a_hint_instead_of_a_panic() {
        let state = demo_state(3);
        let mut view = View::default();
        let rendered = screen(&mut view, &state, 20, 5);
        assert!(rendered.contains("terminal too small"), "{rendered}");
    }

    #[test]
    fn empty_state_renders_loading_placeholders() {
        let state = AppState::default();
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        let rendered = screen(&mut view, &state, 100, 30);
        assert!(rendered.contains("sitemap.xml"), "{rendered}");

        view.page = Page::Projects;
        let rendered = screen(&mut view, &state, 100, 30);
        assert!(rendered.contains("pinned repositories"), "{rendered}");
    }

    #[test]
    fn stale_open_index_falls_back_to_the_list() {
        let state = demo_state(1);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        view.blog.open = Some(99);
        let rendered = screen(&mut view, &state, 100, 30);
        assert!(!view.blog.is_reading());
        assert!(rendered.contains("Post number 0"), "{rendered}");
    }

    #[test]
    fn a_client_only_redraws_when_the_data_changes() {
        let state = demo_state(2);
        let backend = TestBackend::new(80, 24);
        let terminal = Terminal::new(backend).expect("test terminal");
        let mut client = ClientState {
            terminal,
            view: View::default(),
            input: Vec::new(),
            drawn_revision: u64::MAX,
        };
        draw_client(&mut client, &state);
        assert_eq!(client.drawn_revision, state.revision);

        // Already up to date: `redraw_all` must leave it alone.
        let mut clients = HashMap::new();
        clients.insert(1usize, client);
        redraw_all(&mut clients, &state);
        assert_eq!(clients[&1].drawn_revision, state.revision);
    }
}
