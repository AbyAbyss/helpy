//! Global hotkeys, registered from settings and re-registered whenever they change.
//!
//! A combination the OS refuses (usually because another app owns it) is recorded in
//! `AppState::hotkey_errors` and shown next to that hotkey in settings.

use std::collections::HashMap;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::settings::schema::{HotkeyAction, VoiceHotkeyMode};
use crate::settings::validate::parse_hotkey;
use crate::state::{update_runtime, AppState, BuddyActivity};

pub const STATUS_EVENT: &str = "hotkeys://status";

fn escape() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

/// Plugin handler for every registered shortcut.
pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if shortcut.id() == escape().id() {
        if event.state == ShortcutState::Pressed {
            crate::overlay::clear(app);
        }
        return;
    }
    let settings = app.state::<AppState>().settings();
    let Some(action) = HotkeyAction::ALL.into_iter().find(|a| {
        parse_hotkey(settings.hotkeys.get(*a)).map(|s| s.id() == shortcut.id()).unwrap_or(false)
    }) else {
        return;
    };

    // Push-to-talk is the only action that cares about release.
    if action == HotkeyAction::VoiceAsk && settings.hotkeys.voice_mode == VoiceHotkeyMode::PushToTalk {
        let activity = match event.state {
            ShortcutState::Pressed => BuddyActivity::Listening,
            ShortcutState::Released => BuddyActivity::Idle,
        };
        update_runtime(app, |rt| rt.activity = activity);
        if event.state == ShortcutState::Pressed {
            crate::overlay::notice(app, "Listening works in the next build. Voice input arrives in Phase 3.");
        }
        return;
    }
    if event.state != ShortcutState::Pressed {
        return;
    }
    dispatch(app, action);
}

pub fn dispatch(app: &AppHandle, action: HotkeyAction) {
    match action {
        HotkeyAction::VoiceAsk => {
            let listening = app.state::<AppState>().runtime.lock().expect("runtime").activity == BuddyActivity::Listening;
            let next = if listening { BuddyActivity::Idle } else { BuddyActivity::Listening };
            update_runtime(app, |rt| rt.activity = next);
            if !listening {
                crate::overlay::notice(app, "Listening works in the next build. Voice input arrives in Phase 3.");
            }
        }
        HotkeyAction::TextAsk => crate::overlay::notice(app, "Text ask arrives in Phase 2."),
        HotkeyAction::CircleToExplain => crate::overlay::notice(app, "Circle to explain arrives in Phase 5."),
        HotkeyAction::ClearAnnotations => crate::overlay::clear(app),
        HotkeyAction::PauseCapture => {
            update_runtime(app, |rt| rt.capture_paused = !rt.capture_paused);
            let paused = app.state::<AppState>().runtime.lock().expect("runtime").capture_paused;
            crate::overlay::notice(app, if paused { "Screen capture paused" } else { "Screen capture resumed" });
        }
        HotkeyAction::OpenAgentPanel => crate::overlay::notice(app, "The agent panel arrives in Phase 6."),
        HotkeyAction::OpenApprovalInbox => crate::overlay::notice(app, "The approval inbox arrives in Phase 7."),
        HotkeyAction::PauseAllAgents => crate::overlay::notice(app, "No agents are running."),
        HotkeyAction::OpenSettings => crate::app_windows::open_settings(app, None),
    }
}

/// Unregisters everything, then registers the current settings' hotkeys.
pub fn sync(app: &AppHandle) {
    let gs = app.global_shortcut();
    let state = app.state::<AppState>();
    let settings = state.settings();
    let escape_on = state.runtime.lock().expect("runtime").annotations_visible;

    if let Err(e) = gs.unregister_all() {
        log::warn!("unregister_all failed: {e}");
    }
    let mut errors: HashMap<String, String> = HashMap::new();
    for action in HotkeyAction::ALL {
        let raw = settings.hotkeys.get(action);
        if raw.is_empty() {
            continue;
        }
        let result = parse_hotkey(raw).and_then(|sc| gs.register(sc).map_err(|e| e.to_string()));
        if let Err(e) = result {
            log::warn!("hotkey {} ({raw}) not registered: {e}", action.key());
            errors.insert(
                action.key().to_string(),
                "Another app is already using this combination. Pick a different one.".to_string(),
            );
        }
    }
    if escape_on {
        let _ = gs.register(escape());
    }
    *state.hotkey_errors.lock().expect("hotkey errors") = errors.clone();
    let _ = app.emit(STATUS_EVENT, &errors);
}

pub fn set_escape_active(app: &AppHandle, active: bool) {
    let gs = app.global_shortcut();
    let registered = gs.is_registered(escape());
    if active && !registered {
        if let Err(e) = gs.register(escape()) {
            log::warn!("could not register Escape: {e}");
        }
    } else if !active && registered {
        let _ = gs.unregister(escape());
    }
}

#[tauri::command]
pub fn hotkey_status(state: tauri::State<AppState>) -> HashMap<String, String> {
    state.hotkey_errors.lock().expect("hotkey errors").clone()
}

/// Checks whether a combination is free by briefly registering it. Used by the recorder
/// to warn before saving. Combinations Helpy already owns count as free.
#[tauri::command]
pub fn probe_hotkey(app: AppHandle, accelerator: String) -> Result<bool, String> {
    let sc = parse_hotkey(&accelerator)?;
    let gs = app.global_shortcut();
    if gs.is_registered(sc) {
        return Ok(true);
    }
    match gs.register(sc) {
        Ok(()) => {
            let _ = gs.unregister(sc);
            Ok(true)
        }
        Err(_) => Ok(false),
    }
}

/// Releases every Helpy hotkey while the settings window records a new combination
/// (otherwise the OS would deliver Helpy's own combinations to Helpy, not the recorder).
#[tauri::command]
pub fn set_hotkeys_suspended(app: AppHandle, suspended: bool) {
    if suspended {
        let _ = app.global_shortcut().unregister_all();
    } else {
        sync(&app);
    }
}
