//! Codeforces `user.info` models.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct CFResponse {
    /// Empty when the API answers `"status": "FAILED"`.
    #[serde(default)]
    pub(crate) result: Vec<User>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct User {
    /// `null` for unrated handles.
    #[serde(default)]
    pub(crate) rating: Option<i64>,
    #[serde(rename = "maxRating", default)]
    pub(crate) max_rating: Option<i64>,
}
