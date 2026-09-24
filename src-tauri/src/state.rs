//! Shared app state and the runtime (non-persisted) status that drives the buddy and tray.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, RwLock};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::overlay::OverlayGeometry;
use crate::settings::schema::Settings;

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
// Thinking and Speaking are driven by the AI and speech layers (Phases 2 and 3).
#[allow(dead_code)]
pub enum BuddyActivity {
    Idle,
    Listening,
    Thinking,
    Speaking,
}

/// Everything about "right now" that windows need to render. Not saved to disk.
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RuntimeState {
    pub activity: BuddyActivity,
    pub capture_paused: bool,
    pub agents_running: u32,
    /// An agent finished or needs approval.
    pub attention: bool,
    pub approval_needed: bool,
    pub annotations_visible: bool,
}

impl Default for RuntimeState {
    fn default() -> Self {
        Self {
            activity: BuddyActivity::Idle,
            capture_paused: false,
            agents_running: 0,
            attention: false,
            approval_needed: false,
            annotations_visible: false,
        }
    }
}

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub data_dir: PathBuf,
    pub runtime: Mutex<RuntimeState>,
    /// Hotkeys the OS refused to register, keyed by action (camelCase), with the reason.
    pub hotkey_errors: Mutex<HashMap<String, String>>,
    pub overlays: Mutex<Vec<OverlayGeometry>>,
}

impl AppState {
    pub fn settings(&self) -> Settings {
        self.settings.read().expect("settings lock").clone()
    }
}

pub const RUNTIME_EVENT: &str = "runtime://changed";

/// Mutates the runtime state, then tells every window and the tray.
pub fn update_runtime(app: &AppHandle, f: impl FnOnce(&mut RuntimeState)) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut rt = state.runtime.lock().expect("runtime lock");
        f(&mut rt);
        rt.clone()
    };
    let _ = app.emit(RUNTIME_EVENT, &snapshot);
    crate::tray::sync(app);
}
