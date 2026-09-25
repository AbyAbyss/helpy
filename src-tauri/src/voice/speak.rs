//! Speaking text aloud: a queue of sentences played one after another by the
//! chosen engine. `stop()` silences everything at once, including queued
//! sentences, by bumping a generation counter the worker checks.

use std::num::NonZero;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use super::{dsp, piper};
use crate::ai::ask::AiState;
use crate::ai::{openai, secrets, sse};
use crate::settings::schema::TtsEngine;
use crate::settings::{Settings, SettingsStore};

pub const SPEAKING_EVENT: &str = "voice://speaking";
pub const SPEAK_ERROR_EVENT: &str = "voice://speak-error";

pub struct Speaker {
    tx: Mutex<Sender<(u64, String)>>,
    generation: Arc<AtomicU64>,
    pending: Arc<AtomicUsize>,
}

impl Speaker {
    pub fn new(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        let generation = Arc::new(AtomicU64::new(0));
        let pending = Arc::new(AtomicUsize::new(0));
        let (g, p) = (generation.clone(), pending.clone());
        thread::Builder::new()
            .name("helpy-speech".into())
            .spawn(move || worker(app, rx, g, p))
            .expect("spawn speech thread");
        Self {
            tx: Mutex::new(tx),
            generation,
            pending,
        }
    }

    pub fn say(&self, text: impl Into<String>) {
        let text = text.into();
        if text.trim().is_empty() {
            return;
        }
        self.pending.fetch_add(1, Ordering::SeqCst);
        let _ = self
            .tx
            .lock()
            .unwrap()
            .send((self.generation.load(Ordering::SeqCst), text));
    }

    /// Stops the current sentence and drops everything queued.
    pub fn stop(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    pub fn is_speaking(&self) -> bool {
        self.pending.load(Ordering::SeqCst) > 0
    }
}

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SystemVoice {
    pub id: String,
    pub name: String,
    pub language: String,
}

pub fn system_voices() -> Result<Vec<SystemVoice>, String> {
    let tts =
        tts::Tts::default().map_err(|e| format!("The system voices aren't available: {e}"))?;
    let mut out: Vec<SystemVoice> = tts
        .voices()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|v| SystemVoice {
            id: v.id(),
            name: v.name(),
            language: v.language().to_string(),
        })
        .collect();
    out.sort_by(|a, b| a.language.cmp(&b.language).then(a.name.cmp(&b.name)));
    Ok(out)
}

struct Worker {
    app: AppHandle,
    generation: Arc<AtomicU64>,
    sink: Option<rodio::MixerDeviceSink>,
    system: Option<tts::Tts>,
}

fn worker(
    app: AppHandle,
    rx: Receiver<(u64, String)>,
    generation: Arc<AtomicU64>,
    pending: Arc<AtomicUsize>,
) {
    let mut w = Worker {
        app: app.clone(),
        generation,
        sink: None,
        system: None,
    };
    let mut last_error_generation = u64::MAX;
    let mut speaking = false;
    while let Ok((gen, text)) = rx.recv() {
        if gen == w.generation.load(Ordering::SeqCst) {
            if !speaking {
                speaking = true;
                let _ = app.emit(SPEAKING_EVENT, true);
            }
            let settings = app.state::<SettingsStore>().get();
            if let Err(e) = w.speak(&settings, gen, &text) {
                // One message per interruption, not one per sentence.
                if gen != last_error_generation {
                    last_error_generation = gen;
                    log::warn!("speech failed: {e}");
                    let _ = app.emit(SPEAK_ERROR_EVENT, e);
                }
            }
        }
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 && speaking {
            speaking = false;
            let _ = app.emit(SPEAKING_EVENT, false);
        }
    }
}

impl Worker {
    fn cancelled(&self, gen: u64) -> bool {
        self.generation.load(Ordering::SeqCst) != gen
    }

    fn speak(&mut self, s: &Settings, gen: u64, text: &str) -> Result<(), String> {
        let vo = &s.voice_output;
        match vo.engine {
            TtsEngine::System => self.speak_system(s, gen, text),
            TtsEngine::Piper => {
                let (samples, rate) =
                    piper::synthesize(&self.app, &vo.piper_voice, text, vo.speed)?;
                self.play(samples, rate, vo.volume, gen)
            }
            // Offline mode keeps speech on this computer: the system voice
            // stands in for a cloud one.
            TtsEngine::OpenAi if s.privacy.offline && !local_voice(s) => {
                self.speak_system(s, gen, text)
            }
            TtsEngine::OpenAi => {
                let (samples, rate) =
                    tauri::async_runtime::block_on(openai_speech(&self.app, s, text))?;
                self.play(samples, rate, vo.volume, gen)
            }
        }
    }

    fn speak_system(&mut self, s: &Settings, gen: u64, text: &str) -> Result<(), String> {
        if self.system.is_none() {
            self.system = Some(
                tts::Tts::default()
                    .map_err(|e| format!("The system voice isn't available: {e}"))?,
            );
        }
        let generation = self.generation.clone();
        let tts = self.system.as_mut().unwrap();
        let vo = &s.voice_output;
        if let Some(id) = &vo.system_voice {
            if let Some(v) = tts
                .voices()
                .ok()
                .and_then(|vs| vs.into_iter().find(|v| &v.id() == id))
            {
                let _ = tts.set_voice(&v);
            }
        }
        let rate = (tts.normal_rate() * vo.speed as f32).clamp(tts.min_rate(), tts.max_rate());
        let _ = tts.set_rate(rate);
        let volume = tts.min_volume() + (tts.max_volume() - tts.min_volume()) * vo.volume as f32;
        let _ = tts.set_volume(volume);
        tts.speak(text, false).map_err(|e| e.to_string())?;
        // Some backends report "not speaking" for a moment after starting.
        thread::sleep(Duration::from_millis(150));
        while tts.is_speaking().unwrap_or(false) {
            if generation.load(Ordering::SeqCst) != gen {
                let _ = tts.stop();
                break;
            }
            thread::sleep(Duration::from_millis(30));
        }
        Ok(())
    }

    fn play(&mut self, samples: Vec<f32>, rate: u32, volume: f64, gen: u64) -> Result<(), String> {
        if self.cancelled(gen) || samples.is_empty() {
            return Ok(());
        }
        if self.sink.is_none() {
            let sink = rodio::DeviceSinkBuilder::open_default_sink()
                .map_err(|e| format!("No speaker or headphones found: {e}"))?;
            self.sink = Some(sink);
        }
        let player = rodio::Player::connect_new(self.sink.as_ref().unwrap().mixer());
        player.set_volume(volume as f32);
        let rate = NonZero::new(rate).ok_or("bad sample rate")?;
        player.append(rodio::buffer::SamplesBuffer::new(
            NonZero::new(1).unwrap(),
            rate,
            samples,
        ));
        while !player.empty() {
            if self.cancelled(gen) {
                player.stop();
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    }
}

/// OpenAI text to speech: raw 24 kHz 16-bit mono PCM.
fn local_voice(s: &Settings) -> bool {
    let p = s
        .voice_output
        .openai_provider_id
        .as_deref()
        .and_then(|id| s.ai.provider(id));
    p.is_some_and(crate::privacy::is_local)
}

async fn openai_speech(
    app: &AppHandle,
    s: &Settings,
    text: &str,
) -> Result<(Vec<f32>, u32), String> {
    let vo = &s.voice_output;
    let id = vo
        .openai_provider_id
        .as_deref()
        .ok_or("Choose an OpenAI provider for the voice in Settings → Voice output")?;
    let p =
        s.ai.provider(id)
            .ok_or("The voice provider isn't set up any more")?;
    let key = secrets::key_for(p).map_err(|e| e.message)?;
    let mut req = app
        .state::<AiState>()
        .http
        .post(openai::url(&p.base_url, "audio/speech"))
        .json(&json!({
            "model": vo.openai_model,
            "voice": vo.openai_voice,
            "input": text,
            "response_format": "pcm",
            "speed": vo.speed,
        }));
    if let Some(k) = key {
        req = req.bearer_auth(k);
    }
    let r = req.send().await.map_err(|e| e.to_string())?;
    let bytes = sse::check_status(r)
        .await
        .map_err(|e| e.message)?
        .bytes()
        .await
        .map_err(|e| e.to_string())?;
    Ok((dsp::pcm16_to_f32(&bytes), 24_000))
}
