use reqwest::Client;
use rss::Channel;
use crate::models::hblogs::Blogs;
pub async fn getblog(client: &Client) -> Result<Vec<Blogs>,Box<dyn std::error::Error+Send+Sync+'static>> {
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
    Ok(vblogs)
}