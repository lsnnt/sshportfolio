use serde::{Deserialize};

#[derive(Debug, Deserialize)]
pub struct GithubResponse {
    pub data: Data,
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
    pub nodes: Vec<Repo>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Repo {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "stargazerCount")]
    pub stargazer_count: u32,
    pub url: String,
}