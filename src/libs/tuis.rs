use std::collections::HashMap;
use ratatui::backend::Backend;
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::prelude::{Color, Line, Modifier, Span, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use tui_big_text::{BigText, PixelSize};
use crate::{AppState, ClientState, Page, TimelineItem};

const LANGUAGES: &[&str] = &["Go", "Rust", "Python", "C++ / Java"];
const INFRA: &[&str] = &["Linux Kernel", "Docker", "AWS S3", "Wireguard / Bird"];
const SECURITY: &[&str] = &["Reverse Engineering", "PCAPDroid", "Burp Suite", "Frida"];

pub fn draw_about(frame: &mut Frame, area: Rect) {
    let card = Block::default()
        .title(" root@nnt: ~ ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = card.inner(area);
    frame.render_widget(card, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // $ cat about.txt
            Constraint::Length(2), // > SYSTEM_INFO
            Constraint::Length(1), // divider
            Constraint::Length(5), // bio paragraph
            Constraint::Length(1), // spacer
            Constraint::Min(0),    // skill columns
        ])
        .split(inner);

    let cmd = Paragraph::new(Line::from(Span::styled(
        "$ cat about.txt",
        Style::new().fg(Color::Gray),
    )));
    frame.render_widget(cmd, rows[0]);

    let heading = Paragraph::new(Line::from(vec![
        Span::styled("> ", Style::new().fg(Color::Green)),
        Span::styled(
            "SYSTEM_INFO",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
    ]));
    frame.render_widget(heading, rows[1]);

    let divider = Paragraph::new(Line::from(Span::styled(
        "─".repeat(rows[2].width as usize),
        Style::new().fg(Color::DarkGray),
    )));
    frame.render_widget(divider, rows[2]);

    let bio = Paragraph::new(
        "B.Tech CSE student @ VIT Vellore (Class of 2028). I build systems from the metal \
         up — compiling Linux kernels, wiring BGP, and shipping backend services in Go and \
         Rust. I run most of my own infrastructure, reverse-engineer consumer apps to \
         understand how they tick, and document the journey in writing. Off-keyboard: \
         fountain pens and cooking.",
    )
        .style(Style::new().fg(Color::White))
        .wrap(Wrap { trim: true });
    frame.render_widget(bio, rows[3]);

    draw_skill_columns(frame, rows[5]);
}

pub fn draw_skill_columns(frame: &mut Frame, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(34),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(area);

    draw_skill_column(frame, cols[0], "LANGUAGES", LANGUAGES);
    draw_skill_column(frame, cols[1], "INFRA", INFRA);
    draw_skill_column(frame, cols[2], "SECURITY", SECURITY);
}

pub fn draw_skill_column(frame: &mut Frame, area: Rect, label: &str, items: &[&str]) {
    let mut lines = vec![Line::from(vec![
        Span::styled("// ", Style::new().fg(Color::Green)),
        Span::styled(
            label.to_string(),
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
    ])];
    lines.push(Line::from(""));
    for item in items {
        lines.push(Line::from(Span::styled(
            item.to_string(),
            Style::new().fg(Color::Gray),
        )));
    }

    frame.render_widget(Paragraph::new(lines), area);
}


pub fn draw_projects(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .title("Projects")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        "$ cat projects.log",
        Style::new().fg(Color::Gray),
    )));
    lines.push(Line::default());

    if state.repos.is_empty() {
        lines.push(Line::from(Span::styled(
            "Loading repositories...",
            Style::new().fg(Color::DarkGray),
        )));
    } else {
        for repo in &state.repos {
            lines.push(Line::from(vec![
                Span::styled("▶ ", Style::new().fg(Color::Yellow)),
                Span::styled(&repo.name, Style::new().fg(Color::Cyan).bold()),
            ]));

            lines.push(Line::from(Span::styled(
                format!("  {}", repo.description),
                Style::new().fg(Color::White),
            )));

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  ★ {}", repo.stargazer_count),
                    Style::new().fg(Color::Yellow),
                ),
                Span::raw("  "),
                Span::styled(
                    &repo.url,
                    Style::new().fg(Color::Blue).underlined(),
                ),
            ]));

            lines.push(Line::default());
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}
pub fn redraw_all<B: Backend>(clients: &mut HashMap<usize, ClientState<B>>, state: &AppState) {
    for (_, client) in clients.iter_mut() {
        let page = client.page;
        let _ = client.terminal.draw(|frame| {
            draw_ui(frame, page,&state);
        });
    }
}

pub fn draw_ui(frame: &mut Frame, page: Page, state : &AppState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // top nav bar
            Constraint::Min(0),    // body
            Constraint::Length(3), // keybinding footer
        ])
        .split(area);

    draw_nav(frame, chunks[0], page);
    draw_body(frame, chunks[1], page, state);
    draw_footer(frame, chunks[2], page);
}

// ---------- nav bar ----------

pub fn nav_span(label: &'static str, active: bool) -> Span<'static> {
    let style = if active {
        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::White)
    };
    Span::styled(label, style)
}

pub fn draw_nav(frame: &mut Frame, area: Rect, page: Page) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    let left = Paragraph::new(Line::from(vec![
        Span::styled("[ root@nnt ]", Style::new().fg(Color::Green)),
        Span::styled(": /#", Style::new().fg(Color::White)),
    ]));
    frame.render_widget(left, cols[0]);

    let right = Paragraph::new(Line::from(vec![
        nav_span("~/projects", page == Page::Projects),
        Span::raw("   "),
        nav_span("~/timeline", page == Page::Timeline),
        Span::raw("   "),
        nav_span("~/blog", page == Page::Blog),
        Span::raw("   "),
        nav_span("contact", page == Page::Contact),
    ]))
        .alignment(Alignment::Right);
    frame.render_widget(right, cols[1]);
}

// ---------- body ----------

pub fn draw_body(frame: &mut Frame, area: Rect, page: Page, state : &AppState) {
    match page {
        Page::Home => draw_home(frame, area),
        Page::About => draw_about(frame, area),
        Page::Projects => draw_projects(frame, area,state),
        Page::Timeline => draw_timeline(frame, area,state),
        Page::Blog => draw_blogs(frame, area,state),
        Page::Contact => draw_contact(frame, area),
    }
}
pub fn draw_contact(frame: &mut Frame, area: Rect) {
    let outer = Block::default()
        .title("contact.sh")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));

    let inner = outer.inner(area);
    frame.render_widget(outer, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // READY_TO_CONNECT
            Constraint::Length(1), // label
            Constraint::Length(4), // email
            Constraint::Length(1), // label
            Constraint::Length(5), // gpg
            Constraint::Length(1), // label
            Constraint::Min(8),    // socials
        ])
        .split(inner);

    // ================= Header =================

    let header = Paragraph::new(Line::from(Span::styled(
        "[ READY_TO_CONNECT ]",
        Style::new()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )))
        .alignment(ratatui::layout::Alignment::Center);

    frame.render_widget(header, chunks[0]);

    // ================= Email =================

    frame.render_widget(
        Paragraph::new("# Best way to reach me:")
            .style(Style::new().fg(Color::Gray)),
        chunks[1],
    );

    let email_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Green));

    let email_inner = email_block.inner(chunks[2]);
    frame.render_widget(email_block, chunks[2]);

    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("root@nnt:~$ ", Style::new().fg(Color::DarkGray)),
            Span::styled(
                "mail tnityanand523@gmail.com",
                Style::new().fg(Color::Green),
            ),
        ])),
        email_inner,
    );

    // ================= GPG =================

    frame.render_widget(
        Paragraph::new("# Use GPG for private messages:")
            .style(Style::new().fg(Color::Gray)),
        chunks[3],
    );

    let gpg_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::Green));

    let gpg_inner = gpg_block.inner(chunks[4]);
    frame.render_widget(gpg_block, chunks[4]);

    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("root@nnt:~$ ", Style::new().fg(Color::DarkGray)),
                Span::styled(
                    "gpg --auto-key-locate keyserver --locate-keys tnityanand523@gmail.com",
                    Style::new().fg(Color::Green),
                ),
            ]),
        ]),
        gpg_inner,
    );

    // ================= Social Links =================

    frame.render_widget(
        Paragraph::new("# Find me elsewhere:")
            .style(Style::new().fg(Color::Gray)),
        chunks[5],
    );

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[6]);

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
        ("GITHUB", "https://github.com/lsnnt"),
        ("LINKEDIN", "https://in.linkedin.com/in/lsnnt"),
        ("BLOG", "https://blog.lsnnt.dev/"),
        ("HUGGINGFACE", "https://huggingface.co/lsnnt"),
        ("CODEFORCES", "https://codeforces.com/profile/lsnnt"),
        ("LEETCODE", "https://leetcode.com/u/governer/"),
    ];

    let areas = [
        top[0],
        top[1],
        top[2],
        bottom[0],
        bottom[1],
        bottom[2],
    ];

    for ((name, link), area) in socials.iter().zip(areas) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::new().fg(Color::DarkGray));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    format!("[ {} ]", name),
                    Style::new().fg(Color::White).bold(),
                )),
                Line::from(Span::styled(
                    *link,
                    Style::new().fg(Color::Blue),
                )),
            ])
                .alignment(Alignment::Center),
            inner,
        );
    }
}
pub fn draw_blogs(frame: &mut Frame, area: Rect, state: &AppState) {
    let block = Block::default()
        .title("Blogs")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        "$ ls ~/blogs",
        Style::new().fg(Color::Gray),
    )));
    lines.push(Line::default());

    if state.blogs.is_empty() {
        lines.push(Line::from(Span::styled(
            "No blogs available.",
            Style::new().fg(Color::DarkGray),
        )));
    } else {
        for blog in &state.blogs {
            lines.push(Line::from(vec![
                Span::styled("▶ ", Style::new().fg(Color::Yellow)),
                Span::styled(&blog.title, Style::new().fg(Color::Cyan).bold()),
            ]));

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  📅 {}", blog.date),
                    Style::new().fg(Color::Green),
                ),
            ]));

            lines.push(Line::from(vec![
                Span::styled("  ", Style::new()),
                Span::styled(
                    &blog.link,
                    Style::new().fg(Color::Blue).underlined(),
                ),
            ]));

            lines.push(Line::default());
        }
    }

    frame.render_widget(Paragraph::new(lines), inner);
}
pub fn draw_home(frame: &mut Frame, area: Rect) {
    let card = Block::default()
        .title(" root@nnt: ~ ")
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = card.inner(area);
    frame.render_widget(card, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // $ ./init_profile.sh
            Constraint::Length(8), // big title
            Constraint::Min(0),    // info lines
        ])
        .split(inner);

    let cmd = Paragraph::new(Line::from(Span::styled(
        "$ ./init_profile.sh",
        Style::new().fg(Color::Gray),
    )));
    frame.render_widget(cmd, rows[0]);

    let big_text = BigText::builder()
        .pixel_size(PixelSize::Quadrant)
        .style(Style::new().fg(Color::White))
        .lines(vec!["NITYANAND_THAKUR".into()])
        .build();
    // Note: some tui-big-text versions return a Result from build() —
    // if yours does, add `.unwrap()` (or `?` in a function returning Result).
    frame.render_widget(big_text, rows[1]);

    let info = Paragraph::new(vec![
        line_green("> Loading profile..."),
        line_green("> User: nnt"),
        line_green("> Role: SRE / Infra / Security-leaning Systems Engineer"),
        line_green("> Education: B.Tech CSE @ VIT Vellore '28"),
        line_green(
            "> Focus: Building systems from the metal up — kernels, BGP, and backend services.",
        ),
        Line::from(vec![
            Span::styled("> Status: ", Style::new().fg(Color::Green)),
            Span::styled(
                "[ ONLINE ]",
                Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" Available for work", Style::new().fg(Color::Green)),
        ]),
    ])
        .wrap(Wrap { trim: true });
    frame.render_widget(info, rows[2]);
}

pub fn line_green(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(text, Style::new().fg(Color::Green)))
}
fn draw_footer(frame: &mut Frame, area: Rect, page: Page) {
    let hint = |label: &'static str, key: char, active: bool| {
        let style = if active {
            Style::new()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::White)
        };
        Span::styled(format!(" [{key}] {label} "), style)
    };

    let line = Line::from(vec![
        hint("home", 'h', page == Page::Home),
        Span::raw(" "),
        hint("projects", 'p', page == Page::Projects),
        Span::raw(" "),
        hint("about", 'a', page == Page::About),
        Span::raw(" "),
        hint("timeline", 't', page == Page::Timeline),
        Span::raw(" "),
        hint("blog", 'b', page == Page::Blog),
        Span::raw(" "),
        hint("contact", 'c', page == Page::Contact),
    ]);

    let footer = Paragraph::new(line)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    frame.render_widget(footer, area);
}
pub fn draw_timeline(frame: &mut Frame, area: Rect, state: &AppState) {
    let items = vec![
        TimelineItem {
            date: "Jul 2024",
            head: "HEAD~5",
            title: "Leetcode Journey Begins",
            desc: format!("Registered on Leetcode. Solved {} problems.",state.leetcode_problems),
        },
        TimelineItem {
            date: "Oct 2024",
            head: "HEAD~4",
            title: "Codeforces Journey Begins",
            desc: format!("Current rating {}. Max rating {}.",state.rating, state.max_rating),
        },
        TimelineItem {
            date: "Nov 2025",
            head: "HEAD~3",
            title: "Maithili TTS Models",
            desc: "Failed attempt to fine-tuned MMS-TTS for Maithili.".to_string(),
        },
        TimelineItem {
            date: "Jan 2026",
            head: "HEAD~2",
            title: "Self-Hosting & Blogging",
            desc: "Launched blog.lsnnt.dev. Published custom DDns Script in Rust and Vande Bharat train data extraction write-ups.".to_string()
        },
        TimelineItem {
            date: "Jun 2026",
            head: "HEAD~1",
            title: "Systems Engineering Deep Dive",
            desc: "Compiled Linux kernel + BusyBox webserver. Updated ntranscrybe (Rust) for sub-second macOS captions.".to_string()
        }
    ];

    let block = Block::default()
        .title(" git log --oneline ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = Vec::new();

    lines.push(Line::from(Span::styled(
        "> COMMIT_HISTORY",
        Style::default()
            .fg(Color::Green)
            .add_modifier(Modifier::BOLD),
    )));

    lines.push(Line::default());

    for item in items {
        lines.push(Line::from(vec![
            Span::styled("● ", Style::default().fg(Color::Green)),
            Span::styled(
                format!("[{}] {}", item.date, item.head),
                Style::default().fg(Color::Green),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::raw("│ "),
            Span::styled(
                item.title,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("│ ", Style::default().fg(Color::Green)),
            Span::styled(item.desc, Style::default().fg(Color::DarkGray)),
        ]));

        lines.push(Line::from(vec![
            Span::styled("│", Style::default().fg(Color::Green)),
        ]));

        lines.push(Line::default());
    }

    let paragraph = Paragraph::new(lines);

    frame.render_widget(paragraph, inner);

}