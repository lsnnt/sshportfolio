//! Per-client view state and key handling.
//!
//! Every SSH connection gets its own [`View`]: which page it is on, where its
//! cursors are, and — for the blog — which entry is selected and, when a post
//! is open, the post re-flowed into terminal lines for the current width.

use ratatui::text::Line;
use ratatui::widgets::ListState;

use crate::libs::input::Key;
use crate::libs::markdown;
use crate::models::hblogs::BlogPost;

/// Which section of the site a client is currently viewing.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub enum Page {
    #[default]
    Home,
    About,
    Projects,
    Timeline,
    Blog,
    Contact,
}

impl Page {
    /// Nav order (also the order of the chips in the nav bar).
    pub const ALL: [Page; 6] = [
        Page::Home,
        Page::Projects,
        Page::About,
        Page::Timeline,
        Page::Blog,
        Page::Contact,
    ];

    pub const fn key(self) -> char {
        match self {
            Page::Home => 'h',
            Page::Projects => 'p',
            Page::About => 'a',
            Page::Timeline => 't',
            Page::Blog => 'b',
            Page::Contact => 'c',
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Page::Home => "home",
            Page::Projects => "projects",
            Page::About => "about",
            Page::Timeline => "timeline",
            Page::Blog => "blog",
            Page::Contact => "contact",
        }
    }

    /// Path shown in the prompt / breadcrumb.
    pub const fn path(self) -> &'static str {
        match self {
            Page::Home => "~",
            Page::Projects => "~/projects",
            Page::About => "~/about",
            Page::Timeline => "~/timeline",
            Page::Blog => "~/blog",
            Page::Contact => "~/contact",
        }
    }

    pub fn from_key(key: char) -> Option<Page> {
        Page::ALL
            .into_iter()
            .find(|page| page.key() == key.to_ascii_lowercase())
    }
}

/// Blog section state: the post list plus the currently opened post.
#[derive(Debug, Default)]
pub struct BlogView {
    /// Selection + scroll offset of the post list.
    pub list: ListState,
    /// Scroll offset inside an opened post.
    pub post_scroll: u16,
    /// Index into `AppState::posts` of the opened post, if any.
    pub open: Option<usize>,
    /// Width and post index the cached `lines` were rendered for.
    rendered_width: u16,
    rendered_post: Option<usize>,
    /// Post body re-flowed to `rendered_width` columns.
    pub lines: Vec<Line<'static>>,
    /// Height of the reader viewport at the last draw (for paging/clamping).
    pub viewport: u16,
}

impl BlogView {
    /// Index of the selected post (clamped to the number of posts).
    pub fn selected(&self, posts: usize) -> usize {
        if posts == 0 {
            return 0;
        }
        self.list.selected().unwrap_or(0).min(posts - 1)
    }

    /// Keeps the selection inside the list (call before rendering).
    pub fn clamp_selection(&mut self, posts: usize) {
        if posts == 0 {
            self.list.select(None);
            return;
        }
        self.list.select(Some(self.selected(posts)));
    }

    /// Moves the selection by `delta`, clamped to the list bounds.
    pub fn move_selection(&mut self, posts: usize, delta: i64) {
        if posts == 0 {
            return;
        }
        let next = (self.selected(posts) as i64 + delta).clamp(0, posts as i64 - 1);
        self.list.select(Some(next as usize));
    }

    pub fn select(&mut self, posts: usize, index: usize) {
        if posts == 0 {
            return;
        }
        self.list.select(Some(index.min(posts - 1)));
    }

    /// Opens the selected post in the reader.
    pub fn open_selected(&mut self, posts: usize) {
        if posts == 0 {
            return;
        }
        let selected = self.selected(posts);
        self.open = Some(selected);
        self.post_scroll = 0;
        // Force the reader to re-flow on the next draw.
        self.rendered_post = None;
    }

    pub fn close(&mut self) {
        self.open = None;
        self.post_scroll = 0;
    }

    pub fn is_reading(&self) -> bool {
        self.open.is_some()
    }

    /// Maximum scroll offset for the opened post.
    pub fn max_scroll(&self) -> u16 {
        let max = self
            .lines
            .len()
            .saturating_sub(self.viewport.max(1) as usize);
        max.min(u16::MAX as usize) as u16
    }

    pub fn scroll_post(&mut self, delta: i64) {
        let max = self.max_scroll() as i64;
        let next = (self.post_scroll as i64 + delta).clamp(0, max);
        self.post_scroll = next as u16;
    }

    /// Re-flows the post into terminal lines when the width (or the opened
    /// post) changed since the last draw.
    pub fn refresh(&mut self, post: &BlogPost, width: u16) {
        if self.rendered_post != self.open || self.rendered_width != width {
            self.lines = markdown::render(&post.blocks, width);
            self.rendered_width = width;
            self.rendered_post = self.open;
            self.post_scroll = self.post_scroll.min(self.max_scroll());
        }
    }

    /// Scroll progress of the open post, `0..=100`.
    pub fn progress(&self) -> u16 {
        let max = self.max_scroll();
        if max == 0 {
            return 100;
        }
        ((self.post_scroll as u32 * 100) / max as u32).min(100) as u16
    }
}

/// Everything a client "looks at" while connected.
#[derive(Debug, Default)]
pub struct View {
    pub page: Page,
    pub projects_scroll: u16,
    pub timeline_scroll: u16,
    pub blog: BlogView,
    /// Height of the projects/timeline viewport at the last draw.
    pub text_viewport: u16,
}

/// What the caller has to do after a key press.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    Redraw,
    Quit,
}

/// Applies one key press to a client's view.
pub fn handle_key(view: &mut View, key: Key, posts: &[BlogPost]) -> Outcome {
    match key {
        Key::CtrlC => return Outcome::Quit,
        // `q` quits from anywhere; inside the reader `b`/`⌫` walk back out.
        Key::Char('q') | Key::Char('Q') => return Outcome::Quit,
        _ => {}
    }

    if let Key::Char(character) = key
        && let Some(page) = Page::from_key(character)
    {
        // Pressing the key of the section you are already in walks up one level.
        if page == view.page && page == Page::Blog && view.blog.is_reading() {
            view.blog.close();
        } else {
            view.page = page;
        }
        return Outcome::Redraw;
    }

    match view.page {
        Page::Blog if view.blog.is_reading() => post_keys(view, key, posts),
        Page::Blog => list_keys(view, key, posts),
        Page::Projects => {
            scroll_keys(&mut view.projects_scroll, view.text_viewport, key);
        }
        Page::Timeline => {
            scroll_keys(&mut view.timeline_scroll, view.text_viewport, key);
        }
        _ => {}
    }

    Outcome::Redraw
}

fn list_keys(view: &mut View, key: Key, posts: &[BlogPost]) {
    match key {
        Key::Down | Key::Char('j') => view.blog.move_selection(posts.len(), 1),
        Key::Up | Key::Char('k') => view.blog.move_selection(posts.len(), -1),
        Key::PageDown | Key::Char(' ') => view.blog.move_selection(posts.len(), 5),
        Key::PageUp => view.blog.move_selection(posts.len(), -5),
        Key::Home | Key::Char('g') => view.blog.select(posts.len(), 0),
        Key::End | Key::Char('G') => {
            if !posts.is_empty() {
                view.blog.select(posts.len(), posts.len() - 1);
            }
        }
        Key::Enter | Key::Right => view.blog.open_selected(posts.len()),
        _ => {}
    }
}

fn post_keys(view: &mut View, key: Key, posts: &[BlogPost]) {
    let page = view.blog.viewport.saturating_sub(2).max(1) as i64;
    match key {
        Key::Backspace | Key::Esc | Key::Left => view.blog.close(),
        Key::Down | Key::Char('j') => view.blog.scroll_post(1),
        Key::Up | Key::Char('k') => view.blog.scroll_post(-1),
        Key::PageDown | Key::Char(' ') => view.blog.scroll_post(page),
        Key::PageUp => view.blog.scroll_post(-page),
        Key::Home | Key::Char('g') => view.blog.post_scroll = 0,
        Key::End | Key::Char('G') => view.blog.post_scroll = view.blog.max_scroll(),
        Key::Right => view.blog.open_selected(posts.len()),
        _ => {}
    }
}

fn scroll_keys(offset: &mut u16, viewport: u16, key: Key) {
    match key {
        Key::Down | Key::Char('j') => *offset = offset.saturating_add(1),
        Key::Up | Key::Char('k') => *offset = offset.saturating_sub(1),
        Key::PageDown | Key::Char(' ') => *offset = offset.saturating_add(viewport.max(1)),
        Key::PageUp => *offset = offset.saturating_sub(viewport.max(1)),
        Key::Home | Key::Char('g') => *offset = 0,
        Key::End | Key::Char('G') => *offset = u16::MAX / 2,
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libs::markdown;

    fn posts(count: usize) -> Vec<BlogPost> {
        (0..count)
            .map(|index| BlogPost {
                slug: format!("post-{index}"),
                title: format!("Post {index}"),
                blocks: markdown::parse("# Post\n\nbody text\n"),
                ..BlogPost::default()
            })
            .collect()
    }

    #[test]
    fn page_keys_include_every_section() {
        for page in Page::ALL {
            assert_eq!(Page::from_key(page.key()), Some(page));
        }
        assert_eq!(Page::from_key('z'), None);
        assert_eq!(Page::from_key('B'), Some(Page::Blog));
    }

    #[test]
    fn selection_is_clamped_at_both_ends() {
        let list = posts(3);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        handle_key(&mut view, Key::Char('k'), &list);
        assert_eq!(view.blog.selected(list.len()), 0);
        for _ in 0..10 {
            handle_key(&mut view, Key::Char('j'), &list);
        }
        assert_eq!(view.blog.selected(list.len()), 2);
        handle_key(&mut view, Key::Char('G'), &list);
        assert_eq!(view.blog.selected(list.len()), 2);
        handle_key(&mut view, Key::Char('g'), &list);
        assert_eq!(view.blog.selected(list.len()), 0);
    }

    #[test]
    fn enter_opens_the_selected_post_and_backspace_returns() {
        let list = posts(4);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        view.blog.viewport = 2; // a small reader: one line of the post is off-screen
        handle_key(&mut view, Key::Char('j'), &list);
        handle_key(&mut view, Key::Enter, &list);
        assert!(view.blog.is_reading());
        assert_eq!(view.blog.open, Some(1));
        // The reader re-flows the post on the next draw.
        view.blog.refresh(&list[1], 40);
        assert!(view.blog.max_scroll() >= 1);

        // While reading, j/k scroll instead of moving the selection.
        let before = view.blog.selected(list.len());
        handle_key(&mut view, Key::Char('j'), &list);
        assert_eq!(view.blog.selected(list.len()), before);
        assert_eq!(view.blog.post_scroll, 1);

        handle_key(&mut view, Key::Backspace, &list);
        assert!(!view.blog.is_reading());
        assert_eq!(view.blog.post_scroll, 0);
        // The selection is remembered.
        assert_eq!(view.blog.selected(list.len()), 1);

        handle_key(&mut view, Key::Enter, &list);
        handle_key(&mut view, Key::Esc, &list);
        assert!(!view.blog.is_reading());
    }

    #[test]
    fn arrow_keys_and_page_keys_work() {
        let list = posts(20);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        handle_key(&mut view, Key::Down, &list);
        assert_eq!(view.blog.selected(list.len()), 1);
        handle_key(&mut view, Key::Up, &list);
        assert_eq!(view.blog.selected(list.len()), 0);
        handle_key(&mut view, Key::PageDown, &list);
        assert_eq!(view.blog.selected(list.len()), 5);
        handle_key(&mut view, Key::End, &list);
        assert_eq!(view.blog.selected(list.len()), 19);
    }

    #[test]
    fn empty_blog_is_safe() {
        let list: Vec<BlogPost> = Vec::new();
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        handle_key(&mut view, Key::Char('j'), &list);
        handle_key(&mut view, Key::Enter, &list);
        assert!(!view.blog.is_reading());
        assert_eq!(view.blog.selected(0), 0);
    }

    #[test]
    fn quit_keys() {
        let list = posts(1);
        let mut view = View::default();
        assert_eq!(handle_key(&mut view, Key::Char('q'), &list), Outcome::Quit);
        assert_eq!(handle_key(&mut view, Key::CtrlC, &list), Outcome::Quit);
        assert_eq!(
            handle_key(&mut view, Key::Char('j'), &list),
            Outcome::Redraw
        );
    }

    #[test]
    fn blog_key_inside_the_reader_walks_back_out() {
        let list = posts(2);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        handle_key(&mut view, Key::Enter, &list);
        assert!(view.blog.is_reading());
        handle_key(&mut view, Key::Char('b'), &list);
        assert!(!view.blog.is_reading());
        assert_eq!(view.page, Page::Blog);
    }

    #[test]
    fn post_scroll_is_clamped_to_the_content() {
        let list = posts(1);
        let mut view = View {
            page: Page::Blog,
            ..View::default()
        };
        view.blog.viewport = 10;
        handle_key(&mut view, Key::Enter, &list);
        view.blog.refresh(&list[0], 40);
        let max = view.blog.max_scroll();
        for _ in 0..200 {
            handle_key(&mut view, Key::Char('j'), &list);
        }
        assert_eq!(view.blog.post_scroll, max);
        handle_key(&mut view, Key::Char('g'), &list);
        assert_eq!(view.blog.post_scroll, 0);
        // Scrolling up from the top saturates at 0 instead of wrapping around.
        handle_key(&mut view, Key::Char('k'), &list);
        assert_eq!(view.blog.post_scroll, 0);
    }

    #[test]
    fn progress_reports_percentages() {
        let mut blog = BlogView::default();
        assert_eq!(blog.progress(), 100);
        blog.lines = vec![ratatui::text::Line::default(); 100];
        blog.viewport = 10;
        assert_eq!(blog.progress(), 0);
        blog.post_scroll = blog.max_scroll();
        assert_eq!(blog.progress(), 100);
    }
}
