//! System tray: menu and a state-dependent icon.

use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Manager, Wry};

use crate::settings::schema::BuiltinProfile;
use crate::settings::{self, Settings, SettingsStore};

const PROFILES: [(BuiltinProfile, &str, &str); 3] = [
    (BuiltinProfile::Beginner, "profile-beginner", "Beginner"),
    (BuiltinProfile::Expert, "profile-expert", "Expert"),
    (BuiltinProfile::Quiet, "profile-quiet", "Quiet"),
];

pub struct Tray {
    icon: TrayIcon,
    buddy: CheckMenuItem<Wry>,
    voice: CheckMenuItem<Wry>,
    capture: CheckMenuItem<Wry>,
    profiles: Vec<CheckMenuItem<Wry>>,
}

fn icon_for(paused: bool) -> Image<'static> {
    let bytes: &[u8] = match (cfg!(target_os = "macos"), paused) {
        (true, false) => include_bytes!("../icons/tray/idle-template.png"),
        (true, true) => include_bytes!("../icons/tray/paused-template.png"),
        (false, false) => include_bytes!("../icons/tray/idle.png"),
        (false, true) => include_bytes!("../icons/tray/paused.png"),
    };
    Image::from_bytes(bytes).expect("bundled tray icon")
}

fn tooltip(s: &Settings) -> &'static str {
    if s.privacy.capture_paused {
        "Helpy (screen capture paused)"
    } else {
        "Helpy"
    }
}

pub fn build(app: &AppHandle, s: &Settings) -> tauri::Result<()> {
    let buddy = CheckMenuItem::with_id(
        app,
        "buddy",
        "Show cursor buddy",
        true,
        s.buddy.enabled,
        None::<&str>,
    )?;
    let voice = CheckMenuItem::with_id(
        app,
        "voice",
        "Voice guidance",
        true,
        s.voice_output.voice_guidance,
        None::<&str>,
    )?;
    let capture = CheckMenuItem::with_id(
        app,
        "capture",
        "Pause screen capture",
        true,
        s.privacy.capture_paused,
        None::<&str>,
    )?;
    let profiles = PROFILES
        .iter()
        .map(|(p, id, label)| {
            CheckMenuItem::with_id(
                app,
                *id,
                *label,
                true,
                s.profiles.active == *p,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let profile_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> =
        profiles.iter().map(|p| p as _).collect();
    let profile_menu = Submenu::with_items(app, "Behavior profile", true, &profile_refs)?;
    // The agent panel and approval inbox arrive with the agent phases.
    let agents = MenuItem::with_id(app, "agents", "Agent panel", false, None::<&str>)?;
    let approvals = MenuItem::with_id(app, "approvals", "Approval inbox", false, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Helpy", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);

    let menu = Menu::with_items(
        app,
        &[
            &buddy,
            &voice,
            &capture,
            &sep()?,
            &profile_menu,
            &sep()?,
            &agents,
            &approvals,
            &sep()?,
            &settings_item,
            &quit,
        ],
    )?;

    let icon = TrayIconBuilder::with_id("helpy")
        .icon(icon_for(s.privacy.capture_paused))
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(tooltip(s))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .build(app)?;

    app.manage(Tray {
        icon,
        buddy,
        voice,
        capture,
        profiles,
    });
    Ok(())
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let s = app.state::<SettingsStore>().get();
    let set = |path: &str, value: serde_json::Value| {
        if let Err(e) = settings::settings_set(app.clone(), path.into(), value) {
            log::error!("tray couldn't change {path}: {e:?}");
        }
    };
    match event.id().as_ref() {
        "buddy" => set("buddy.enabled", (!s.buddy.enabled).into()),
        "voice" => set(
            "voiceOutput.voiceGuidance",
            (!s.voice_output.voice_guidance).into(),
        ),
        "capture" => set("privacy.capturePaused", (!s.privacy.capture_paused).into()),
        "settings" => crate::windows::show_settings(app),
        "quit" => app.exit(0),
        id => {
            if let Some((p, _, _)) = PROFILES.iter().find(|(_, pid, _)| *pid == id) {
                set("profiles.active", serde_json::to_value(p).unwrap());
            }
        }
    }
    // Menus toggle check marks on click by themselves; re-sync so they always
    // match the saved settings, even when a change was rejected.
    sync(app, &app.state::<SettingsStore>().get());
}

pub fn sync(app: &AppHandle, s: &Settings) {
    let Some(tray) = app.try_state::<Tray>() else {
        return;
    };
    let _ = tray.buddy.set_checked(s.buddy.enabled);
    let _ = tray.voice.set_checked(s.voice_output.voice_guidance);
    let _ = tray.capture.set_checked(s.privacy.capture_paused);
    for (item, (p, _, _)) in tray.profiles.iter().zip(PROFILES.iter()) {
        let _ = item.set_checked(s.profiles.active == *p);
    }
    let _ = tray.icon.set_icon(Some(icon_for(s.privacy.capture_paused)));
    let _ = tray.icon.set_icon_as_template(cfg!(target_os = "macos"));
    let _ = tray.icon.set_tooltip(Some(tooltip(s)));
}
