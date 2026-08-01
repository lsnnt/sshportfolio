use serde::Deserialize;

//{"data":{"matchedUser":{"submitStats":{"acSubmissionNum":[{"difficulty":"All","count":186},{"difficulty":"Easy","count":105},{"difficulty":"Medium","count":76},{"difficulty":"Hard","count":5}]}}}}

#[derive(Debug, Deserialize)]
pub struct LCResponse {
    pub data: Data,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    #[serde(rename = "matchedUser")]
    pub matched_user: MatchedUser,
}

#[derive(Debug, Deserialize)]
pub struct MatchedUser {
    #[serde(rename = "submitStats")]
    pub submit_stats: SubmitStats,
}

#[derive(Debug, Deserialize)]
pub struct SubmitStats {
    #[serde(rename = "acSubmissionNum")]
    pub ac_submission_num: Vec<SubmissionStat>,
}

#[derive(Debug, Deserialize)]
pub struct SubmissionStat {
    pub difficulty: String,
    pub count: u32,
}