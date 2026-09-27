//! Reference pictures for explanations, found on Wikimedia Commons: free to
//! use, no account needed, and strong on diagrams (a heart, a cell, a map).
//! The picture is downloaded here and handed to the overlay as a data URL,
//! since the overlay loads nothing from the web itself.

use std::time::Duration;

use base64::Engine;
use serde_json::Value;

const API: &str = "https://commons.wikimedia.org/w/api.php";
/// Wikimedia asks every client to say who it is and how to reach its
/// makers; without that its rate limits are far lower.
const USER_AGENT: &str = "Helpy/1.0 (https://github.com/AbyAbyss/helpy)";
/// Longest wait for a rate limit to lift before giving up on a picture.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(3);
/// Width of the thumbnail asked for, pixels. Enough for a card on screen.
const THUMB_WIDTH: u32 = 800;
const MAX_BYTES: usize = 3_000_000;
const CANDIDATES: usize = 6;

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|e| e.to_string())
}

/// The first picture Commons finds for `query`, as a data URL.
pub async fn find(query: &str) -> Result<String, String> {
    let http = client()?;
    let width = THUMB_WIDTH.to_string();
    let limit = CANDIDATES.to_string();
    let search = || {
        http.get(API).query(&[
            ("action", "query"),
            ("format", "json"),
            ("generator", "search"),
            // The File: namespace.
            ("gsrnamespace", "6"),
            ("gsrsearch", query),
            ("gsrlimit", limit.as_str()),
            ("prop", "imageinfo"),
            ("iiprop", "url|mime"),
            ("iiurlwidth", width.as_str()),
        ])
    };
    let found: Value = send(search)
        .await?
        .json()
        .await
        .map_err(|e| format!("Couldn't read the picture search: {e}"))?;
    for url in thumbnails(&found) {
        if let Ok(data) = download(&http, &url).await {
            return Ok(data);
        }
    }
    Err(format!("No picture was found for \"{query}\"."))
}

/// Thumbnail links of the still images in a search result, best match first.
fn thumbnails(found: &Value) -> Vec<String> {
    let Some(pages) = found["query"]["pages"].as_object() else {
        return Vec::new();
    };
    let mut hits: Vec<(i64, String)> = pages
        .values()
        .filter_map(|p| {
            let info = p["imageinfo"].get(0)?;
            let mime = info["mime"].as_str()?;
            // Photos, drawings and diagrams; not sound, video or documents.
            if !matches!(mime, "image/jpeg" | "image/png" | "image/svg+xml" | "image/webp") {
                return None;
            }
            let url = info["thumburl"].as_str().or(info["url"].as_str())?;
            Some((p["index"].as_i64().unwrap_or(i64::MAX), url.to_string()))
        })
        .collect();
    hits.sort_by_key(|(i, _)| *i);
    hits.into_iter().map(|(_, u)| u).collect()
}

/// Sends a request, waiting once for a rate limit to lift when Wikimedia
/// says it will soon.
async fn send(
    request: impl Fn() -> reqwest::RequestBuilder,
) -> Result<reqwest::Response, String> {
    let fail = |e: reqwest::Error| format!("Couldn't reach Wikimedia Commons: {e}");
    let res = request().send().await.map_err(fail)?;
    if res.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Ok(res);
    }
    let wait = res
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok()?.parse().ok())
        .map(Duration::from_secs)
        .unwrap_or(MAX_RETRY_WAIT);
    if wait > MAX_RETRY_WAIT {
        return Err("Wikimedia Commons is limiting requests right now.".into());
    }
    tokio::time::sleep(wait).await;
    request().send().await.map_err(fail)
}

async fn download(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let res = send(|| http.get(url)).await?;
    let mime = res
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if !mime.starts_with("image/") || !res.status().is_success() {
        return Err(format!("not a picture ({mime})"));
    }
    let bytes = res.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES {
        return Err("too large".into());
    }
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn keeps_still_images_in_search_order() {
        let found = json!({ "query": { "pages": {
            "3": { "index": 2, "imageinfo": [{ "mime": "image/png", "thumburl": "https://x/b.png" }] },
            "1": { "index": 3, "imageinfo": [{ "mime": "video/webm", "thumburl": "https://x/v.jpg" }] },
            "2": { "index": 1, "imageinfo": [{ "mime": "image/svg+xml", "thumburl": "https://x/a.svg.png" }] },
            "4": { "index": 4 }
        }}});
        assert_eq!(thumbnails(&found), ["https://x/a.svg.png", "https://x/b.png"]);
        assert!(thumbnails(&json!({})).is_empty());
    }

    /// Needs the network; run with `cargo test -- --ignored`.
    #[tokio::test]
    #[ignore]
    async fn finds_a_heart_diagram() {
        let data = find("human heart diagram").await.unwrap();
        assert!(data.starts_with("data:image/"));
    }
}
