use reqwest::blocking::Client;
use crate::models::cfstruct::CFResponse;
pub fn rating(client: &Client) -> Result<(u16, u16), Box<dyn std::error::Error+Send+Sync+'static>> {
    let resp = client
        .get("https://codeforces.com/api/user.info?handles=lsnnt")
        .send()?;
    let body = resp.text()?;
    let json_body: CFResponse = serde_json::from_str(&body)?;
    Ok((
        json_body.result[0].rating as u16,
        json_body.result[0].max_rating as u16,
    ))
}
