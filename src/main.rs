mod libs;
mod models;

use crate::libs::getpinnedrepo::get_pinned_repo;
use crate::libs::lcapi::lcapi;
use crate::libs::mblog::getblog;
use crate::libs::rating::rating;
use crate::libs::tuis::{draw_ui, redraw_all};
use crate::models::githubstructs::Repo;
use crate::models::hblogs::Blogs;
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};
use reqwest::Client;
use russh::keys::ssh_key::PublicKey;
use russh::server::*;
use russh::{Channel, ChannelId, Pty};
use std::collections::HashMap;
use std::sync::{Arc, LazyLock};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::sync::{Mutex, RwLock};
use tokio::try_join;

/// Which section of the site a client is currently viewing.
///
type SharedState = Arc<RwLock<AppState>>;
pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .build().unwrap()
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

pub struct AppState {
    pub repos: Vec<Repo>,
    pub rating: u16,
    pub max_rating: u16,
    pub leetcode_problems: u16,
    pub blogs: Vec<Blogs>,
}


struct TimelineItem<'a> {
    date: &'a str,
    head: &'a str,
    title: &'a str,
    desc: String,
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
    state : SharedState,
    id: usize,
}

impl AppServer {
    pub fn new(state: SharedState) -> Self {
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            state,
            id: 0,
        }
    }

    pub async fn run(&mut self) -> Result<(), anyhow::Error> {
        let clients = self.clients.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_millis(250));
            loop {
                interval.tick().await;
                let state = state.read().await;

                let mut clients = clients.lock().await;

                redraw_all(&mut clients, &state);
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

        self.run_on_address(Arc::new(config), ("0.0.0.0", 22))
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
                let state = self.state.clone();
                let mut clients = self.clients.lock().await;
                if let Some(client) = clients.get_mut(&self.id) {
                    handle_key(client, key as char);

                    // Optional: redraw immediately
                    let page = client.page;
                    let state = state.read().await;
                    let _ = client.terminal.draw(|frame| {
                        draw_ui(frame, page, &state);
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

async fn update_loop(state: SharedState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(10*60 ));
    let client = reqwest::Client::new();
    loop {
        println!("Updating...");
        interval.tick().await;

        match try_join!(
        get_pinned_repo(&client),
        rating(&client),
        lcapi(&client),
        getblog(&client),
    ) {
            Ok((repos, (rating, max_rating), lc, blogs)) => {
                println!(
                    "repos={}, rating={}, max={}, lc={}, blogs={}",
                    repos.len(),
                    rating,
                    max_rating,
                    lc,
                    blogs.len()
                );
                let mut data = state.write().await;
                data.repos = repos;
                data.rating = rating;
                data.max_rating = max_rating;
                data.leetcode_problems = lc;
                data.blogs = blogs;
            }
            Err(e) => {
                eprintln!("update failed: {e}");
            }
        }
    }
}
#[tokio::main]
async fn main() {
    let state = Arc::new(RwLock::new(AppState {
        repos: vec![],
        rating: 0,
        max_rating: 0,
        leetcode_problems: 0,
        blogs: vec![],
    }));
    {
        let state_clone = state.clone();
        tokio::spawn(async move {
            update_loop(state_clone).await;
        });
    }
    let mut server = AppServer::new(state);
    server.run().await.expect("Failed running server");
}