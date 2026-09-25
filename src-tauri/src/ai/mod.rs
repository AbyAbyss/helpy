//! The AI provider layer: adapters, limits, spending, keys, and the ask flow.

pub mod anthropic;
pub mod ask;
pub mod call;
pub mod error;
pub mod gemini;
pub mod ledger;
pub mod limits;
pub mod openai;
pub mod provider;
pub mod secrets;
pub mod sse;
pub mod types;

use tauri::{AppHandle, Manager};

use crate::settings::schema::ProviderConfig;
use ask::AiState;
use ledger::UsageToday;
use provider::LocalServer;
use types::ModelInfo;

#[tauri::command]
pub fn ai_set_key(provider_id: String, key: String) -> Result<(), String> {
    if key.trim().is_empty() {
        return secrets::delete(&provider_id).map_err(|e| e.message);
    }
    secrets::set(&provider_id, &key).map_err(|e| e.message)
}

#[tauri::command]
pub fn ai_delete_key(provider_id: String) -> Result<(), String> {
    secrets::delete(&provider_id).map_err(|e| e.message)
}

/// Whether a key is stored. Errors when the keychain can't be read.
#[tauri::command]
pub fn ai_has_key(provider_id: String) -> Result<bool, String> {
    secrets::get(&provider_id)
        .map(|k| k.is_some())
        .map_err(|e| e.message)
}

/// Lists a provider's models. Takes the provider as edited on the page, so it
/// works before the edit is saved; the key comes from the keychain.
#[tauri::command]
pub async fn ai_list_models(
    app: AppHandle,
    provider: ProviderConfig,
) -> Result<Vec<ModelInfo>, String> {
    let key = secrets::key_for(&provider).map_err(|e| e.message)?;
    let http = app.state::<AiState>().http.clone();
    provider::list_models(&http, &provider, key.as_deref())
        .await
        .map_err(|e| e.message)
}

#[tauri::command]
pub async fn ai_test_provider(app: AppHandle, provider: ProviderConfig) -> Result<String, String> {
    let models = ai_list_models(app, provider).await?;
    Ok(match models.len() {
        0 => "Connected, but it has no models yet".into(),
        1 => "Connected. 1 model available".into(),
        n => format!("Connected. {n} models available"),
    })
}

#[tauri::command]
pub async fn ai_detect_local(app: AppHandle) -> Vec<LocalServer> {
    let http = app.state::<AiState>().http.clone();
    provider::detect_local(&http).await
}

#[tauri::command]
pub fn ai_usage_today(app: AppHandle) -> UsageToday {
    app.state::<AiState>().ledger.today()
}
