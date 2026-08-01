use reqwest::Client;
use crate::models::lcmodel::LCResponse;

pub async fn lcapi(client: &Client) -> Result<u16, Box<dyn std::error::Error+Send+Sync+'static>> {

    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert("Content-Type", "application/json".parse()?);

    let data = r#"{
    "query": "\n    query userSessionProgress($username: String!) {\n  matchedUser(username: $username) {\n    submitStats {\n      acSubmissionNum {\n        difficulty \n        count\n }\n   }\n  }\n}\n    ",
    "variables": {
        "username": "governer"
    },
    "operationName": "userSessionProgress"
}"#;

    let json: serde_json::Value = serde_json::from_str(&data)?;

    let request = client.request(reqwest::Method::POST, "https://leetcode.com/graphql/")
        .headers(headers)
        .json(&json);

    let response = request.send().await?;
    let body = response.text().await?;
    let lcdata : LCResponse = serde_json::from_str(&body)?;
    let mut resu: u16 = 0;
   for x in  lcdata.data.matched_user.submit_stats.ac_submission_num {
       if x.difficulty == "All" {
           resu = x.count as u16;
       }
   }

    Ok(resu)
}