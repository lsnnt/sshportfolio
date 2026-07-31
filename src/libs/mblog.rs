use std::time::Duration;
use moka::future::Cache;
use once_cell::sync::Lazy;
use reqwest::Client;
use rss::Channel;
use crate::models::hblogs::Blogs;
static MBLOG: once_cell::sync::Lazy<Cache<&'static str, Vec<Blogs>>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(60 * 60))
        .build()
});
pub async fn getblog(client: &Client) -> Result<Vec<Blogs>,Box<dyn std::error::Error+Send+Sync+'static>> {
    if let Some(value) = MBLOG.get("mblog").await {
        return Ok(value)
    }
    let content = client.get("https://blog.lsnnt.dev/rss.xml").send().await?.bytes().await?;

    let channel = Channel::read_from(&content[..])?;
    let vblogs: Vec<Blogs> = channel
        .items()
        .iter()
        .map(|item| Blogs {
            title: item.title.clone().unwrap_or_else(|| "Untitled".to_string()),
            date: item.pub_date.clone().unwrap_or_else(|| "01-01-1970".to_string()),
            link: item.link.clone().unwrap_or_else(|| "".to_string()),
        })
        .collect();
    MBLOG.insert("mblog", vblogs.clone()).await;
    Ok(vblogs)
}