//! Codeforces rating, from the public `user.info` API.

use anyhow::{Context, Result};
use reqwest::Client;

use crate::models::cfstruct::CFResponse;

const USER_INFO: &str = "https://codeforces.com/api/user.info?handles=lsnnt";

/// `(current rating, max rating)`.
///
/// Unrated handles have `null` ratings and an unknown handle has an empty
/// `result`, both of which used to panic on the `result[0]` index.
pub async fn rating(client: &Client) -> Result<(u16, u16)> {
    let body = client
        .get(USER_INFO)
        .send()
        .await
        .context("Codeforces request failed")?
        .error_for_status()
        .context("Codeforces returned an error status")?
        .text()
        .await?;

    let parsed: CFResponse =
        serde_json::from_str(&body).context("unexpected Codeforces payload")?;

    let user = parsed
        .result
        .first()
        .context("Codeforces returned no user for that handle")?;

    Ok((clamp_rating(user.rating), clamp_rating(user.max_rating)))
}

/// Ratings fit in `u16` today, but clamp instead of casting blindly.
fn clamp_rating(rating: Option<i64>) -> u16 {
    rating.unwrap_or(0).clamp(0, u16::MAX as i64) as u16
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn null_ratings_become_zero() {
        assert_eq!(clamp_rating(None), 0);
        assert_eq!(clamp_rating(Some(-5)), 0);
        assert_eq!(clamp_rating(Some(1234)), 1234);
        assert_eq!(clamp_rating(Some(999_999)), u16::MAX);
    }

    #[test]
    fn unrated_user_deserializes() {
        let body = json!({
            "status": "OK",
            "result": [{"handle": "lsnnt", "rating": null, "maxRating": null}]
        })
        .to_string();
        let parsed: CFResponse = serde_json::from_str(&body).unwrap();
        assert_eq!(clamp_rating(parsed.result[0].rating), 0);
    }

    #[test]
    fn failed_status_has_no_result() {
        let body = json!({"status": "FAILED", "comment": "handle not found"}).to_string();
        let parsed: CFResponse = serde_json::from_str(&body).unwrap();
        assert!(parsed.result.is_empty());
    }
}
