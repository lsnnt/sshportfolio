# sshportfolio

A portfolio you visit over SSH. Every connection gets its own ratatui terminal:
the whole site — home, projects, timeline, blog, contact — is drawn inside the
SSH session.

```console
$ ssh lsnnt.dev        # or: ssh -p 2222 localhost
# any password / any public key is accepted
```

## Features

- **Everything is a TUI.** No browser, no HTML — just a palette, box drawing and
  a keyboard.
- **The blog renders in the terminal.** Posts are pulled from
  `blog.lsnnt.dev` and re-flowed as CommonMark: headings, lists, quotes, code
  blocks with a border and a language label, links, images.
- **Read it like a pager.** Move through the post list with `j`/`k`, press
  <kbd>Enter</kbd> to open a post, <kbd>Backspace</kbd>/<kbd>Esc</kbd> to walk
  back out.
- **Live data.** Pinned repositories (GitHub GraphQL), Codeforces rating and
  LeetCode solve count are refreshed every 10 minutes.
- **Per-client state.** Selection, scroll position and the opened post are
  private to each SSH connection; a shared `AppState` holds the fetched data.

## Keys

| Key | Action |
| --- | --- |
| `h` `p` `a` `t` `b` `c` | go to home / projects / about / timeline / blog / contact |
| `j` / `↓`, `k` / `↑` | move down / up (post list, reader, projects, timeline) |
| `g` / `Home`, `G` / `End` | jump to the top / the bottom |
| `space` / `PageDown`, `PageUp` | page through a post |
| `Enter` / `→` | open the selected blog post |
| `Backspace` / `Esc` / `←` | close the post, back to the list |
| `q` / `Ctrl-C` | disconnect |

Inside the reader the same key switches back to the list: pressing `b` while
reading a post takes you up one level.

## How it works

```
src/main.rs           ssh server: sessions, key decoding, background refresh
src/libs/
  tuis.rs             every screen (nav bar, body, footer)
  view.rs             per-client view state + key handling
  input.rs            raw bytes -> keys (escape sequences, split reads)
  markdown.rs         CommonMark -> ratatui lines
  mblog.rs            blog.lsnnt.dev: sitemap.xml + <permalink>.md
  theme.rs            palette and small widget helpers
  getpinnedrepo.rs    GitHub GraphQL (pinned repositories)
  rating.rs           Codeforces user.info
  lcapi.rs            LeetCode GraphQL
src/models/           serde models for the APIs above
```

Each connection gets a `Terminal<CrosstermBackend<TerminalHandle>>` whose
`Write` sink forwards bytes into the SSH channel, and its own `View` (page,
selections, scroll offsets, the re-flowed post). The server keeps one
`AppState` behind an `RwLock`; a background task refreshes it and a 250 ms tick
repaints only the clients whose data actually changed.

### The blog

`blog.lsnnt.dev` is a Hashnode site, which gives us two things:

1. `https://blog.lsnnt.dev/sitemap.xml` — one `<loc>` per page, `<lastmod>` for
   the date. Entries that are not posts (`/`, `/archive`, `/recommendations`,
   `/tag/...`, …) are skipped.
2. `https://blog.lsnnt.dev/<slug>.md` — the post as CommonMark. The leading `#`
   heading is the title, the rest is the body.

The markdown is parsed into a small block model (`markdown.rs`) and *re-flowed
on demand*: terminal lines depend on the window width, so they are re-rendered
whenever a client resizes, instead of being baked in at fetch time.

```
GET sitemap.xml -> <loc> list -> GET <loc>.md -> parse -> BlogPost { title, date, blocks }
```

Hashnode exports images as `![alt](url align="center")`, which is not valid
CommonMark, so that attribute is stripped before parsing.

## Building and running

```console
cargo run            # listens on 0.0.0.0:2222 and prints the ssh command
ssh -p 2222 localhost
```

Environment:

| Variable | Purpose |
| --- | --- |
| `GT_TOKEN` | GitHub token with `read:user`; without it the projects page stays empty |
| `RUST_LOG` | `info` by default; `RUST_LOG=russh=debug` for protocol-level logs |

## Tests

```console
cargo test                    # unit tests: markdown, sitemap parsing, keys, view state, every screen
cargo test -- --ignored --nocapture   # hits the real blog (network)
cargo fmt && cargo clippy --all-targets
```

The screen tests render every page at 40x12, 80x24, 120x40 and 200x60 through
ratatui's `TestBackend` and assert on the resulting text, so layout regressions
and panics show up without an SSH client.

## Deployment notes

- The server binds `0.0.0.0:2222`; put it behind a reverse proxy or an
  `iptables` redirect from port 22.
- Authentication accepts any password and any public key (it is a portfolio,
  not a shell).
- When running without a controlling tty (systemd, containers), ratatui's
  `Terminal::resize` cannot query the local terminal size. That failure is
  logged and ignored — it must not tear down the client's connection.

## Thanks

Built on [russh](https://github.com/Eugeny/russh),
[ratatui](https://ratatui.rs), [tui-big-text](https://github.com/joshka/tui-big-text),
[pulldown-cmark](https://github.com/pulldown-cmark/pulldown-cmark) and
[textwrap](https://github.com/mgeisler/textwrap).
