//! LeetCode solved-problem count, from the public GraphQL endpoint.

use anyhow::{Context, Result};
use reqwest::Client;
use serde_json::json;

use crate::models::lcmodel::LCResponse;

const LEETCODE_GRAPHQL: &str = "https://leetcode.com/graphql/";

/// Number of accepted submissions (the `All` bucket).
pub async fn lcapi(client: &Client) -> Result<u16> {
    let payload = json!({
        "query": "query userSessionProgress($username: String!) { matchedUser(username: $username) { submitStats { acSubmissionNum { difficulty count } } } }",
        "variables": { "username": "governer" },
        "operationName": "userSessionProgress"
    });

    let body = client
        .post(LEETCODE_GRAPHQL)
        .header("User-Agent", "lsnnt/sshportfolio")
        .json(&payload)
        .send()
        .await
        .context("LeetCode request failed")?
        .error_for_status()
        .context("LeetCode returned an error status")?
        .text()
        .await?;

    let parsed: LCResponse = serde_json::from_str(&body).context("unexpected LeetCode payload")?;

    let matched = parsed
        .data
        .and_then(|data| data.matched_user)
        .context("LeetCode returned no such user")?;

    let solved = matched
        .submit_stats
        .ac_submission_num
        .iter()
        .find(|stat| stat.difficulty == "All")
        .and_then(|stat| stat.count)
        .unwrap_or(0);

    Ok(solved.min(u16::MAX as u32) as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_bucket_is_picked() {
        let body = json!({"data":{"matchedUser":{"submitStats":{"acSubmissionNum":[
            {"difficulty":"All","count":186},
            {"difficulty":"Easy","count":105}]}}}})
        .to_string();
        let parsed: LCResponse = serde_json::from_str(&body).unwrap();
        let matched = parsed.data.unwrap().matched_user.unwrap();
        let solved = matched
            .submit_stats
            .ac_submission_num
            .iter()
            .find(|stat| stat.difficulty == "All")
            .and_then(|stat| stat.count)
            .unwrap_or(0);
        assert_eq!(solved, 186);
    }

    #[test]
    fn unknown_user_deserializes_to_none() {
        let body = json!({"data":{"matchedUser":null}}).to_string();
        let parsed: LCResponse = serde_json::from_str(&body).unwrap();
        assert!(parsed.data.unwrap().matched_user.is_none());
    }
}
