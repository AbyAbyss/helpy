mod ai;
mod buddy;
mod capture;
mod cursor;
mod fullscreen;
mod hotkeys;
mod overlay;
mod platform;
mod settings;
mod tray;
mod windows;

use tauri::{AppHandle, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

use settings::{Settings, SettingsStore};

/// Reacts to a saved settings change. Windows pick changes up themselves from
/// the `settings://changed` event.
pub(crate) fn on_settings_changed(app: &AppHandle, prev: &Settings, next: &Settings) {
    if prev.hotkeys != next.hotkeys {
        hotkeys::sync(app, next);
    }
    if prev.general.launch_at_login != next.general.launch_at_login {
        apply_autostart(app, next.general.launch_at_login);
    }
    tray::sync(app, next);
}

fn apply_autostart(app: &AppHandle, enabled: bool) {
    let autostart = app.autolaunch();
    let result = if enabled {
        autostart.enable()
    } else {
        autostart.disable()
    };
    if let Err(e) = result {
        log::error!("couldn't change launch at login: {e}");
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(target_os = "linux")]
    platform::prefer_x11();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            windows::show_settings(app)
        }))
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(hotkeys::handle)
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(hotkeys::Hotkeys::default())
        .manage(overlay::Overlays::default())
        .manage(cursor::CursorShared::default())
        .manage(ai::ask::AskState::default())
        .setup(|app| {
            // A tray app: no Dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let handle = app.handle().clone();
            let store = SettingsStore::load(app.path().app_config_dir()?.join("settings.json"));
            let s = store.get();
            app.manage(store);
            app.manage(ai::ask::AiState {
                http: ai::provider::http_client(),
                ledger: ai::ledger::Ledger::load(app.path().app_data_dir()?.join("usage.json")),
            });

            tray::build(&handle, &s)?;
            hotkeys::sync(&handle, &s);
            overlay::sync(&handle);
            cursor::spawn(handle.clone());
            if handle.autolaunch().is_enabled().unwrap_or(false) != s.general.launch_at_login {
                apply_autostart(&handle, s.general.launch_at_login);
            }
            if !s.general.start_minimized {
                windows::show_settings(&handle);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing settings hides it; Helpy keeps running in the tray.
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == windows::SETTINGS || window.label() == windows::ASK {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            settings::settings_get,
            settings::settings_defaults,
            settings::settings_set,
            settings::settings_reset_section,
            settings::settings_reset_all,
            settings::settings_export,
            settings::settings_import,
            hotkeys::hotkeys_status,
            hotkeys::hotkeys_suspend,
            hotkeys::hotkeys_resume,
            cursor::overlay_ready,
            buddy::buddy_set_custom_image,
            buddy::buddy_custom_image,
            platform::platform_info,
            ai::ai_set_key,
            ai::ai_delete_key,
            ai::ai_has_key,
            ai::ai_list_models,
            ai::ai_test_provider,
            ai::ai_detect_local,
            ai::ai_usage_today,
            ai::ask::ask_send,
            ai::ask::ask_cancel,
            ai::ask::ask_reset,
            ai::ask::ask_screen_answer,
            ai::ask::ask_status,
            windows::ask_hide,
            windows::open_settings_section,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Helpy");

    app.run(|_, event| {
        // Only an explicit Quit (which passes an exit code) ends the app.
        if let RunEvent::ExitRequested {
            code: None, api, ..
        } = event
        {
            api.prevent_exit();
        }
    });
}
