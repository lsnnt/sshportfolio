//! GitHub GraphQL models.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct GithubResponse {
    /// `null` when the query fails (bad token, rate limit, ...).
    pub data: Option<Data>,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    pub user: User,
}

#[derive(Debug, Deserialize)]
pub struct User {
    #[serde(rename = "pinnedItems")]
    pub pinned_items: PinnedItems,
}

#[derive(Debug, Deserialize)]
pub struct PinnedItems {
    #[serde(default)]
    pub nodes: Vec<Repo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Language {
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Repo {
    pub name: String,
    /// Repositories without a description have `description: null`.
    #[serde(default)]
    pub description: String,
    #[serde(rename = "stargazerCount")]
    pub stargazer_count: u32,
    pub url: String,
    #[serde(rename = "primaryLanguage")]
    pub primary_language: Option<Language>,
}
