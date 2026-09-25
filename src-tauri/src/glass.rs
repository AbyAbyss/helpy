//! Native translucency for settings, the agent panel and the dock card: vibrancy on
//! macOS, Mica or Acrylic on Windows 11. Each page asks which look it got
//! and draws translucent surfaces only then; on Linux and older Windows the
//! windows stay opaque (the dock card falls back to CSS glass).

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, WebviewWindow};

use crate::settings::schema::Theme;
use crate::windows::{AGENTS, DOCK_CARD, SETTINGS};

#[derive(Default)]
pub struct Glass(Mutex<HashMap<String, &'static str>>);

pub fn setup(app: &AppHandle) {
    let state = app.state::<Glass>();
    for label in [SETTINGS, AGENTS, DOCK_CARD] {
        if let Some(w) = app.get_webview_window(label) {
            if let Some(kind) = apply(&w) {
                state.0.lock().unwrap().insert(label.into(), kind);
            }
        }
    }
    let theme = app
        .state::<crate::settings::SettingsStore>()
        .get()
        .general
        .theme;
    follow_theme(app, theme);
}

/// Native materials take their light or dark tint from the window's theme.
pub fn follow_theme(app: &AppHandle, theme: Theme) {
    let t = match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    for label in [SETTINGS, AGENTS, DOCK_CARD] {
        if let Some(w) = app.get_webview_window(label) {
            let _ = w.set_theme(t);
        }
    }
}

/// The look the calling window got: "macos", "windows", or None.
#[tauri::command]
pub fn window_glass(window: WebviewWindow, state: tauri::State<Glass>) -> Option<&'static str> {
    state.0.lock().unwrap().get(window.label()).copied()
}

#[cfg(target_os = "macos")]
fn apply(w: &WebviewWindow) -> Option<&'static str> {
    use tauri::window::{Effect, EffectState, EffectsBuilder};
    let effects = if w.label() != DOCK_CARD {
        // The sidebar material, as in Finder and Notes; the content side
        // gets a light wash in CSS.
        EffectsBuilder::new()
            .effect(Effect::Sidebar)
            .state(EffectState::FollowsWindowActiveState)
            .build()
    } else {
        EffectsBuilder::new()
            .effect(Effect::Popover)
            .state(EffectState::Active)
            .radius(20.0)
            .build()
    };
    w.set_effects(effects).ok()?;
    let _ = w.set_shadow(true);
    Some("macos")
}

#[cfg(windows)]
fn apply(w: &WebviewWindow) -> Option<&'static str> {
    use tauri::window::{Effect, EffectsBuilder};
    let build = crate::platform::windows_build()?;
    let effect = if w.label() != DOCK_CARD && build >= 22000 {
        Effect::Mica
    } else if build >= 22523 {
        // Older builds only have an Acrylic that lags while dragging.
        Effect::Acrylic
    } else {
        return None;
    };
    w.set_effects(EffectsBuilder::new().effect(effect).build())
        .ok()?;
    // Gives undecorated windows Windows 11's rounded corners and shadow.
    let _ = w.set_shadow(true);
    Some("windows")
}

#[cfg(not(any(target_os = "macos", windows)))]
fn apply(_: &WebviewWindow) -> Option<&'static str> {
    None
}
