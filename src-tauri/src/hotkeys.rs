//! Global hotkeys. Every binding is validated and conflict-checked, but only
//! actions that exist in the current build are registered with the OS: a
//! registered hotkey takes the key combination away from every other app, so
//! registering one that does nothing yet would only get in the way.

use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};
use ts_rs::TS;

use crate::settings::{self, Settings, SettingsStore};

pub const STATUS_EVENT: &str = "hotkeys://status";

/// Actions that do something in this build.
const WIRED: &[&str] = &[
    "openSettings",
    "pauseCapture",
    "clearAnnotations",
    "circleToExplain",
    "pauseAllAgents",
    "openAgentPanel",
    "openApprovalInbox",
    "textAsk",
    "voiceAsk",
];

#[derive(Serialize, TS, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum HotkeyState {
    Active,
    Unbound,
    /// Saved and conflict-checked; starts working when its feature ships.
    NotYetAvailable,
    Failed,
}

#[derive(Serialize, TS, Clone, Debug)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HotkeyStatus {
    pub action: String,
    pub state: HotkeyState,
    pub error: Option<String>,
    /// Set when the combination is probably already used by the OS or by
    /// most apps. Registering it still works; this is advice.
    pub warning: Option<String>,
}

#[derive(Default)]
pub struct Hotkeys {
    by_id: Mutex<HashMap<u32, &'static str>>,
    status: Mutex<Vec<HotkeyStatus>>,
    /// Escape is captured only while something of Helpy's is open.
    escape: Mutex<bool>,
}

fn escape_shortcut() -> Shortcut {
    Shortcut::new(None, Code::Escape)
}

/// Captures or releases Escape. While captured, it cancels listening,
/// answering and speaking; otherwise other apps get it as usual.
pub fn set_escape(app: &AppHandle, on: bool) {
    {
        let state = app.state::<Hotkeys>();
        let mut wanted = state.escape.lock().unwrap();
        if *wanted == on {
            return;
        }
        *wanted = on;
    }
    // This is often called from inside a hotkey handler. The plugin waits on
    // the main thread to (un)register, which that handler is blocking, so
    // doing it here would deadlock. Apply it from another thread instead.
    let app = app.clone();
    std::thread::spawn(move || {
        // A later call may have changed its mind already.
        if *app.state::<Hotkeys>().escape.lock().unwrap() != on {
            return;
        }
        let gs = app.global_shortcut();
        let result = if on {
            gs.register(escape_shortcut())
        } else {
            gs.unregister(escape_shortcut())
        };
        if let Err(e) = result {
            log::warn!(
                "couldn't {} Escape: {e}",
                if on { "capture" } else { "release" }
            );
        }
    });
}

pub fn parse(accel: &str) -> Result<Shortcut, String> {
    let shortcut = Shortcut::from_str(accel)
        .map_err(|_| format!("\"{accel}\" isn't a key combination Helpy understands"))?;
    let is_function_key = matches!(
        shortcut.key,
        Code::F1
            | Code::F2
            | Code::F3
            | Code::F4
            | Code::F5
            | Code::F6
            | Code::F7
            | Code::F8
            | Code::F9
            | Code::F10
            | Code::F11
            | Code::F12
            | Code::F13
            | Code::F14
            | Code::F15
            | Code::F16
            | Code::F17
            | Code::F18
            | Code::F19
            | Code::F20
            | Code::F21
            | Code::F22
            | Code::F23
            | Code::F24
    );
    if shortcut.mods.is_empty() && !is_function_key {
        return Err(
            "Add Ctrl, Alt, Shift or Cmd so the key still types normally in other apps".into(),
        );
    }
    Ok(shortcut)
}

/// Combinations that the OS, or nearly every app, already uses.
fn os_warning(s: &Shortcut) -> Option<String> {
    #[cfg(target_os = "macos")]
    const RESERVED: &[&str] = &[
        "Super+Space",
        "Super+Tab",
        "Super+Q",
        "Super+W",
        "Super+H",
        "Super+M",
        "Super+Shift+3",
        "Super+Shift+4",
        "Super+Shift+5",
        "Control+Space",
        "Super+Alt+Escape",
        "Control+ArrowUp",
        "Control+ArrowDown",
        "Super+Alt+Space",
        "Control+Super+Q",
        "Control+Super+Space",
    ];
    #[cfg(target_os = "windows")]
    const RESERVED: &[&str] = &[
        "Alt+Tab",
        "Alt+F4",
        "Alt+Space",
        "Alt+Escape",
        "Control+Escape",
        "Control+Shift+Escape",
        "Super+L",
        "Super+D",
        "Super+E",
        "Super+R",
        "Super+Tab",
        "Super+Shift+S",
        "Super+V",
        "Super+Period",
        "Super+I",
        "Super+A",
    ];
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    const RESERVED: &[&str] = &[
        "Alt+Tab",
        "Alt+F4",
        "Alt+F2",
        "Control+Alt+T",
        "Control+Alt+Delete",
        "Control+Alt+ArrowLeft",
        "Control+Alt+ArrowRight",
        "Super+L",
        "Super+A",
        "Super+Tab",
    ];

    if RESERVED
        .iter()
        .filter_map(|r| Shortcut::from_str(r).ok())
        .any(|r| r.id() == s.id())
    {
        return Some("Your operating system already uses this combination".into());
    }
    let primary = if cfg!(target_os = "macos") {
        Modifiers::SUPER
    } else {
        Modifiers::CONTROL
    };
    let is_letter_or_digit =
        format!("{:?}", s.key).starts_with("Key") || format!("{:?}", s.key).starts_with("Digit");
    if s.mods == primary && is_letter_or_digit {
        let name = if cfg!(target_os = "macos") {
            "Cmd"
        } else {
            "Ctrl"
        };
        return Some(format!("Most apps use {name} + a letter (copy, paste, save). Helpy would take it from all of them"));
    }
    None
}

/// Re-registers hotkeys from settings and publishes their status.
pub fn sync(app: &AppHandle, settings: &Settings) {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let state = app.state::<Hotkeys>();
    let mut by_id = HashMap::new();
    let mut statuses = Vec::new();

    for (action, _, accel) in settings.hotkeys.actions() {
        let mut status = HotkeyStatus {
            action: action.into(),
            state: HotkeyState::Unbound,
            error: None,
            warning: None,
        };
        if !accel.is_empty() {
            match parse(accel) {
                Err(e) => {
                    status.state = HotkeyState::Failed;
                    status.error = Some(e);
                }
                Ok(shortcut) => {
                    status.warning = os_warning(&shortcut);
                    if !WIRED.contains(&action) {
                        status.state = HotkeyState::NotYetAvailable;
                    } else if let Err(e) = gs.register(shortcut) {
                        log::warn!("couldn't register {accel} for {action}: {e}");
                        status.state = HotkeyState::Failed;
                        status.error = Some("Another app has already claimed this combination. Pick a different one".into());
                    } else {
                        status.state = HotkeyState::Active;
                        by_id.insert(shortcut.id(), action);
                    }
                }
            }
        }
        statuses.push(status);
    }

    *state.by_id.lock().unwrap() = by_id;
    *state.status.lock().unwrap() = statuses.clone();
    if *state.escape.lock().unwrap() {
        let _ = gs.register(escape_shortcut());
    }
    let _ = app.emit(STATUS_EVENT, statuses);
}

pub fn handle(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if shortcut.id() == escape_shortcut().id() {
        if event.state == ShortcutState::Pressed {
            // Esc closes Circle to explain first; otherwise it stops voice,
            // answers and walkthroughs.
            if crate::circle::active(app) {
                crate::circle::end(app);
            } else {
                crate::voice::cancel(app);
            }
        }
        return;
    }
    let Some(action) = app
        .state::<Hotkeys>()
        .by_id
        .lock()
        .unwrap()
        .get(&shortcut.id())
        .copied()
    else {
        return;
    };
    // Push-to-talk needs the key release too; everything else acts on press.
    if action == "voiceAsk" {
        match event.state {
            ShortcutState::Pressed => crate::voice::hotkey_pressed(app),
            ShortcutState::Released => crate::voice::hotkey_released(app),
        }
        return;
    }
    if event.state != ShortcutState::Pressed {
        return;
    }
    match action {
        "openSettings" => crate::windows::show_settings(app),
        "textAsk" => crate::windows::show_ask(app),
        "circleToExplain" => crate::circle::toggle(app),
        "pauseAllAgents" => crate::agents::pause_all(app),
        "openAgentPanel" => crate::windows::show_agents(app, None),
        "openApprovalInbox" => crate::windows::show_agents(app, Some(crate::windows::INBOX.into())),
        "pauseCapture" => {
            let s = app.state::<SettingsStore>().get();
            let _ = settings::settings_set(
                app.clone(),
                "privacy.capturePaused".into(),
                (!s.privacy.capture_paused).into(),
            );
        }
        "clearAnnotations" => {
            let _ = app.emit(crate::overlay::CLEAR_EVENT, ());
        }
        _ => {}
    }
}

#[tauri::command]
pub fn hotkeys_status(state: tauri::State<Hotkeys>) -> Vec<HotkeyStatus> {
    state.status.lock().unwrap().clone()
}

/// Called while the settings page records a new combination, so pressing a
/// combination Helpy already owns reaches the page instead of firing.
#[tauri::command]
pub fn hotkeys_suspend(app: AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}

#[tauri::command]
pub fn hotkeys_resume(app: AppHandle) {
    sync(&app, &app.state::<SettingsStore>().get());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_keys_are_rejected_but_function_keys_are_allowed() {
        assert!(parse("A").is_err());
        assert!(parse("Space").is_err());
        assert!(parse("F9").is_ok());
        assert!(parse("Alt+Shift+Space").is_ok());
    }

    #[test]
    fn primary_plus_letter_warns() {
        let accel = if cfg!(target_os = "macos") {
            "Super+C"
        } else {
            "Control+C"
        };
        assert!(os_warning(&parse(accel).unwrap()).is_some());
        assert!(os_warning(&parse("Alt+Shift+C").unwrap()).is_none());
    }

    #[test]
    fn reserved_combination_warns() {
        assert!(os_warning(&parse("Alt+Tab").unwrap()).is_some() || cfg!(target_os = "macos"));
    }
}
