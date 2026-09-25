//! Speaking text aloud: a queue of sentences played one after another by the
//! chosen engine. Audio is made ahead on one thread and played on another,
//! so the next sentence is ready when the current one ends. `stop()` silences
//! everything at once, including queued sentences, by bumping a generation
//! counter both threads check.

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

/// The one system synthesizer. On macOS creating a second fails (the
/// backend registers an Objective-C class each time), and the crate's handle
/// isn't really thread-safe, so it's only ever used under this lock.
fn with_system<R>(f: impl FnOnce(&mut tts::Tts) -> Result<R, String>) -> Result<R, String> {
    static SYSTEM: Mutex<Option<tts::Tts>> = Mutex::new(None);
    let mut system = SYSTEM.lock().unwrap();
    if system.is_none() {
        *system = Some(
            tts::Tts::default().map_err(|e| format!("The system voice isn't available: {e}"))?,
        );
    }
    f(system.as_mut().unwrap())
}

pub fn system_voices() -> Result<Vec<SystemVoice>, String> {
    let mut out: Vec<SystemVoice> = with_system(|tts| tts.voices().map_err(|e| e.to_string()))?
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
    generation: Arc<AtomicU64>,
    sink: Option<rodio::MixerDeviceSink>,
}

/// A sentence ready to play.
enum Job {
    /// The system voice speaks it itself.
    System(u64, String),
    Audio {
        generation: u64,
        samples: Vec<f32>,
        rate: u32,
    },
    /// Nothing to play (stopped, or making the audio failed).
    Skip,
}

/// Makes audio for queued sentences as soon as they arrive, while the player
/// thread plays the ones before.
fn worker(
    app: AppHandle,
    rx: Receiver<(u64, String)>,
    generation: Arc<AtomicU64>,
    pending: Arc<AtomicUsize>,
) {
    let (jobs, queue) = mpsc::channel();
    let last_error = Arc::new(AtomicU64::new(u64::MAX));
    {
        let (app, generation, last_error) = (app.clone(), generation.clone(), last_error.clone());
        thread::Builder::new()
            .name("helpy-speech-play".into())
            .spawn(move || player(app, queue, generation, pending, last_error))
            .expect("spawn speech player");
    }
    while let Ok((gen, text)) = rx.recv() {
        let job = if gen != generation.load(Ordering::SeqCst) {
            Job::Skip
        } else {
            let settings = app.state::<SettingsStore>().get();
            match synthesize(&app, &settings, &text) {
                Ok(Some((samples, rate))) => Job::Audio {
                    generation: gen,
                    samples,
                    rate,
                },
                Ok(None) => Job::System(gen, text),
                Err(e) => {
                    report(&app, &last_error, gen, e);
                    Job::Skip
                }
            }
        };
        if jobs.send(job).is_err() {
            break;
        }
    }
}

/// One message per interruption, not one per sentence.
fn report(app: &AppHandle, last_error: &AtomicU64, gen: u64, e: String) {
    if last_error.swap(gen, Ordering::SeqCst) != gen {
        log::warn!("speech failed: {e}");
        let _ = app.emit(SPEAK_ERROR_EVENT, e);
    }
}

fn player(
    app: AppHandle,
    queue: Receiver<Job>,
    generation: Arc<AtomicU64>,
    pending: Arc<AtomicUsize>,
    last_error: Arc<AtomicU64>,
) {
    let mut w = Worker {
        generation,
        sink: None,
    };
    let mut speaking = false;
    while let Ok(job) = queue.recv() {
        let (gen, result) = match job {
            Job::System(gen, text) if !w.cancelled(gen) => {
                if !speaking {
                    speaking = true;
                    let _ = app.emit(SPEAKING_EVENT, true);
                }
                let settings = app.state::<SettingsStore>().get();
                (gen, w.speak_system(&settings, gen, &text))
            }
            Job::Audio {
                generation: gen,
                samples,
                rate,
            } if !w.cancelled(gen) => {
                if !speaking {
                    speaking = true;
                    let _ = app.emit(SPEAKING_EVENT, true);
                }
                let volume = app.state::<SettingsStore>().get().voice_output.volume;
                (gen, w.play(samples, rate, volume, gen))
            }
            _ => (0, Ok(())),
        };
        if let Err(e) = result {
            report(&app, &last_error, gen, e);
        }
        if pending.fetch_sub(1, Ordering::SeqCst) == 1 && speaking {
            speaking = false;
            let _ = app.emit(SPEAKING_EVENT, false);
        }
    }
}

/// Audio for one sentence, or None when the system voice speaks it itself.
fn synthesize(
    app: &AppHandle,
    s: &Settings,
    text: &str,
) -> Result<Option<(Vec<f32>, u32)>, String> {
    let vo = &s.voice_output;
    match vo.engine {
        TtsEngine::System => Ok(None),
        // Piper can be chosen on a system where it no longer runs (macOS).
        TtsEngine::Piper if !piper::available() => Ok(None),
        TtsEngine::Piper => piper::synthesize(app, &vo.piper_voice, text, vo.speed).map(Some),
        // Offline mode keeps speech on this computer: the system voice
        // stands in for a cloud one.
        TtsEngine::OpenAi if s.privacy.offline && !local_voice(s) => Ok(None),
        TtsEngine::OpenAi => tauri::async_runtime::block_on(openai_speech(app, s, text)).map(Some),
    }
}

impl Worker {
    fn cancelled(&self, gen: u64) -> bool {
        self.generation.load(Ordering::SeqCst) != gen
    }

    fn speak_system(&mut self, s: &Settings, gen: u64, text: &str) -> Result<(), String> {
        let generation = self.generation.clone();
        let vo = &s.voice_output;
        with_system(|tts| {
            if let Some(id) = &vo.system_voice {
                if let Some(v) = tts
                    .voices()
                    .ok()
                    .and_then(|vs| vs.into_iter().find(|v| &v.id() == id))
                {
                    let _ = tts.set_voice(&v);
                }
            }
            let rate =
                (tts.normal_rate() * vo.speed as f32).clamp(tts.min_rate(), tts.max_rate());
            let _ = tts.set_rate(rate);
            let volume =
                tts.min_volume() + (tts.max_volume() - tts.min_volume()) * vo.volume as f32;
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
        })
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

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn system_voices_can_be_listed_twice() {
        assert!(!super::system_voices().unwrap().is_empty());
        assert!(!super::system_voices().unwrap().is_empty());
    }
}
