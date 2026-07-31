use std::time::Duration;
use moka::future::Cache;
use once_cell::sync::Lazy;
use reqwest::Client;
use serde_json::json;
use crate::models::githubstructs::{GithubResponse, Repo};

static PINNED_USER_REPO: Lazy<Cache<&'static str, Vec<Repo>>>  = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(60 * 60))
        .build()
});
pub async fn get_pinned_repo(client: &Client) -> Result<Vec<Repo>, Box<dyn std::error::Error+Send+Sync+'static>> {
    if let Some(value) = PINNED_USER_REPO.get("lsnnt").await {
        return Ok(value);
    }
    let authtoken = std::env::var("GT_TOKEN")?;

    let query = r#"
        query($login: String!) {
            user(login: $login) {
                pinnedItems(first: 6, types: REPOSITORY) {
                    nodes {
                        ... on Repository {
                            name
                            description
                            stargazerCount
                            url
                        }
                    }
                }
            }
        }
    "#;

    let body = json!({
        "query": query,
        "variables": {
            "login": "lsnnt"
        }
    });

    let res = client
        .post("https://api.github.com/graphql")
        .header("User-Agent", "lsnnt")
        .bearer_auth(authtoken)
        .json(&body)
        .send()
        .await?;

    let text = res.text().await?;
    let response: GithubResponse = serde_json::from_str(&text)?;
   PINNED_USER_REPO.insert("lsnnt", response.data.user.pinned_items.nodes.clone()).await;

    Ok(response.data.user.pinned_items.nodes)

}