use std::time::Duration;
use moka::sync::Cache;
use once_cell::sync::Lazy;
use reqwest::blocking::Client;
use serde_json::from_str;
use crate::models::lcmodel::LCResponse;

static LC_CACHE: Lazy<Cache<&'static str, u16>> = Lazy::new(|| {
    Cache::builder()
        .time_to_live(Duration::from_secs(60 * 60))
        .build()
});
pub fn lcapi(client: &Client) -> Result<u16, Box<dyn std::error::Error+Send+Sync+'static>> {
    if let Some(value) = LC_CACHE.get("leetcode") {
        return Ok(value);
    }
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse()?);

    let data = r#"{
    "query": "\n    query userSessionProgress($username: String!) {\n  matchedUser(username: $username) {\n    submitStats {\n      acSubmissionNum {\n        difficulty \n        count\n }\n   }\n  }\n}\n    ",
    "variables": {
        "username": "governer"
    },
    "operationName": "userSessionProgress"
}"#;

    let json: serde_json::Value = from_str(&data)?;

    let request = client.request(reqwest::Method::POST, "https://leetcode.com/graphql/")
        .headers(headers)
        .json(&json);

    let response = request.send()?;
    let body = response.text()?;
    let lcdata : LCResponse = from_str(&body)?;
    let mut resu: u16 = 0;
    for x in lcdata.data.matched_user.submit_stats.ac_submission_num {
        if x.difficulty == "All" {
            resu = x.count as u16;
        }
    }

    LC_CACHE.insert("leetcode", resu);
    Ok(resu)
}