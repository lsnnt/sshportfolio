//! An SSH-rendered portfolio.
//!
//! Every connection gets its own ratatui `Terminal` speaking over the SSH
//! channel; the data it renders (pinned repos, Codeforces/LeetCode stats, blog
//! posts) is refreshed in the background and shared through [`AppState`].

mod libs;
mod models;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, LazyLock};
use std::time::Duration;

use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::layout::Rect;
use ratatui::{Terminal, TerminalOptions, Viewport};
use reqwest::Client;
use russh::keys::PrivateKey;
use russh::keys::ssh_key::PublicKey;
use russh::server::{Auth, Config, Handle, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId, Pty};
use ssh_key::Algorithm::Ed25519;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex, RwLock};
use tokio::time::MissedTickBehavior;

use crate::libs::getpinnedrepo::get_pinned_repo;
use crate::libs::input::parse_keys;
use crate::libs::lcapi::lcapi;
use crate::libs::mblog::get_posts;
use crate::libs::rating::rating;
use crate::libs::theme;
use crate::libs::tuis::{draw_client, redraw_all};
use crate::libs::view::{self, View};
use crate::models::githubstructs::Repo;
use crate::models::hblogs::BlogPost;

/// Everything the pages read from, refreshed by [`update_loop`].
type SharedState = Arc<RwLock<AppState>>;

/// One HTTP client for the whole process (connection pooling + sane timeouts,
/// so a hung request can never wedge the refresh loop).
pub static HTTP_CLIENT: LazyLock<Client> = LazyLock::new(|| {
    Client::builder()
        .user_agent("sshportfolio/0.1 (+https://github.com/lsnnt/sshportfolio)")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20))
        .build()
        .expect("failed to build the http client")
});

/// How often the upstream data is refreshed.
const REFRESH_INTERVAL: Duration = Duration::from_secs(10 * 60);
/// How often clients are asked to redraw (only actually redraws when needed).
const REDRAW_INTERVAL: Duration = Duration::from_millis(250);
/// Grace period before an idle connection is dropped.
const INACTIVITY_TIMEOUT: Duration = Duration::from_secs(3600);

const LISTEN_ADDR: &str = "0.0.0.0:2222";

#[derive(Default)]
pub struct AppState {
    pub repos: Vec<Repo>,
    pub rating: u16,
    pub max_rating: u16,
    pub leetcode_problems: u16,
    pub posts: Vec<BlogPost>,
    /// Bumped whenever the data changes, so idle clients are not redrawn for
    /// nothing (see [`ClientState::drawn_revision`]).
    pub revision: u64,
    /// `HH:MMZ` of the last refresh.
    pub last_update: String,
}

/// Per-connection state.
pub struct ClientState<B: Backend> {
    pub terminal: Terminal<B>,
    pub view: View,
    /// Bytes received from the pty that do not form a complete key yet.
    pub input: Vec<u8>,
    /// [`AppState::revision`] of the last frame drawn for this client.
    pub drawn_revision: u64,
}

/// The sink that feeds the SSH channel.
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
                if let Err(error) = handle.data(channel_id, data).await {
                    log::debug!("failed to send data: {error:?}");
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
        // Move the accumulated frame out instead of copying it.
        let frame = std::mem::take(&mut self.sink);
        self.sender
            .send(frame)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::BrokenPipe, error.to_string()))
    }
}

#[derive(Clone)]
struct AppServer {
    clients: Arc<Mutex<HashMap<usize, ClientState<CrosstermBackend<TerminalHandle>>>>>,
    state: SharedState,
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
        self.spawn_redraw_loop();

        let config = Config {
            inactivity_timeout: Some(INACTIVITY_TIMEOUT),
            auth_rejection_time: Duration::from_secs(3),
            auth_rejection_time_initial: Some(Duration::from_secs(0)),
            keys: vec![PrivateKey::random(&mut rand::rng(), Ed25519)?],
            nodelay: true,
            ..Default::default()
        };

        self.run_on_address(Arc::new(config), LISTEN_ADDR).await?;
        Ok(())
    }

    /// A single task redraws clients whose view is stale. Clients that have not
    /// seen the current data revision are repainted; everything else is left
    /// alone, so an idle session costs nothing.
    fn spawn_redraw_loop(&self) {
        let clients = self.clients.clone();
        let state = self.state.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(REDRAW_INTERVAL);
            interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                // Keep the state lock short: copy what we need, release, draw.
                let state = state.read().await;
                let mut clients = clients.lock().await;
                redraw_all(&mut clients, &state);
            }
        });
    }
}

impl Server for AppServer {
    type Handler = Self;

    fn new_client(&mut self, _: Option<SocketAddr>) -> Self {
        // `id` is handed out by the one long-lived server instance, so every
        // connection gets its own slot in `clients`.
        let id = self.id;
        self.id += 1;
        Self {
            clients: self.clients.clone(),
            state: self.state.clone(),
            id,
        }
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
        reply: russh::server::ChannelOpenHandle,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let terminal_handle = TerminalHandle::start(session.handle(), channel.id()).await;
        let backend = CrosstermBackend::new(terminal_handle);

        // The real viewport is set when the client requests a pty.
        let options = TerminalOptions {
            viewport: Viewport::Fixed(Rect::default()),
        };
        let terminal = Terminal::with_options(backend, options)?;

        let mut clients = self.clients.lock().await;
        clients.insert(
            self.id,
            ClientState {
                terminal,
                view: View::default(),
                input: Vec::new(),
                // `u64::MAX` never equals a real revision, so the first frame
                // of a new connection is always painted.
                drawn_revision: u64::MAX,
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
        // 1. Buffer the bytes and decode whatever complete keys they contain.
        //    (Escape sequences can be split across two reads, so leftovers stay
        //    buffered instead of being decoded as garbage.)
        let keys = {
            let mut clients = self.clients.lock().await;
            let Some(client) = clients.get_mut(&self.id) else {
                return Ok(());
            };
            client.input.extend_from_slice(data);
            parse_keys(&mut client.input)
        };
        if keys.is_empty() {
            return Ok(());
        }

        // 2. Apply the keys and repaint. Lock order (state → clients) matches
        //    the redraw loop, so the two can never deadlock.
        let mut quit = false;
        {
            let state = self.state.read().await;
            let mut clients = self.clients.lock().await;
            let Some(client) = clients.get_mut(&self.id) else {
                return Ok(());
            };
            for key in keys {
                if view::handle_key(&mut client.view, key, &state.posts) == view::Outcome::Quit {
                    quit = true;
                    break;
                }
            }
            if !quit {
                draw_client(client, &state);
            }
        }

        if quit {
            self.clients.lock().await.remove(&self.id);
            let _ = session.data(channel, FAREWELL.as_bytes().to_vec());
            let _ = session.eof(channel);
            let _ = session.close(channel);
        }

        Ok(())
    }

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
        match self.client_rect(col_width, row_height) {
            Some(rect) => self.resize_client(rect).await,
            None => log::warn!("ignoring pty request with a zero-sized window"),
        }
        // The client must always learn whether its request was accepted;
        // a failure to resize must never take the connection down with it.
        if let Err(error) = session.channel_success(channel) {
            log::warn!("pty_request: channel_success failed: {error:?}");
        }
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        _: ChannelId,
        col_width: u32,
        row_height: u32,
        _: u32,
        _: u32,
        _: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(rect) = self.client_rect(col_width, row_height) {
            self.resize_client(rect).await;
        }
        Ok(())
    }
}

impl AppServer {
    /// Window size from the client, or `None` when the client reported a zero
    /// sized pty (which some terminals do before the first resize).
    fn client_rect(&self, col_width: u32, row_height: u32) -> Option<Rect> {
        if col_width == 0 || row_height == 0 {
            return None;
        }
        Some(Rect {
            x: 0,
            y: 0,
            width: col_width.min(u16::MAX as u32) as u16,
            height: row_height.min(u16::MAX as u32) as u16,
        })
    }

    /// Resizes the terminal and repaints immediately, so a resize is instant
    /// instead of waiting for the next refresh tick.
    ///
    /// Resizing is best-effort: ratatui's `Terminal::resize` asks the *local*
    /// backend for its size (which fails when the server has no controlling
    /// tty, e.g. under systemd or in a container) and that error used to
    /// propagate out of `pty_request` — where russh turns a handler error into
    /// "Connection closed with error", killing the client mid-login. The
    /// viewport itself is updated before that failure, so ignoring it is safe.
    async fn resize_client(&self, rect: Rect) {
        let state = self.state.read().await;
        let mut clients = self.clients.lock().await;
        let Some(client) = clients.get_mut(&self.id) else {
            return;
        };
        if let Err(error) = client.terminal.resize(rect) {
            log::warn!(
                "could not resize the terminal to {}x{}: {error}",
                rect.width,
                rect.height
            );
        }
        draw_client(client, &state);
    }
}

impl Drop for AppServer {
    fn drop(&mut self) {
        // Removing the client from the shared map needs a runtime; during
        // shutdown there may not be one, in which case the map dies with us.
        let id = self.id;
        let clients = self.clients.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                clients.lock().await.remove(&id);
            });
        }
    }
}

/// Shown to a client right before its channel is closed.
const FAREWELL: &str = "\x1b[0m\x1b[?25h\x1b[2J\x1b[H\
\x1b[32m[*]\x1b[0m connection closed — thanks for stopping by\r\n\
     github.com/lsnnt · blog.lsnnt.dev\r\n";

/// Refreshes every upstream source on a timer.
///
/// Each source is fetched independently: a failing GitHub token or a flaky
/// rating API must not blank out the blog, and the blog must not take the
/// homepage down.
async fn update_loop(state: SharedState) {
    let mut interval = tokio::time::interval(REFRESH_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        interval.tick().await;
        let client = HTTP_CLIENT.clone();

        let (repos, codeforces, leetcode, posts) = tokio::join!(
            get_pinned_repo(&client),
            rating(&client),
            lcapi(&client),
            get_posts(&client),
        );

        let mut data = state.write().await;
        let mut changed = false;

        match repos {
            Ok(repos) => {
                data.repos = repos;
                changed = true;
            }
            Err(error) => log::warn!("pinned repositories: {error:#}"),
        }
        match codeforces {
            Ok((rating, max_rating)) => {
                data.rating = rating;
                data.max_rating = max_rating;
                changed = true;
            }
            Err(error) => log::warn!("codeforces: {error:#}"),
        }
        match leetcode {
            Ok(solved) => {
                data.leetcode_problems = solved;
                changed = true;
            }
            Err(error) => log::warn!("leetcode: {error:#}"),
        }
        match posts {
            Ok(posts) if !posts.is_empty() => {
                log::info!("blog: {} posts", posts.len());
                data.posts = posts;
                changed = true;
            }
            Ok(_) => log::warn!("blog: the sitemap listed no readable posts"),
            Err(error) => log::warn!("blog: {error:#}"),
        }

        if changed {
            data.revision += 1;
            data.last_update = theme::clock_utc();
        }
        log::info!(
            "{} repos · {} posts · codeforces {} (max {}) · leetcode {}",
            data.repos.len(),
            data.posts.len(),
            data.rating,
            data.max_rating,
            data.leetcode_problems
        );
        drop(data);
    }
}

#[tokio::main]
async fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let state: SharedState = Arc::new(RwLock::new(AppState::default()));

    println!("sshportfolio listening on {LISTEN_ADDR}");
    println!("  try it: ssh -p 2222 localhost");

    tokio::spawn(update_loop(Arc::clone(&state)));

    let mut server = AppServer::new(state);
    if let Err(error) = server.run().await {
        eprintln!("server stopped: {error:#}");
        std::process::exit(1);
    }
}
