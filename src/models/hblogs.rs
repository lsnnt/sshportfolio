use crate::libs::markdown::Block;

/// A blog post pulled from `blog.lsnnt.dev`.
///
/// The sitemap gives us the permalink + last-modified date, `<permalink>.md`
/// gives us the body. The body is kept *parsed* (`blocks`) rather than
/// rendered, because terminal lines depend on the window width: they are
/// re-flowed whenever the client resizes.
#[derive(Clone, Debug, Default)]
pub struct BlogPost {
    /// Last path segment of the permalink, e.g. `making-of-ntranscribe`.
    pub slug: String,
    /// Title, taken from the leading `# heading` of the markdown body.
    pub title: String,
    /// Human-readable date, e.g. `01 Jul 2026`.
    pub date: String,
    /// Canonical web permalink.
    pub url: String,
    /// Word count of the body.
    pub words: usize,
    /// Estimated reading time in minutes.
    pub minutes: usize,
    /// One-line teaser shown in the post list.
    pub excerpt: String,
    /// Parsed markdown body (without the leading title heading).
    pub blocks: Vec<Block>,
}
