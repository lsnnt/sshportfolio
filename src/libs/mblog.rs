//! Blog source — `blog.lsnnt.dev`.
//!
//! Two requests per post:
//!
//! 1. `/sitemap.xml` — every `<loc>` is a permalink, `<lastmod>` its date.
//! 2. `<permalink>.md` — the CommonMark body, whose leading `# H1` is the title.
//!
//! Anything that is not a post (the site root, `/archive`, `/recommendations`,
//! `/tag/...`, ...) answers the `.md` request with a 404 or an HTML page, so the
//! response — not the URL — decides what counts as a post.

use anyhow::{Result, bail};
use reqwest::Client;

use crate::libs::markdown;
use crate::models::hblogs::BlogPost;

pub const SITEMAP_URL: &str = "https://blog.lsnnt.dev/sitemap.xml";

/// Routes served by the blog that are pages rather than posts. Used only to
/// avoid a pointless request; the real filter is the `.md` response.
const NON_POST_ROUTES: &[&str] = &[
    "",
    "archive",
    "recommendations",
    "tag",
    "tags",
    "series",
    "search",
    "about",
    "contact",
    "newsletter",
    "privacy",
    "terms",
    "feed",
    "rss.xml",
    "sitemap.xml",
    "robots.txt",
    "404",
];

/// How many posts we are willing to pull on a refresh.
const MAX_POSTS: usize = 60;

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// A `<url>` element of the sitemap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SitemapEntry {
    pub loc: String,
    pub lastmod: Option<String>,
}

/// Extracts every `<url>` of a sitemap document.
pub fn parse_sitemap(xml: &str) -> Vec<SitemapEntry> {
    let mut entries = Vec::new();
    let mut rest = xml;

    while let Some(start) = rest.find("<loc>") {
        let after = &rest[start + "<loc>".len()..];
        let Some(end) = after.find("</loc>") else {
            break;
        };
        let loc = unescape_xml(after[..end].trim());

        // Only look for a `<lastmod>` inside the same `<url>` element.
        let tail = &after[end..];
        let scope_end = tail.find("<loc>").unwrap_or(tail.len());
        let scope = &tail[..scope_end];
        let lastmod = scope.find("<lastmod>").and_then(|position| {
            let inner = &scope[position + "<lastmod>".len()..];
            inner
                .find("</lastmod>")
                .map(|end| inner[..end].trim().to_string())
        });

        if !loc.is_empty() {
            entries.push(SitemapEntry { loc, lastmod });
        }
        rest = tail;
    }

    entries
}

fn unescape_xml(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
}

/// Path part of a URL, without the leading slash (`""` for the site root).
pub fn path_of(url: &str) -> &str {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    match without_scheme.split_once('/') {
        Some((_, path)) => path.trim_end_matches('/'),
        None => "",
    }
}

/// Last path segment of a permalink.
pub fn slug_of(url: &str) -> String {
    path_of(url)
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_string()
}

/// Whether a sitemap entry is worth asking for a markdown body.
pub fn is_post_url(url: &str) -> bool {
    let path = path_of(url);
    if path.is_empty() {
        return false;
    }
    let first_segment = path.split('/').next().unwrap_or("");
    !NON_POST_ROUTES.contains(&first_segment)
}

/// `making-of-ntranscribe` → `Making of ntranscribe` (fallback when a post has
/// no `# heading`).
pub fn prettify_slug(slug: &str) -> String {
    let words: Vec<String> = slug
        .split('-')
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect();
    let joined = words.join(" ");
    if joined.is_empty() {
        slug.to_string()
    } else {
        joined
    }
}

/// Title of a post: the text of its leading `# heading`.
pub fn extract_title(markdown_body: &str) -> Option<String> {
    for line in markdown_body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return trimmed
            .strip_prefix("# ")
            .map(|title| title.trim().to_string())
            .filter(|title| !title.is_empty());
    }
    None
}

/// Removes the leading `# heading` so the reader does not repeat the title.
pub fn strip_title(markdown_body: &str) -> String {
    let mut seen_content = false;
    let mut out: Vec<&str> = Vec::new();

    for line in markdown_body.lines() {
        let trimmed = line.trim();
        if !seen_content {
            if trimmed.starts_with("# ") {
                // The leading H1 is the title: drop it and any blank lines after.
                seen_content = true;
                continue;
            }
            if trimmed.is_empty() {
                continue;
            }
            // No leading H1 at all: keep everything from here on.
            seen_content = true;
        }
        out.push(line);
    }

    while out
        .first()
        .map(|line| line.trim().is_empty())
        .unwrap_or(false)
    {
        out.remove(0);
    }
    out.join("\n")
}

/// `2026-07-01T10:59:03.057Z` → `01 Jul 2026` (falls back to the raw value).
pub fn format_date(raw: &str) -> String {
    let date = raw.split('T').next().unwrap_or(raw);
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() == 3
        && let (Ok(year), Ok(month), Ok(day)) = (
            parts[0].parse::<u32>(),
            parts[1].parse::<usize>(),
            parts[2].parse::<u32>(),
        )
        && (1..=12).contains(&month)
    {
        return format!("{day:02} {} {year}", MONTHS[month - 1]);
    }
    raw.to_string()
}

/// Reading time in minutes for a word count (~200 wpm, minimum 1).
pub fn reading_minutes(words: usize) -> usize {
    words.div_ceil(200).max(1)
}

/// Hashnode exports images as `![alt](url align="center")`, which is not valid
/// CommonMark (a link destination cannot contain spaces) and would otherwise be
/// rendered as raw syntax. Drop the alignment attribute.
pub fn sanitize_markdown(markdown_body: &str) -> String {
    let mut out = markdown_body.to_string();
    for attribute in [
        " align=\"center\"",
        " align=\"left\"",
        " align=\"right\"",
        " align=“center”",
        " align=“left”",
        " align=“right”",
    ] {
        out = out.replace(&format!("{attribute})"), ")");
    }
    out
}

/// A `.md` response is treated as a post when it is markdown — some hosts serve
/// it as `text/plain`, and non-posts answer with an HTML 404 page or a one-line
/// "Post not found" body.
fn looks_like_post(content_type: &str, body: &str) -> bool {
    let head = body.trim_start();
    if head.starts_with("<!DOCTYPE") || head.starts_with("<html") || head.starts_with('<') {
        return false;
    }
    if head.eq_ignore_ascii_case("post not found") || head.is_empty() {
        return false;
    }
    content_type.contains("markdown") || body.lines().any(|line| line.starts_with("# "))
}

/// Fetches the sitemap and then every post body.
///
/// Individual failures are logged and skipped: one broken post must not take
/// the whole blog section down.
pub async fn get_posts(client: &Client) -> Result<Vec<BlogPost>> {
    let body = client
        .get(SITEMAP_URL)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let entries = parse_sitemap(&body);
    if entries.is_empty() {
        bail!("sitemap at {SITEMAP_URL} contained no <loc> entries");
    }

    let mut posts = Vec::new();
    for entry in entries
        .into_iter()
        .filter(|entry| is_post_url(&entry.loc))
        .take(MAX_POSTS)
    {
        match fetch_post(client, &entry).await {
            Ok(post) => posts.push(post),
            Err(error) => log::warn!("skipping {}: {error:#}", entry.loc),
        }
    }
    Ok(posts)
}

async fn fetch_post(client: &Client, entry: &SitemapEntry) -> Result<BlogPost> {
    let url = format!("{}.md", entry.loc);
    let response = client.get(&url).send().await?;
    if !response.status().is_success() {
        bail!("HTTP {} from {url}", response.status());
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let body = response.text().await?;
    if !looks_like_post(&content_type, &body) {
        bail!("not a post (content-type: {content_type})");
    }

    let slug = slug_of(&entry.loc);
    let body = sanitize_markdown(&body);
    let title = extract_title(&body).unwrap_or_else(|| prettify_slug(&slug));
    let blocks = markdown::parse(&strip_title(&body));
    let words = markdown::word_count(&blocks);

    Ok(BlogPost {
        slug,
        title,
        date: entry
            .lastmod
            .as_deref()
            .map(format_date)
            .unwrap_or_else(|| "—".to_string()),
        url: entry.loc.clone(),
        words,
        minutes: reading_minutes(words),
        excerpt: markdown::excerpt(&blocks, 96),
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::libs::markdown::Block;

    const SITEMAP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url>
    <loc>https://blog.lsnnt.dev</loc>
    <lastmod>2026-10-11T12:27:09.765Z</lastmod>
    <changefreq>always</changefreq>
    <priority>1.0</priority>
  </url>
  <url>
    <loc>https://blog.lsnnt.dev/making-of-ntranscribe</loc>
    <lastmod>2026-07-01T10:59:03.057Z</lastmod>
    <changefreq>daily</changefreq>
    <priority>0.8</priority>
  </url>
  <url>
    <loc>https://blog.lsnnt.dev/archive</loc>
    <changefreq>daily</changefreq>
    <priority>0.5</priority>
  </url>
</urlset>"#;

    #[test]
    fn sitemap_entries_keep_their_own_lastmod() {
        let entries = parse_sitemap(SITEMAP);
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].loc, "https://blog.lsnnt.dev");
        assert_eq!(
            entries[0].lastmod.as_deref(),
            Some("2026-10-11T12:27:09.765Z")
        );
        assert_eq!(
            entries[1].loc,
            "https://blog.lsnnt.dev/making-of-ntranscribe"
        );
        assert_eq!(
            entries[1].lastmod.as_deref(),
            Some("2026-07-01T10:59:03.057Z")
        );
        // The last entry has no <lastmod>: it must not inherit the previous one.
        assert_eq!(entries[2].lastmod, None);
    }

    #[test]
    fn only_posts_are_requested() {
        assert!(!is_post_url("https://blog.lsnnt.dev"));
        assert!(!is_post_url("https://blog.lsnnt.dev/archive"));
        assert!(!is_post_url("https://blog.lsnnt.dev/recommendations"));
        assert!(!is_post_url("https://blog.lsnnt.dev/tag/rust"));
        assert!(is_post_url("https://blog.lsnnt.dev/making-of-ntranscribe"));
        assert!(is_post_url(
            "https://blog.lsnnt.dev/how-i-created-my-own-custom-ddns-script-df3d5402658f"
        ));
    }

    #[test]
    fn slugs_and_dates() {
        assert_eq!(
            slug_of("https://blog.lsnnt.dev/making-of-ntranscribe"),
            "making-of-ntranscribe"
        );
        assert_eq!(
            prettify_slug("making-of-ntranscribe"),
            "Making Of Ntranscribe"
        );
        assert_eq!(format_date("2026-07-01T10:59:03.057Z"), "01 Jul 2026");
        assert_eq!(format_date("not-a-date"), "not-a-date");
    }

    #[test]
    fn title_comes_from_the_leading_h1_and_is_stripped() {
        let body = "# Making of ntranscribe\n\n# Why this?\n\nBecause.\n";
        assert_eq!(
            extract_title(body).as_deref(),
            Some("Making of ntranscribe")
        );
        let stripped = strip_title(body);
        assert!(!stripped.starts_with("# Making of ntranscribe"));
        assert!(stripped.contains("# Why this?"));
        assert_eq!(extract_title("no heading here"), None);
    }

    #[test]
    fn html_404_pages_are_not_posts() {
        assert!(!looks_like_post("text/html", "<!DOCTYPE html><html>..."));
        assert!(!looks_like_post("text/plain", "Post not found"));
        assert!(!looks_like_post("text/plain", ""));
        assert!(looks_like_post(
            "text/markdown; charset=utf-8",
            "# Hi\n\nbody"
        ));
        assert!(looks_like_post("text/plain", "# Hi\n\nbody"));
    }

    #[test]
    fn reading_time_is_at_least_one_minute() {
        assert_eq!(reading_minutes(0), 1);
        assert_eq!(reading_minutes(200), 1);
        assert_eq!(reading_minutes(201), 2);
    }

    #[test]
    fn hashnode_image_attributes_are_stripped() {
        let body = "![](https://cdn.test/a.png align=\"center\")\n";
        assert_eq!(sanitize_markdown(body), "![](https://cdn.test/a.png)\n");
        // The cleaned-up image is parsed as an image block, not as text.
        let blocks = markdown::parse(&sanitize_markdown(body));
        assert!(matches!(blocks[0], Block::Image { .. }), "{blocks:?}");
    }

    /// End-to-end check against the live blog (needs network):
    ///
    /// ```text
    /// cargo test live_blog -- --ignored --nocapture
    /// ```
    #[tokio::test]
    #[ignore = "requires network access to blog.lsnnt.dev"]
    async fn live_blog_fetch_and_render() {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("client");
        let posts = get_posts(&client).await.expect("fetch posts");

        assert!(!posts.is_empty(), "the sitemap produced no posts");
        for post in &posts {
            assert!(!post.title.is_empty(), "{} has no title", post.slug);
            assert!(!post.blocks.is_empty(), "{} has no body", post.slug);
            assert!(!post.excerpt.is_empty(), "{} has no excerpt", post.slug);
        }

        println!("\n{} posts from {SITEMAP_URL}", posts.len());
        for post in &posts {
            println!(
                "  • [{}] {} — {} words / {} min ({} lines of markdown)",
                post.date,
                post.title,
                post.words,
                post.minutes,
                post.blocks.len()
            );
        }

        let post = &posts[0];
        let lines = markdown::render(&post.blocks, 88);
        println!("\n=== {} — {} rendered lines ===", post.title, lines.len());
        for line in lines.iter().take(30) {
            let text: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            println!("{text}");
        }
    }
}
