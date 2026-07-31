mod libs;
mod models;

use std::collections::HashMap;
use std::fmt::format;
use std::sync::{Arc, LazyLock};
use once_cell::unsync::Lazy;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport};
use ratatui::text::{Line, Span};
use reqwest::Client;
use russh::keys::ssh_key::PublicKey;
use russh::server::*;
use russh::{Channel, ChannelId, Pty};
use tokio::sync::Mutex;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};

use tui_big_text::{BigText, PixelSize};
use crate::libs::lcapi::lcapi;
use crate::libs::rating::rating;

/// Which section of the site a client is currently viewing.
///

pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent("lsnnt")
        .build()
        .expect("Failed to build reqwest client")
});
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    About,
    Projects,
    Timeline,
    Blog,
    Contact,
}

/// Whatever you already store per SSH connection — extend it with `page`.
/// Swap the key type (`usize` below) for whatever you key `clients` on.
pub struct ClientState<B: Backend> {
    pub terminal: Terminal<B>,
    pub page: Page,
}

/// Call this from your existing key-byte handler for a connection.
pub fn handle_key<B: Backend>(state: &mut ClientState<B>, key: char) {
    state.page = match key {
        'a' | 'A' => Page::About,
        't' | 'T' => Page::Timeline,
        'P' | 'p' => Page::Projects,
        'b' | 'B' => Page::Blog,
        'c' | 'C' => Page::Contact,
        'h' | 'H' => Page::Home, // back to the hero landing screen
        _ => return, // unrecognized key, leave the page as-is
    };
}

/// This is your posted loop, filled in — redraw every connected client
/// using its own current page.
pub async fn redraw_all<B: Backend>(clients: &mut HashMap<usize, ClientState<B>>) {
    for (_, client) in clients.iter_mut() {
        let page = client.page;
        let _ = client.terminal.draw(|frame| {
            draw_ui(frame, page);
        });
    }
}

fn draw_ui(frame: &mut Frame, page: Page) {
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
    draw_body(frame, chunks[1], page);
    draw_footer(frame, chunks[2], page);
}

// ---------- nav bar ----------

fn nav_span(label: &'static str, active: bool) -> Span<'static> {
    let style = if active {
        Style::new().fg(Color::Green).add_modifier(Modifier::BOLD)
    } else {
        Style::new().fg(Color::White)
    };
    Span::styled(label, style)
}

fn draw_nav(frame: &mut Frame, area: Rect, page: Page) {
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

fn draw_body(frame: &mut Frame, area: Rect, page: Page) {
    match page {
        Page::Home => draw_home(frame, area),
        Page::About => draw_about(frame, area),
        Page::Projects => draw_projects(frame, area),
        Page::Timeline => draw_timeline(frame, area),
        Page::Blog => draw_placeholder(frame, area, "BLOG", "ls ~/blog"),
        Page::Contact => draw_placeholder(frame, area, "CONTACT", "./contact.sh"),
    }
}

fn draw_home(frame: &mut Frame, area: Rect) {
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

fn line_green(text: &'static str) -> Line<'static> {
    Line::from(Span::styled(text, Style::new().fg(Color::Green)))
}

const LANGUAGES: &[&str] = &["Go", "Rust", "Python", "C++ / Java"];
const INFRA: &[&str] = &["Linux Kernel", "Docker", "AWS S3", "Wireguard / Bird"];
const SECURITY: &[&str] = &["Reverse Engineering", "PCAPDroid", "Burp Suite", "Frida"];

fn draw_about(frame: &mut Frame, area: Rect) {
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

fn draw_skill_columns(frame: &mut Frame, area: Rect) {
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

fn draw_skill_column(frame: &mut Frame, area: Rect, label: &str, items: &[&str]) {
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

fn draw_placeholder(frame: &mut Frame, area: Rect, title: &str, hint: &str) {
    let block = Block::default()
        .title(format!(" {title} "))
        .borders(Borders::ALL)
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let body = Paragraph::new(vec![
        Line::from(Span::styled(format!("$ {hint}"), Style::new().fg(Color::Gray))),
        Line::from(""),
        Line::from(Span::styled(
            format!("> {title} content goes here."),
            Style::new().fg(Color::Green),
        )),
    ]);
    frame.render_widget(body, inner);
}
struct Project<'a> {
    name: &'a str,
    description: &'a str,
    stars: u32,
    language: &'a str,
    language_color: Color,
}
fn draw_projects(frame: &mut Frame, area: Rect) {
    let projects = [
        Project {
            name: "SSH Portfolio",
            description: "A portfolio accessible over SSH using Russh + Ratatui.",
            stars: 152,
            language: "Rust",
            language_color: Color::Magenta,
        },
        Project {
            name: "Spotify-banner-for-github",
            description: "An http server that returns a svg image of the last 20 songs listened on spotify (no client id,secret needed) (no spotify premium needed) ",
            stars: 8,
            language: "MicroPython",
            language_color: Color::Green,
        },
        Project {
            name: "MineStress",
            description: "Minecraft server stress-testing tool with configurable bots.",
            stars: 41,
            language: "Rust",
            language_color: Color::Magenta,
        },
    ];
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

    for project in projects {
        lines.push(Line::from(vec![
            Span::styled("▶ ", Style::new().fg(Color::Yellow)),
            Span::styled(project.name, Style::new().fg(Color::Cyan).bold()),
        ]));

        lines.push(Line::from(Span::styled(
            format!("  {}", project.description),
            Style::new().fg(Color::White),
        )));

        lines.push(Line::from(vec![
            Span::styled(
                format!("  ★ {}  ", project.stars),
                Style::new().fg(Color::Yellow),
            ),
            Span::styled(
                project.language,
                Style::new().fg(project.language_color),
            ),
        ]));

        lines.push(Line::default());
    }

    frame.render_widget(Paragraph::new(lines), inner);
}


struct TimelineItem<'a> {
    date: &'a str,
    head: &'a str,
    title: &'a str,
    desc: String,
}
fn draw_timeline(frame: &mut Frame, area: Rect) {
    let (cr,mr) = rating(&HTTP_CLIENT).unwrap_or((0,0));
    let items = vec![
        TimelineItem {
            date: "Jul 2024",
            head: "HEAD~5",
            title: "Leetcode Journey Begins",
            desc: format!("Registered on Leetcode. Solved {} problems.",lcapi(&HTTP_CLIENT).await.unwrap_or(0)),
        },
        TimelineItem {
            date: "Oct 2024",
            head: "HEAD~4",
            title: "Codeforces Journey Begins",
            desc: format!("Current rating {}. Max rating {}.",cr,mr),
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
// ---------- footer ----------

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


struct TerminalHandle {
    sender: UnboundedSender<Vec<u8>>,
    // The sink collects the data which is finally sent to sender.
    sink: Vec<u8>,
}

impl TerminalHandle {
    async fn start(handle: Handle, channel_id: ChannelId) -> Self {
        let (sender, mut receiver) = unbounded_channel::<Vec<u8>>();
        tokio::spawn(async move {
            while let Some(data) = receiver.recv().await {
                let result = handle.data(channel_id, data).await;
                if result.is_err() {
                    eprintln!("Failed to send data: {result:?}");
                }
            }
        });
        Self {
            sender,
            sink: Vec::new(),
        }
    }
}

// The crossterm backend writes to the terminal handle.
impl std::io::Write for TerminalHandle {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.sink.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let result = self.sender.send(self.sink.clone());
        if result.is_err() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                result.unwrap_err(),
            ));
        }

        self.sink.clear();
        Ok(())
    }
}

#[derive(Clone)]
struct AppServer {
    clients: Arc<Mutex<HashMap<usize, ClientState<CrosstermBackend<TerminalHandle>>>>>,
    id: usize,
}

impl AppServer {
    pub fn new() -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            id: 0,
        }
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
            loop {
                interval.tick().await;

                let mut clients = clients.lock().await;
                redraw_all(&mut clients).await;
            }
        });

        let config = Config {
            inactivity_timeout: Some(std::time::Duration::from_secs(3600)),
            auth_rejection_time: std::time::Duration::from_secs(3),
            auth_rejection_time_initial: Some(std::time::Duration::from_secs(0)),
            keys: vec![
                russh::keys::PrivateKey::random(&mut rand::rng(), ssh_key::Algorithm::Ed25519)?,
            ],
            nodelay: true,
            ..Default::default()
        };

        self.run_on_address(Arc::new(config), ("0.0.0.0", 2222))
            .await?;
        Ok(())
    }
}

impl Server for AppServer {
    type Handler = Self;
    fn new_client(&mut self, _: Option<std::net::SocketAddr>) -> Self {
        let s = self.clone();
        self.id += 1;
        s
    }
}

impl Handler for AppServer {
    type Error = anyhow::Error;
    async fn auth_password(&mut self, _user: &str, _password: &str) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }
    async fn auth_publickey(&mut self, _: &str, _: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let terminal_handle = TerminalHandle::start(session.handle(), channel.id()).await;

        let backend = CrosstermBackend::new(terminal_handle);

        // the correct viewport area will be set when the client request a pty
        let options = TerminalOptions {
            viewport: Viewport::Fixed(Rect::default()),
        };

        let terminal = Terminal::with_options(backend, options)?;

        let mut clients = self.clients.lock().await;
        clients.insert(
            self.id,
            ClientState {
                terminal,
                page: Page::Home,
            },
        );
        reply.accept().await;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        match data {
            b"q" => {
                self.clients.lock().await.remove(&self.id);
                session.close(channel)?;
            }

            &[key] => {
                let mut clients = self.clients.lock().await;

                if let Some(client) = clients.get_mut(&self.id) {
                    handle_key(client, key as char);

                    // Optional: redraw immediately
                    let page = client.page;
                    let _ = client.terminal.draw(|frame| {
                        draw_ui(frame, page);
                    });
                }
            }

            _ => {}
        }

        Ok(())
    }

    /// The client requests a pseudo-terminal with the given
    /// specifications.
    ///
    /// **Note:** Success or failure should be communicated to the client by calling
    /// `session.channel_success(channel)` or `session.channel_failure(channel)` respectively.
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _: &str,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let rect = Rect {
            x: 0,
            y: 0,
            width: col_width as u16,
            height: row_height as u16,
        };

        let mut clients = self.clients.lock().await;
        let terminal = clients.get_mut(&self.id).unwrap();
        terminal.terminal.resize(rect)?;

        session.channel_success(channel)?;

        Ok(())
    }

    /// The client's window size has changed.
    async fn window_change_request(
        &mut self,
        _: ChannelId,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        let rect = Rect {
            x: 0,
            y: 0,
            width: col_width as u16,
            height: row_height as u16,
        };

        let mut clients = self.clients.lock().await;
        let terminal = clients.get_mut(&self.id).unwrap();
        terminal.terminal.resize(rect)?;

        Ok(())
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        let id = self.id;
        let clients = self.clients.clone();
        tokio::spawn(async move {
            let mut clients = clients.lock().await;
            clients.remove(&id);
        });
    }
}

#[tokio::main]
async fn main() {
    let mut server = AppServer::new();
    server.run().await.expect("Failed running server");
}