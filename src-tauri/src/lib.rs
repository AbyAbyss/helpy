mod app_windows;
mod hotkeys;
mod overlay;
mod settings;
mod state;
mod tray;

use std::collections::HashMap;
use std::sync::{Mutex, RwLock};

use tauri::{Manager, RunEvent};
use tauri_plugin_autostart::MacosLauncher;

use state::{AppState, RuntimeState};

#[tauri::command]
fn get_runtime_state(state: tauri::State<AppState>) -> RuntimeState {
    state.runtime.lock().expect("runtime").clone()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        // Must be first: a second launch focuses settings in the running instance instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            app_windows::open_settings(app, None);
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
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Tray app: no Dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let settings_path = config_dir.join("settings.json");
            let loaded = settings::store::load(&settings_path);
            let start_minimized = loaded.general.start_minimized;
            let launch_at_login = loaded.general.launch_at_login;

            app.manage(AppState {
                settings: RwLock::new(loaded),
                settings_path,
                data_dir,
                runtime: Mutex::new(RuntimeState::default()),
                hotkey_errors: Mutex::new(HashMap::new()),
                overlays: Mutex::new(Vec::new()),
            });

            let handle = app.handle();
            settings::sync_autostart(handle, launch_at_login);
            tray::create(handle)?;
            hotkeys::sync(handle);
            overlay::start(handle);

            let autostarted = std::env::args().any(|a| a == "--autostart");
            if !start_minimized && !autostarted {
                app_windows::open_settings(handle, None);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_runtime_state,
            settings::get_settings,
            settings::update_settings,
            settings::reset_settings_section,
            settings::reset_all_settings,
            settings::export_settings,
            settings::import_settings,
            settings::set_custom_buddy,
            settings::get_custom_buddy,
            hotkeys::hotkey_status,
            hotkeys::probe_hotkey,
            hotkeys::set_hotkeys_suspended,
            overlay::overlay_geometry,
            overlay::list_displays,
            overlay::show_overlay_check,
            overlay::clear_annotations,
            app_windows::platform_info,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Helpy");

    app.run(|_app, event| {
        // Closing the settings window must not quit: Helpy lives in the tray.
        // `app.exit(0)` from the tray menu passes a code and is allowed through.
        if let RunEvent::ExitRequested { api, code: None, .. } = event {
            api.prevent_exit();
        }
    });
}
