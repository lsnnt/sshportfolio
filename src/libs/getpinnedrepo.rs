use reqwest::Client;
use serde_json::json;
use crate::models::githubstructs::{GithubResponse, Repo};

pub async fn get_pinned_repo(client: &Client) -> Result<Vec<Repo>, Box<dyn std::error::Error+Send+Sync+'static>> {
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
                            primaryLanguage {
                                name
                            }
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
    // println!("1");
    Ok(response.data.user.pinned_items.nodes)

}