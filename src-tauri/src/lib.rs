mod agents;
mod ai;
mod buddy;
mod capture;
mod circle;
mod cursor;
mod fullscreen;
mod guide;
mod hotkeys;
mod overlay;
mod platform;
mod settings;
mod tray;
mod voice;
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
    if prev.voice_input != next.voice_input {
        if prev.voice_input.whisper_model != next.voice_input.whisper_model {
            app.state::<voice::VoiceState>().whisper.forget();
        }
        voice::sync_wake(app);
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
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_notification::init())
        .manage(hotkeys::Hotkeys::default())
        .manage(overlay::Overlays::default())
        .manage(cursor::CursorShared::default())
        .manage(ai::ask::AskState::default())
        .manage(guide::GuideState::default())
        .manage(circle::CircleState::default())
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

            app.manage(voice::VoiceState::new(handle.clone()));
            app.manage(agents::AgentsState::load(app.path().app_data_dir()?));
            tray::build(&handle, &s)?;
            hotkeys::sync(&handle, &s);
            overlay::sync(&handle);
            cursor::spawn(handle.clone());
            voice::setup(&handle);
            guide::setup(&handle);
            agents::setup(&handle);
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
            guide::guide_action,
            guide::guide_card,
            guide::guide_card_glass,
            circle::circle_select,
            circle::circle_action,
            circle::circle_close,
            circle::circle_copy,
            agents::agents_list,
            agents::agents_answer,
            agents::agents_pause,
            agents::agents_resume,
            agents::agents_cancel,
            agents::agents_retry,
            agents::agents_raise,
            agents::agents_follow_up,
            agents::agents_undo,
            agents::agents_rename,
            agents::agents_dismiss,
            agents::agents_seen,
            agents::agents_delete,
            agents::agents_duplicate,
            agents::agents_set_brave_key,
            agents::agents_has_brave_key,
            voice::voice_input_devices,
            voice::voice_meter_start,
            voice::voice_meter_stop,
            voice::voice_whisper_models,
            voice::voice_whisper_download,
            voice::voice_whisper_delete,
            voice::voice_system_voices,
            voice::voice_piper_voices,
            voice::voice_piper_install,
            voice::voice_piper_remove,
            voice::voice_play_sample,
            voice::voice_stop_speaking,
            voice::voice_set_deepgram_key,
            voice::voice_has_deepgram_key,
            voice::voice_expand,
            voice::voice_pill_hide,
            voice::voice_cancel,
            voice::voice_support,
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
