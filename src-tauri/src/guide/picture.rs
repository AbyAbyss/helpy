//! Reference pictures for explanations, found on Wikimedia Commons: free to
//! use, no account needed, and strong on diagrams (a heart, a cell, a map).
//! The picture is downloaded here and handed to the overlay as a data URL,
//! since the overlay loads nothing from the web itself.

use std::time::Duration;

use base64::Engine;
use serde_json::Value;

const API: &str = "https://commons.wikimedia.org/w/api.php";
/// Wikimedia asks every client to say who it is.
const USER_AGENT: &str = "Helpy/1.0 (desktop assistant; reference pictures)";
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
    let found: Value = http
        .get(API)
        .query(&[
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
        .send()
        .await
        .map_err(|e| format!("Couldn't search for a picture: {e}"))?
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

async fn download(http: &reqwest::Client, url: &str) -> Result<String, String> {
    let res = http.get(url).send().await.map_err(|e| e.to_string())?;
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
