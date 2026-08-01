use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct CFResponse {
    pub(crate) result: Vec<User>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct User {
    pub(crate) rating: i32,
    #[serde(rename = "maxRating")]
    pub(crate) max_rating: i32,
}