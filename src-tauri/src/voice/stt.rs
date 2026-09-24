//! Speech to text: local Whisper, OpenAI or Deepgram. Cloud requests go
//! through the same retry and backoff rules as every other provider call.

use std::time::Duration;

use serde_json::Value;
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

use super::dsp::{self, SPEECH_RATE};
use super::VoiceState;
use crate::ai::ask::AiState;
use crate::ai::error::{ErrorKind, ProviderError};
use crate::ai::limits::{self, Policy};
use crate::ai::{openai, secrets, sse};
use crate::settings::schema::SttEngine;
use crate::settings::Settings;

const DEEPGRAM_URL: &str = "https://api.deepgram.com/v1/listen";

pub async fn transcribe(
    app: &AppHandle,
    s: &Settings,
    samples: Vec<f32>,
) -> Result<String, String> {
    let vi = &s.voice_input;
    match vi.engine {
        SttEngine::Whisper => {
            let (app2, model, lang) = (app.clone(), vi.whisper_model.clone(), vi.language.clone());
            tokio::task::spawn_blocking(move || {
                app2.state::<VoiceState>()
                    .whisper
                    .transcribe(&app2, &model, &samples, &lang)
            })
            .await
            .map_err(|e| e.to_string())?
        }
        SttEngine::OpenAi | SttEngine::Deepgram => {
            cloud(app, s, samples).await.map_err(|e| e.message)
        }
    }
}

async fn cloud(app: &AppHandle, s: &Settings, samples: Vec<f32>) -> Result<String, ProviderError> {
    let vi = s.voice_input.clone();
    let wav = dsp::wav(&samples, SPEECH_RATE);
    let http = app.state::<AiState>().http.clone();
    let idle = Duration::from_secs(s.ai.timeout_secs as u64);
    let setup = |m: &str| ProviderError::new(ErrorKind::Setup, m.to_string());

    // Resolve the endpoint and key once; retries reuse them.
    let (url, auth, is_openai) = match vi.engine {
        SttEngine::OpenAi => {
            let id = vi.openai_provider_id.as_deref().ok_or_else(|| {
                setup("Choose an OpenAI provider for speech in Settings → Voice input")
            })?;
            let p =
                s.ai.provider(id)
                    .ok_or_else(|| setup("The speech provider isn't set up any more"))?;
            let key = secrets::key_for(p)?;
            (
                openai::url(&p.base_url, "audio/transcriptions"),
                key.map(|k| format!("Bearer {k}")),
                true,
            )
        }
        _ => {
            let key = secrets::get_speech("deepgram")?
                .ok_or_else(|| setup("Add a Deepgram key in Settings → Voice input"))?;
            let mut url = format!(
                "{DEEPGRAM_URL}?model={}&smart_format=true",
                vi.deepgram_model
            );
            if vi.language == "auto" {
                url.push_str("&detect_language=true");
            } else {
                url.push_str(&format!("&language={}", vi.language));
            }
            (url, Some(format!("Token {key}")), false)
        }
    };

    let cancel = CancellationToken::new();
    limits::run_step(
        &Policy::from(&s.limits),
        &[()],
        &cancel,
        // Speech is billed per minute of audio, not in tokens.
        |_| Ok(()),
        |_| {
            let (http, url, auth, wav, vi) = (
                http.clone(),
                url.clone(),
                auth.clone(),
                wav.clone(),
                vi.clone(),
            );
            async move {
                let mut req = if is_openai {
                    let file = reqwest::multipart::Part::bytes(wav)
                        .file_name("speech.wav")
                        .mime_str("audio/wav")
                        .unwrap();
                    let mut form = reqwest::multipart::Form::new()
                        .part("file", file)
                        .text("model", vi.openai_model.clone())
                        .text("response_format", "json");
                    if vi.language != "auto" {
                        form = form.text("language", vi.language.clone());
                    }
                    http.post(&url).multipart(form)
                } else {
                    http.post(&url)
                        .header("Content-Type", "audio/wav")
                        .body(wav)
                };
                if let Some(a) = auth {
                    req = req.header("Authorization", a);
                }
                let response = tokio::time::timeout(idle, req.send())
                    .await
                    .map_err(|_| {
                        ProviderError::new(
                            ErrorKind::Timeout,
                            "The speech service didn't respond in time",
                        )
                    })?
                    .map_err(|e| ProviderError::from_reqwest(&e))?;
                let v: Value = sse::check_status(response)
                    .await?
                    .json()
                    .await
                    .map_err(|e| ProviderError::from_reqwest(&e))?;
                let text = if is_openai {
                    v["text"].as_str()
                } else {
                    v["results"]["channels"][0]["alternatives"][0]["transcript"].as_str()
                };
                text.map(|t| t.trim().to_string()).ok_or_else(|| {
                    ProviderError::new(
                        ErrorKind::Malformed,
                        "The speech service sent an unexpected reply",
                    )
                })
            }
        },
        |_| {},
    )
    .await
    .map_err(|f| f.error)
}
