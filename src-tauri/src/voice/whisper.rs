//! Local speech recognition with whisper.cpp, and its model manager.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use ts_rs::TS;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

use super::download;

const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// The models offered, smallest first. Sizes come from the server, not from
/// this list, so they're always accurate.
pub const MODELS: &[(&str, &str)] = &[
    ("tiny", "Fastest, least accurate. Fine for short commands"),
    ("tiny.en", "Fastest, English only"),
    ("base", "Fast and good enough for most questions"),
    (
        "base.en",
        "Fast, English only, a little more accurate in English",
    ),
    ("small", "Slower, noticeably more accurate"),
    ("small.en", "Slower, English only"),
    (
        "large-v3-turbo-q5_0",
        "Close to the best accuracy at a fraction of the size. Needs a fast computer",
    ),
    ("medium", "Slow on most laptops, very accurate"),
    (
        "large-v3-turbo",
        "Best accuracy that still runs at a usable speed on fast computers",
    ),
];

pub fn url(name: &str) -> String {
    format!("{BASE_URL}/ggml-{name}.bin")
}

pub fn path(app: &AppHandle, name: &str) -> Option<PathBuf> {
    Some(
        app.path()
            .app_data_dir()
            .ok()?
            .join("models")
            .join("whisper")
            .join(format!("ggml-{name}.bin")),
    )
}

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WhisperModel {
    pub name: String,
    pub description: String,
    pub installed: bool,
    /// Bytes on disk when installed, else the download size if known.
    #[ts(type = "number | null")]
    pub size: Option<u64>,
}

pub async fn list(app: &AppHandle, http: &reqwest::Client) -> Vec<WhisperModel> {
    let mut out = Vec::new();
    for (name, description) in MODELS {
        let local = path(app, name)
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len());
        let size = match local {
            Some(n) => Some(n),
            None => download::remote_size(http, &url(name)).await,
        };
        out.push(WhisperModel {
            name: name.to_string(),
            description: description.to_string(),
            installed: local.is_some(),
            size,
        });
    }
    out
}

/// Keeps the most recently used model loaded; loading takes a moment.
#[derive(Default)]
pub struct Loaded(Mutex<Option<(String, Arc<WhisperContext>)>>);

impl Loaded {
    fn get(&self, app: &AppHandle, name: &str) -> Result<Arc<WhisperContext>, String> {
        let mut slot = self.0.lock().unwrap();
        if let Some((n, ctx)) = slot.as_ref() {
            if n == name {
                return Ok(ctx.clone());
            }
        }
        let p = path(app, name).filter(|p| p.exists()).ok_or_else(|| {
            format!("The Whisper model \"{name}\" isn't downloaded. Download it in Settings → Voice input")
        })?;
        let ctx = WhisperContext::new_with_params(&p, WhisperContextParameters::default())
            .map_err(|e| format!("Couldn't load the Whisper model: {e}"))?;
        let ctx = Arc::new(ctx);
        *slot = Some((name.to_string(), ctx.clone()));
        Ok(ctx)
    }

    pub fn forget(&self) {
        *self.0.lock().unwrap() = None;
    }

    /// Transcribes 16 kHz mono audio. Blocking; run it off the async runtime.
    pub fn transcribe(
        &self,
        app: &AppHandle,
        name: &str,
        samples: &[f32],
        language: &str,
    ) -> Result<String, String> {
        let ctx = self.get(app, name)?;
        let mut state = ctx.create_state().map_err(|e| e.to_string())?;
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some(if language == "auto" { "auto" } else { language }));
        params.set_n_threads(
            std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32,
        );
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_suppress_blank(true);
        params.set_no_context(true);
        // Whisper needs at least a second of audio; pad short clips with silence.
        let mut padded;
        let audio = if samples.len() < 16_000 {
            padded = samples.to_vec();
            padded.resize(16_100, 0.0);
            &padded[..]
        } else {
            samples
        };
        state
            .full(params, audio)
            .map_err(|e| format!("Transcription failed: {e}"))?;
        let text: Vec<String> = state
            .as_iter()
            .filter_map(|seg| seg.to_str_lossy().ok().map(|s| s.trim().to_string()))
            .filter(|s| !s.is_empty() && !is_non_speech(s))
            .collect();
        Ok(text.join(" "))
    }
}

/// Whisper marks silence and noise with tags like "[BLANK_AUDIO]" or "(music)".
fn is_non_speech(s: &str) -> bool {
    let t = s.trim();
    (t.starts_with('[') && t.ends_with(']')) || (t.starts_with('(') && t.ends_with(')'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_speech_tags_are_dropped() {
        assert!(is_non_speech("[BLANK_AUDIO]"));
        assert!(is_non_speech(" (music) "));
        assert!(!is_non_speech("Open (the) folder"));
    }

    /// Runs only when HELPY_WHISPER_MODEL points at a ggml model and
    /// HELPY_SPEECH_WAV at a 16 kHz mono 16-bit WAV of someone speaking.
    #[test]
    #[ignore]
    fn transcribes_a_real_recording() {
        let model = std::env::var("HELPY_WHISPER_MODEL").unwrap();
        let wav = std::fs::read(std::env::var("HELPY_SPEECH_WAV").unwrap()).unwrap();
        let samples = super::super::dsp::pcm16_to_f32(&wav[44..]);
        let ctx =
            WhisperContext::new_with_params(&model, WhisperContextParameters::default()).unwrap();
        let mut state = ctx.create_state().unwrap();
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        state.full(params, &samples).unwrap();
        let text: String = state
            .as_iter()
            .filter_map(|s| s.to_str_lossy().ok().map(|t| t.to_string()))
            .collect();
        eprintln!("transcript: {text}");
        assert!(!text.trim().is_empty());
    }
}
