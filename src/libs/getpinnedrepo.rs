//! Pinned repositories, from the GitHub GraphQL API.

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;

use crate::models::githubstructs::GithubResponse;

const GITHUB_GRAPHQL: &str = "https://api.github.com/graphql";

/// Pinned repositories of `lsnnt`, most recent first as GitHub returns them.
///
/// The API requires a token: without one every request would come back 401 and
/// (before) take the whole refresh down with it, so a missing token simply
/// yields an empty list and a note on stderr.
pub async fn get_pinned_repo(client: &Client) -> Result<Vec<crate::models::githubstructs::Repo>> {
    let Some(token) = std::env::var("GT_TOKEN")
        .ok()
        .filter(|token| !token.trim().is_empty())
    else {
        log::warn!("GT_TOKEN is not set — skipping pinned repositories");
        return Ok(Vec::new());
    };

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

    let response = client
        .post(GITHUB_GRAPHQL)
        .header("User-Agent", "lsnnt/sshportfolio")
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .context("GitHub GraphQL request failed")?
        .error_for_status()
        .context("GitHub GraphQL returned an error status")?;

    let text = response.text().await?;
    let parsed: GithubResponse =
        serde_json::from_str(&text).context("unexpected GitHub GraphQL payload")?;

    // A GraphQL error comes back as `"data": null`, which is a valid response.
    Ok(parsed
        .data
        .map(|data| data.user.pinned_items.nodes)
        .unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphql_error_payload_is_handled() {
        let payload = r#"{"data":null,"errors":[{"message":"Bad credentials"}]}"#;
        let parsed: GithubResponse = serde_json::from_str(payload).unwrap();
        assert!(parsed.data.is_none());
    }

    #[test]
    fn fixture_payload_deserializes() {
        let payload = r#"{"data":{"user":{"pinnedItems":{"nodes":[
            {"name":"sshportfolio","description":"TUI over ssh","stargazerCount":3,
             "url":"https://github.com/lsnnt/sshportfolio","primaryLanguage":{"name":"Rust"}}]}}}}"#;
        let parsed: GithubResponse = serde_json::from_str(payload).unwrap();
        let repos = parsed.data.unwrap().user.pinned_items.nodes;
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "sshportfolio");
        assert_eq!(repos[0].primary_language.as_ref().unwrap().name, "Rust");
    }
}
