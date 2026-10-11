//! LeetCode GraphQL models.

use serde::Deserialize;

// {"data":{"matchedUser":{"submitStats":{"acSubmissionNum":[{"difficulty":"All","count":186},{"difficulty":"Easy","count":105},{"difficulty":"Medium","count":76},{"difficulty":"Hard","count":5}]}}}}

#[derive(Debug, Deserialize)]
pub struct LCResponse {
    pub data: Option<Data>,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    /// `null` when the handle does not exist.
    #[serde(rename = "matchedUser")]
    pub matched_user: Option<MatchedUser>,
}

#[derive(Debug, Deserialize)]
pub struct MatchedUser {
    #[serde(rename = "submitStats")]
    pub submit_stats: SubmitStats,
}

#[derive(Debug, Deserialize)]
pub struct SubmitStats {
    #[serde(rename = "acSubmissionNum", default)]
    pub ac_submission_num: Vec<SubmissionStat>,
}

#[derive(Debug, Deserialize)]
pub struct SubmissionStat {
    pub difficulty: String,
    #[serde(default)]
    pub count: Option<u32>,
}
