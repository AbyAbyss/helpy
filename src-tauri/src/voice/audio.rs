//! Microphone capture. Each capture runs on its own thread, because audio
//! streams can't move between threads on every platform, and delivers 16 kHz
//! mono blocks over a channel.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rodio::cpal;
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::dsp::Pipeline;

pub struct Mic {
    pub blocks: Receiver<Vec<f32>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Mic {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

fn device_name(d: &cpal::Device) -> String {
    d.description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| "Microphone".into())
}

/// Names of the available input devices.
pub fn input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut names: Vec<String> = host
        .input_devices()
        .map(|ds| ds.map(|d| device_name(&d)).collect())
        .unwrap_or_default();
    names.dedup();
    names
}

fn no_mic() -> String {
    "No microphone found. Connect one, or pick another in Settings → Voice input".into()
}

fn permission_hint(e: impl std::fmt::Display) -> String {
    if cfg!(target_os = "macos") {
        format!("Couldn't open the microphone ({e}). Allow Helpy in System Settings → Privacy & Security → Microphone")
    } else {
        format!("Couldn't open the microphone: {e}")
    }
}

/// Starts capturing. `device` None or unknown falls back to the default input.
pub fn start(device: Option<String>, denoise: bool) -> Result<Mic, String> {
    let (tx, blocks) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = stop.clone();

    thread::Builder::new()
        .name("helpy-mic".into())
        .spawn(move || {
            let host = cpal::default_host();
            let chosen = device
                .as_deref()
                .and_then(|want| host.input_devices().ok()?.find(|d| device_name(d) == want));
            let Some(dev) = chosen.or_else(|| host.default_input_device()) else {
                let _ = ready_tx.send(Err(no_mic()));
                return;
            };
            let config = match dev.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    let _ = ready_tx.send(Err(permission_hint(e)));
                    return;
                }
            };
            let rate = config.sample_rate();
            let channels = config.channels() as usize;
            let mut pipeline = Pipeline::new(rate, channels, denoise);
            let mut send = move |f: Vec<f32>| {
                let out = pipeline.process(&f);
                if !out.is_empty() {
                    let _ = tx.send(out);
                }
            };
            let err = |e: cpal::StreamError| log::error!("microphone stream error: {e}");
            let stream_config = config.config();
            let stream = match config.sample_format() {
                cpal::SampleFormat::F32 => dev.build_input_stream(
                    &stream_config,
                    move |d: &[f32], _: &_| send(d.to_vec()),
                    err,
                    None,
                ),
                cpal::SampleFormat::I16 => dev.build_input_stream(
                    &stream_config,
                    move |d: &[i16], _: &_| send(d.iter().map(|s| *s as f32 / 32768.0).collect()),
                    err,
                    None,
                ),
                cpal::SampleFormat::U16 => dev.build_input_stream(
                    &stream_config,
                    move |d: &[u16], _: &_| {
                        send(d.iter().map(|s| (*s as f32 - 32768.0) / 32768.0).collect())
                    },
                    err,
                    None,
                ),
                other => {
                    let _ = ready_tx.send(Err(format!(
                        "This microphone uses an unsupported format ({other:?})"
                    )));
                    return;
                }
            };
            let stream = match stream.and_then(|s| {
                s.play()
                    .map(|_| s)
                    .map_err(|e| cpal::BuildStreamError::BackendSpecific {
                        err: cpal::BackendSpecificError {
                            description: e.to_string(),
                        },
                    })
            }) {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(permission_hint(e)));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            while !stop_flag.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(20));
            }
            drop(stream);
        })
        .map_err(|e| e.to_string())?;

    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => Ok(Mic { blocks, stop }),
        Ok(Err(e)) => Err(e),
        Err(_) => {
            stop.store(true, Ordering::Relaxed);
            Err("The microphone didn't respond".into())
        }
    }
}
