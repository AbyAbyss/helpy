//! System tray: menu plus an icon and tooltip that follow Helpy's state.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};
use serde_json::json;

use crate::state::{update_runtime, AppState, BuddyActivity, RuntimeState};

struct TrayHandles {
    icon: TrayIcon,
    buddy: CheckMenuItem<Wry>,
    voice: CheckMenuItem<Wry>,
    capture: CheckMenuItem<Wry>,
    current: std::sync::Mutex<TrayLook>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TrayLook {
    Idle,
    Listening,
    CapturePaused,
    AgentsRunning,
    ApprovalNeeded,
}

impl TrayLook {
    fn from(rt: &RuntimeState) -> Self {
        // Most urgent first.
        if rt.approval_needed {
            TrayLook::ApprovalNeeded
        } else if rt.activity == BuddyActivity::Listening {
            TrayLook::Listening
        } else if rt.capture_paused {
            TrayLook::CapturePaused
        } else if rt.agents_running > 0 {
            TrayLook::AgentsRunning
        } else {
            TrayLook::Idle
        }
    }

    fn icon(self) -> Image<'static> {
        let bytes: &'static [u8] = match self {
            TrayLook::Idle => include_bytes!("../../icons/tray/idle.png"),
            TrayLook::Listening => include_bytes!("../../icons/tray/listening.png"),
            TrayLook::CapturePaused => include_bytes!("../../icons/tray/paused.png"),
            TrayLook::AgentsRunning => include_bytes!("../../icons/tray/agents.png"),
            TrayLook::ApprovalNeeded => include_bytes!("../../icons/tray/approval.png"),
        };
        Image::from_bytes(bytes).expect("bundled tray icon")
    }

    fn tooltip(self, rt: &RuntimeState) -> String {
        match self {
            TrayLook::Idle => "Helpy".into(),
            TrayLook::Listening => "Helpy is listening".into(),
            TrayLook::CapturePaused => "Helpy (screen capture paused)".into(),
            TrayLook::AgentsRunning => format!("Helpy ({} agent{} running)", rt.agents_running, if rt.agents_running == 1 { "" } else { "s" }),
            TrayLook::ApprovalNeeded => "Helpy needs your approval".into(),
        }
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let s = app.state::<AppState>().settings();
    let rt = app.state::<AppState>().runtime.lock().expect("runtime").clone();

    let buddy = CheckMenuItem::with_id(app, "buddy", "Show cursor buddy", true, s.buddy.enabled, None::<&str>)?;
    let voice = CheckMenuItem::with_id(app, "voice", "Voice guidance", true, s.voice_output.guidance_enabled, None::<&str>)?;
    let capture = CheckMenuItem::with_id(app, "capture", "Pause screen capture", true, rt.capture_paused, None::<&str>)?;

    // Profiles land in Phase 9; the entries are here so the menu layout is final.
    let profiles = Submenu::with_id_and_items(
        app,
        "profiles",
        "Behavior profile",
        true,
        &[
            &CheckMenuItem::with_id(app, "profile-beginner", "Beginner", false, false, None::<&str>)?,
            &CheckMenuItem::with_id(app, "profile-expert", "Expert", false, false, None::<&str>)?,
            &CheckMenuItem::with_id(app, "profile-quiet", "Quiet", false, false, None::<&str>)?,
        ],
    )?;

    let menu = Menu::with_items(
        app,
        &[
            &buddy,
            &voice,
            &capture,
            &profiles,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "circle", "Circle to explain", false, None::<&str>)?,
            &MenuItem::with_id(app, "agents", "Agent panel", false, None::<&str>)?,
            &MenuItem::with_id(app, "inbox", "Approval inbox", false, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
            &MenuItem::with_id(app, "quit", "Quit Helpy", true, None::<&str>)?,
        ],
    )?;

    let look = TrayLook::from(&rt);
    let icon = TrayIconBuilder::with_id("helpy")
        .icon(look.icon())
        .tooltip(look.tooltip(&rt))
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| {
            // Left click opens settings; the menu stays on right click.
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                crate::app_windows::open_settings(tray.app_handle(), None);
            }
        })
        .build(app)?;

    app.manage(TrayHandles { icon, buddy, voice, capture, current: std::sync::Mutex::new(look) });
    Ok(())
}

fn on_menu(app: &AppHandle, id: &str) {
    let s = app.state::<AppState>().settings();
    let patch = match id {
        "buddy" => Some(json!({ "buddy": { "enabled": !s.buddy.enabled } })),
        "voice" => Some(json!({ "voiceOutput": { "guidanceEnabled": !s.voice_output.guidance_enabled } })),
        _ => None,
    };
    if let Some(p) = patch {
        match crate::settings::store::apply_patch(&s, p) {
            Ok(next) => {
                let _ = crate::settings::commit(app, next);
            }
            Err(e) => log::error!("tray toggle failed: {e}"),
        }
        return;
    }
    match id {
        "capture" => update_runtime(app, |rt| rt.capture_paused = !rt.capture_paused),
        "settings" => crate::app_windows::open_settings(app, None),
        "quit" => app.exit(0),
        _ => {}
    }
}

/// Brings check marks, icon, and tooltip in line with settings and runtime state.
pub fn sync(app: &AppHandle) {
    let Some(h) = app.try_state::<TrayHandles>() else { return };
    let state = app.state::<AppState>();
    let s = state.settings();
    let rt = state.runtime.lock().expect("runtime").clone();

    let _ = h.buddy.set_checked(s.buddy.enabled);
    let _ = h.voice.set_checked(s.voice_output.guidance_enabled);
    let _ = h.capture.set_checked(rt.capture_paused);

    let look = TrayLook::from(&rt);
    let mut cur = h.current.lock().expect("tray look");
    if *cur != look {
        *cur = look;
        let _ = h.icon.set_icon(Some(look.icon()));
    }
    let _ = h.icon.set_tooltip(Some(look.tooltip(&rt)));
}
