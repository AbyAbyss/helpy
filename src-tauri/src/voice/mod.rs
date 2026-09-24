//! Voice: listening sessions (hotkey or wake word), speech to text, and
//! reading answers aloud.

pub mod audio;
pub mod download;
pub mod dsp;
pub mod piper;
pub mod speak;
pub mod stt;
pub mod text;
pub mod whisper;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Listener, Manager};
use ts_rs::TS;

use crate::ai::ask::{self, AiState, AskEvent};
use crate::settings::schema::{ReadAloud, SttEngine, VoiceHotkeyMode};
use crate::settings::{Settings, SettingsStore};
use dsp::{meter_level, SilenceDetector, Vad, SPEECH_RATE};

pub const STATE_EVENT: &str = "voice://state";
pub const LEVEL_EVENT: &str = "voice://level";
pub const PARTIAL_EVENT: &str = "voice://partial";
pub const METER_EVENT: &str = "voice://meter";

/// Longest recording, so a stuck toggle can't record forever.
const MAX_RECORDING: Duration = Duration::from_secs(60);
/// A wake-word or toggle session ends if nobody speaks within this time.
const NO_SPEECH_TIMEOUT: Duration = Duration::from_secs(8);
const PARTIAL_EVERY: Duration = Duration::from_millis(1200);

/// What the waveform pill shows.
#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(tag = "phase", rename_all = "camelCase")]
#[ts(export)]
pub enum VoicePhase {
    Listening,
    Transcribing,
    /// The question went to the AI.
    Thinking {
        transcript: String,
    },
    /// Nothing to do; `message` explains why, if there's a reason.
    Idle {
        message: Option<String>,
    },
    Error {
        message: String,
    },
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Trigger {
    PushToTalk,
    Toggle,
    Wake,
}

struct Session {
    id: u64,
    stop: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

pub struct VoiceState {
    pub whisper: whisper::Loaded,
    pub speaker: speak::Speaker,
    session: Mutex<Option<Session>>,
    meter: Mutex<Option<Arc<AtomicBool>>>,
    wake: Mutex<Option<Arc<AtomicBool>>>,
    catalogue: tokio::sync::Mutex<Option<Value>>,
    next_id: AtomicU64,
}

impl VoiceState {
    pub fn new(app: AppHandle) -> Self {
        Self {
            whisper: whisper::Loaded::default(),
            speaker: speak::Speaker::new(app),
            session: Mutex::new(None),
            meter: Mutex::new(None),
            wake: Mutex::new(None),
            catalogue: tokio::sync::Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    fn listening(&self) -> bool {
        self.session.lock().unwrap().is_some()
    }
}

/// Registers the listeners that keep Escape captured only while needed.
pub fn setup(app: &AppHandle) {
    let handle = app.clone();
    app.listen(speak::SPEAKING_EVENT, move |_| update_escape(&handle));
    sync_wake(app);
}

fn emit_phase(app: &AppHandle, phase: VoicePhase) {
    match &phase {
        VoicePhase::Error { message } => log::warn!("voice: {message}"),
        VoicePhase::Idle { message: Some(m) } => log::info!("voice: {m}"),
        _ => {}
    }
    let _ = app.emit(STATE_EVENT, phase);
}

/// Escape is taken from other apps only while Helpy is listening, answering
/// a voice question or speaking.
fn update_escape(app: &AppHandle) {
    let v = app.state::<VoiceState>();
    crate::hotkeys::set_escape(app, v.listening() || v.speaker.is_speaking());
}

// ---------- Hotkeys ----------

pub fn hotkey_pressed(app: &AppHandle) {
    let mode = app.state::<SettingsStore>().get().hotkeys.voice_mode;
    let listening = app.state::<VoiceState>().listening();
    match mode {
        VoiceHotkeyMode::PushToTalk => start(app, Trigger::PushToTalk),
        VoiceHotkeyMode::Toggle if listening => stop_listening(app),
        VoiceHotkeyMode::Toggle => start(app, Trigger::Toggle),
    }
}

pub fn hotkey_released(app: &AppHandle) {
    if app.state::<SettingsStore>().get().hotkeys.voice_mode == VoiceHotkeyMode::PushToTalk {
        stop_listening(app);
    }
}

/// Escape: stop listening, stop the answer, stop speaking.
pub fn cancel(app: &AppHandle) {
    let v = app.state::<VoiceState>();
    if let Some(s) = v.session.lock().unwrap().as_ref() {
        s.cancel.store(true, Ordering::Relaxed);
        s.stop.store(true, Ordering::Relaxed);
    }
    v.speaker.stop();
    ask::cancel(app);
    crate::windows::hide_pill(app);
    emit_phase(app, VoicePhase::Idle { message: None });
    update_escape(app);
}

// ---------- Sessions ----------

pub fn start(app: &AppHandle, trigger: Trigger) {
    let v = app.state::<VoiceState>();
    let mut slot = v.session.lock().unwrap();
    if slot.is_some() {
        return;
    }
    // A new question interrupts whatever Helpy was saying.
    v.speaker.stop();
    if let Some(m) = v.meter.lock().unwrap().take() {
        m.store(true, Ordering::Relaxed);
    }
    let session = Session {
        id: v.next_id.fetch_add(1, Ordering::Relaxed),
        stop: Arc::new(AtomicBool::new(false)),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let (id, stop, cancel) = (session.id, session.stop.clone(), session.cancel.clone());
    *slot = Some(session);
    drop(slot);

    crate::windows::show_pill(app);
    update_escape(app);
    emit_phase(app, VoicePhase::Listening);
    let app = app.clone();
    thread::Builder::new()
        .name("helpy-voice".into())
        .spawn(move || {
            run_session(&app, trigger, &stop, &cancel);
            let v = app.state::<VoiceState>();
            let mut slot = v.session.lock().unwrap();
            if slot.as_ref().is_some_and(|s| s.id == id) {
                *slot = None;
            }
            drop(slot);
            update_escape(&app);
        })
        .expect("spawn voice session");
}

pub fn stop_listening(app: &AppHandle) {
    if let Some(s) = app.state::<VoiceState>().session.lock().unwrap().as_ref() {
        s.stop.store(true, Ordering::Relaxed);
    }
}

fn run_session(app: &AppHandle, trigger: Trigger, stop: &AtomicBool, cancel: &AtomicBool) {
    let s = app.state::<SettingsStore>().get();
    let vi = &s.voice_input;
    let mic = match audio::start(vi.microphone.clone(), vi.noise_suppression) {
        Ok(m) => m,
        Err(message) => return emit_phase(app, VoicePhase::Error { message }),
    };

    // Holding the key decides when push-to-talk ends; the others stop on silence.
    let silence = if trigger == Trigger::PushToTalk {
        0.0
    } else {
        vi.silence_seconds
    };
    let mut vad = SilenceDetector::new(silence);
    let mut recording: Vec<f32> = Vec::new();
    let started = Instant::now();
    let mut last_level = Instant::now();
    let mut peak = 0f32;
    let mut last_partial = Instant::now();
    let partial_busy = Arc::new(AtomicBool::new(false));

    loop {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        if stop.load(Ordering::Relaxed) || started.elapsed() > MAX_RECORDING {
            break;
        }
        match mic.blocks.recv_timeout(Duration::from_millis(50)) {
            Ok(block) => {
                recording.extend_from_slice(&block);
                peak = peak.max(meter_level(&block));
                if last_level.elapsed() >= Duration::from_millis(33) {
                    let _ = app.emit(LEVEL_EVENT, peak);
                    peak = 0.0;
                    last_level = Instant::now();
                }
                if vad.push(&block) == Vad::Finished {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return emit_phase(
                    app,
                    VoicePhase::Error {
                        message: "The microphone stopped working".into(),
                    },
                );
            }
        }
        if trigger != Trigger::PushToTalk
            && !vad.heard_speech()
            && started.elapsed() > NO_SPEECH_TIMEOUT
        {
            return emit_phase(
                app,
                VoicePhase::Idle {
                    message: Some("I didn't hear anything".into()),
                },
            );
        }
        // Live transcript while speaking, local Whisper only (cloud engines
        // would bill every partial).
        if vi.engine == SttEngine::Whisper
            && vad.heard_speech()
            && last_partial.elapsed() >= PARTIAL_EVERY
            && !partial_busy.swap(true, Ordering::Relaxed)
        {
            last_partial = Instant::now();
            let (app2, busy, audio) = (app.clone(), partial_busy.clone(), recording.clone());
            let (model, lang) = (vi.whisper_model.clone(), vi.language.clone());
            thread::spawn(move || {
                if let Ok(t) = app2
                    .state::<VoiceState>()
                    .whisper
                    .transcribe(&app2, &model, &audio, &lang)
                {
                    if !t.is_empty() {
                        let _ = app2.emit(PARTIAL_EVENT, t);
                    }
                }
                busy.store(false, Ordering::Relaxed);
            });
        }
    }
    drop(mic);

    if cancel.load(Ordering::Relaxed) {
        return;
    }
    if !vad.heard_speech() || recording.len() < SPEECH_RATE as usize / 4 {
        return emit_phase(
            app,
            VoicePhase::Idle {
                message: Some("I didn't hear anything".into()),
            },
        );
    }
    emit_phase(app, VoicePhase::Transcribing);
    let text = match tauri::async_runtime::block_on(stt::transcribe(app, &s, recording)) {
        Ok(t) => t,
        Err(message) => return emit_phase(app, VoicePhase::Error { message }),
    };
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    if text.trim().is_empty() {
        return emit_phase(
            app,
            VoicePhase::Idle {
                message: Some("I didn't catch that. Try again?".into()),
            },
        );
    }
    emit_phase(
        app,
        VoicePhase::Thinking {
            transcript: text.clone(),
        },
    );
    if let Err(message) = tauri::async_runtime::block_on(ask::ask(app, text, ask::Origin::Voice)) {
        emit_phase(app, VoicePhase::Error { message });
    }
}

// ---------- Reading answers aloud ----------

/// Feeds a voice question's answer to the speaker as it streams in.
pub struct Feed {
    app: AppHandle,
    splitter: Mutex<text::SentenceSplitter>,
    answer: Mutex<String>,
    steps_only: bool,
}

impl Feed {
    /// None when voice guidance is off.
    pub fn new(app: &AppHandle, s: &Settings) -> Option<Self> {
        s.voice_output.voice_guidance.then(|| Self {
            app: app.clone(),
            splitter: Mutex::default(),
            answer: Mutex::default(),
            steps_only: s.voice_output.read_aloud == ReadAloud::StepsOnly,
        })
    }

    pub fn on_event(&self, e: &AskEvent) {
        let speaker = &self.app.state::<VoiceState>().speaker;
        match e {
            AskEvent::Text { text } => {
                self.answer.lock().unwrap().push_str(text);
                if !self.steps_only {
                    for sentence in self.splitter.lock().unwrap().push(text) {
                        speaker.say(sentence);
                    }
                }
            }
            // The failed attempt's words are discarded, so stop saying them.
            AskEvent::Retry { .. } => {
                speaker.stop();
                self.splitter.lock().unwrap().reset();
                self.answer.lock().unwrap().clear();
            }
            AskEvent::Done { .. } => {
                if self.steps_only {
                    speaker.say(text::steps_only(&self.answer.lock().unwrap()));
                } else if let Some(rest) = self.splitter.lock().unwrap().finish() {
                    speaker.say(rest);
                }
            }
            AskEvent::Error { .. } => self.splitter.lock().unwrap().reset(),
            _ => {}
        }
    }
}

// ---------- Wake word ----------

/// Starts or stops wake-word listening to match settings.
pub fn sync_wake(app: &AppHandle) {
    let v = app.state::<VoiceState>();
    if let Some(stop) = v.wake.lock().unwrap().take() {
        stop.store(true, Ordering::Relaxed);
    }
    let s = app.state::<SettingsStore>().get();
    if !s.voice_input.wake_word {
        return;
    }
    let stop = Arc::new(AtomicBool::new(false));
    *v.wake.lock().unwrap() = Some(stop.clone());
    let app = app.clone();
    thread::Builder::new()
        .name("helpy-wake".into())
        .spawn(move || wake_loop(&app, &stop))
        .expect("spawn wake thread");
}

fn wake_loop(app: &AppHandle, stop: &AtomicBool) {
    let v = app.state::<VoiceState>();
    while !stop.load(Ordering::Relaxed) {
        // A session has the microphone; wait for it to finish.
        if v.listening() {
            thread::sleep(Duration::from_millis(300));
            continue;
        }
        let s = app.state::<SettingsStore>().get();
        let vi = s.voice_input.clone();
        if !whisper::path(app, &vi.whisper_model).is_some_and(|p| p.exists()) {
            log::warn!(
                "wake word needs the local Whisper model {}",
                vi.whisper_model
            );
            thread::sleep(Duration::from_secs(5));
            continue;
        }
        let mic = match audio::start(vi.microphone.clone(), vi.noise_suppression) {
            Ok(m) => m,
            Err(e) => {
                log::warn!("wake word: {e}");
                thread::sleep(Duration::from_secs(5));
                continue;
            }
        };
        let mut vad = SilenceDetector::new(0.6);
        let mut segment: Vec<f32> = Vec::new();
        loop {
            if stop.load(Ordering::Relaxed) || v.listening() {
                break;
            }
            let Ok(block) = mic.blocks.recv_timeout(Duration::from_millis(100)) else {
                continue;
            };
            let state = vad.push(&block);
            if vad.heard_speech() {
                segment.extend_from_slice(&block);
            }
            let long = segment.len() > SPEECH_RATE as usize * 3;
            if state == Vad::Finished || long {
                let heard = v
                    .whisper
                    .transcribe(app, &vi.whisper_model, &segment, &vi.language)
                    .unwrap_or_default();
                if text::heard_wake_phrase(&heard, &vi.wake_phrase) {
                    drop(mic);
                    start(app, Trigger::Wake);
                    break;
                }
                vad = SilenceDetector::new(0.6);
                segment.clear();
            }
        }
    }
}

// ---------- Commands for the settings page and the pill ----------

#[tauri::command]
pub fn voice_input_devices() -> Vec<String> {
    audio::input_devices()
}

/// Streams the microphone level to the settings page until stopped.
#[tauri::command]
pub fn voice_meter_start(
    app: AppHandle,
    device: Option<String>,
    denoise: bool,
) -> Result<(), String> {
    let v = app.state::<VoiceState>();
    if v.listening() {
        return Err("Helpy is listening right now".into());
    }
    if let Some(m) = v.meter.lock().unwrap().take() {
        m.store(true, Ordering::Relaxed);
    }
    let mic = audio::start(device, denoise)?;
    let stop = Arc::new(AtomicBool::new(false));
    *v.meter.lock().unwrap() = Some(stop.clone());
    thread::spawn(move || {
        let mut peak = 0f32;
        let mut last = Instant::now();
        while !stop.load(Ordering::Relaxed) {
            if let Ok(b) = mic.blocks.recv_timeout(Duration::from_millis(100)) {
                peak = peak.max(meter_level(&b));
            }
            if last.elapsed() >= Duration::from_millis(50) {
                let _ = app.emit_to(crate::windows::SETTINGS, METER_EVENT, peak);
                peak = 0.0;
                last = Instant::now();
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub fn voice_meter_stop(app: AppHandle) {
    if let Some(m) = app.state::<VoiceState>().meter.lock().unwrap().take() {
        m.store(true, Ordering::Relaxed);
    }
}

#[tauri::command]
pub async fn voice_whisper_models(app: AppHandle) -> Vec<whisper::WhisperModel> {
    let http = app.state::<AiState>().http.clone();
    whisper::list(&app, &http).await
}

#[tauri::command]
pub async fn voice_whisper_download(app: AppHandle, name: String) -> Result<(), String> {
    if !whisper::MODELS.iter().any(|(n, _)| *n == name) {
        return Err(format!("Unknown model {name}"));
    }
    let http = app.state::<AiState>().http.clone();
    let dest = whisper::path(&app, &name).ok_or("No app data folder")?;
    download::download(
        &app,
        &http,
        &whisper::url(&name),
        &dest,
        &format!("whisper:{name}"),
    )
    .await?;
    sync_wake(&app);
    Ok(())
}

#[tauri::command]
pub fn voice_whisper_delete(app: AppHandle, name: String) -> Result<(), String> {
    app.state::<VoiceState>().whisper.forget();
    let p = whisper::path(&app, &name).ok_or("No app data folder")?;
    std::fs::remove_file(p).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn voice_system_voices() -> Result<Vec<speak::SystemVoice>, String> {
    speak::system_voices()
}

async fn catalogue(app: &AppHandle) -> Result<Value, String> {
    let v = app.state::<VoiceState>();
    let mut slot = v.catalogue.lock().await;
    if slot.is_none() {
        *slot = Some(piper::catalogue(&app.state::<AiState>().http).await?);
    }
    Ok(slot.clone().unwrap())
}

#[tauri::command]
pub async fn voice_piper_voices(app: AppHandle) -> Result<Vec<piper::PiperVoice>, String> {
    if !piper::available() {
        return Err("Piper isn't available for this computer".into());
    }
    Ok(piper::list(&app, &catalogue(&app).await?))
}

#[tauri::command]
pub async fn voice_piper_install(app: AppHandle, key: String) -> Result<(), String> {
    let cat = catalogue(&app).await?;
    piper::install_voice(&app, &app.state::<AiState>().http.clone(), &cat, &key).await
}

#[tauri::command]
pub fn voice_piper_remove(app: AppHandle, key: String) -> Result<(), String> {
    piper::remove_voice(&app, &key)
}

#[tauri::command]
pub fn voice_play_sample(app: AppHandle) {
    let v = app.state::<VoiceState>();
    v.speaker.stop();
    v.speaker
        .say("Hi, I'm Helpy. This is how I'll sound when I read answers to you.");
}

#[tauri::command]
pub fn voice_stop_speaking(app: AppHandle) {
    app.state::<VoiceState>().speaker.stop();
}

#[tauri::command]
pub fn voice_set_deepgram_key(key: String) -> Result<(), String> {
    crate::ai::secrets::set_speech("deepgram", &key).map_err(|e| e.message)
}

#[tauri::command]
pub fn voice_has_deepgram_key() -> Result<bool, String> {
    crate::ai::secrets::get_speech("deepgram")
        .map(|k| k.is_some())
        .map_err(|e| e.message)
}

/// The pill was clicked: show the full conversation.
#[tauri::command]
pub fn voice_expand(app: AppHandle) {
    crate::windows::hide_pill(&app);
    crate::windows::show_ask(&app);
}

#[tauri::command]
pub fn voice_pill_hide(app: AppHandle) {
    crate::windows::hide_pill(&app);
}

#[tauri::command]
pub fn voice_cancel(app: AppHandle) {
    cancel(&app);
}

/// Features the settings page should hide on this computer.
#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct VoiceSupport {
    pub piper: bool,
}

#[tauri::command]
pub fn voice_support() -> VoiceSupport {
    VoiceSupport {
        piper: piper::available(),
    }
}
