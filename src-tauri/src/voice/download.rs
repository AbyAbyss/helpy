//! Downloads models and engines with progress events. Files arrive as
//! `.part` and are renamed only once complete, so a half-finished download is
//! never mistaken for a model.

use std::path::Path;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::io::AsyncWriteExt;
use ts_rs::TS;

pub const PROGRESS_EVENT: &str = "voice://download";

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DownloadProgress {
    /// What is downloading, e.g. "whisper:base" or "piper:en_US-lessac-medium".
    pub item: String,
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number | null")]
    pub total: Option<u64>,
}

/// Size of a remote file, following redirects. None when the server won't say.
pub async fn remote_size(http: &reqwest::Client, url: &str) -> Option<u64> {
    let r = http.head(url).send().await.ok()?;
    if !r.status().is_success() {
        return None;
    }
    r.content_length().filter(|n| *n > 0)
}

pub async fn download(
    app: &AppHandle,
    http: &reqwest::Client,
    url: &str,
    dest: &Path,
    item: &str,
) -> Result<(), String> {
    let fail = |e: &dyn std::fmt::Display| format!("Download failed: {e}");
    if let Some(dir) = dest.parent() {
        tokio::fs::create_dir_all(dir).await.map_err(|e| fail(&e))?;
    }
    let response = http.get(url).send().await.map_err(|e| fail(&e))?;
    if !response.status().is_success() {
        return Err(format!(
            "Download failed: the server answered {}",
            response.status()
        ));
    }
    let total = response.content_length();
    let part = dest.with_extension("part");
    let mut file = tokio::fs::File::create(&part).await.map_err(|e| fail(&e))?;
    let mut done = 0u64;
    let mut last_report = 0u64;
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| fail(&e))?;
        file.write_all(&chunk).await.map_err(|e| fail(&e))?;
        done += chunk.len() as u64;
        // Report about every 1 MB, not on every chunk.
        if done - last_report > 1 << 20 || Some(done) == total {
            last_report = done;
            let _ = app.emit(
                PROGRESS_EVENT,
                DownloadProgress {
                    item: item.into(),
                    done,
                    total,
                },
            );
        }
    }
    file.flush().await.map_err(|e| fail(&e))?;
    drop(file);
    if let Some(t) = total {
        if done != t {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(format!(
                "Download failed: got {done} of {t} bytes. Try again"
            ));
        }
    }
    tokio::fs::rename(&part, dest).await.map_err(|e| fail(&e))
}
