//! API keys live in the OS keychain (Keychain on macOS, Credential Manager
//! on Windows, Secret Service on Linux), never in the settings file.
//!
//! Debug builds use a file in the app data folder instead: each rebuild is a
//! new unsigned binary, so macOS would ask for the keychain password again,
//! twice per key, after every change.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use super::error::{ErrorKind, ProviderError};

/// Accounts already read this run. Each keychain read can raise a macOS
/// password prompt, so every account is read at most once per launch.
fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where debug builds keep their keys. Release builds ignore it.
pub fn init(data_dir: PathBuf) {
    let _ = DATA_DIR.set(data_dir);
}

#[cfg(not(debug_assertions))]
mod store {
    use super::{ErrorKind, ProviderError};

    const SERVICE: &str = "com.helpy.app";

    fn entry(account: &str) -> Result<keyring::Entry, ProviderError> {
        keyring::Entry::new(SERVICE, account).map_err(keychain_error)
    }

    fn keychain_error(e: keyring::Error) -> ProviderError {
        ProviderError::new(
            ErrorKind::Setup,
            format!("Couldn't use the system keychain: {e}"),
        )
    }

    pub fn get(account: &str) -> Result<Option<String>, ProviderError> {
        match entry(account)?.get_password() {
            Ok(k) => Ok(Some(k)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keychain_error(e)),
        }
    }

    pub fn set(account: &str, key: &str) -> Result<(), ProviderError> {
        entry(account)?.set_password(key).map_err(keychain_error)
    }

    pub fn delete(account: &str) -> Result<(), ProviderError> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keychain_error(e)),
        }
    }
}

#[cfg(debug_assertions)]
mod store {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{ErrorKind, ProviderError, DATA_DIR};

    fn path() -> Result<PathBuf, ProviderError> {
        DATA_DIR
            .get()
            .map(|d| d.join("dev-secrets.json"))
            .ok_or_else(|| error("the app data folder isn't known yet"))
    }

    fn error(e: impl std::fmt::Display) -> ProviderError {
        ProviderError::new(ErrorKind::Setup, format!("Couldn't use the dev key file: {e}"))
    }

    fn load() -> Result<HashMap<String, String>, ProviderError> {
        match std::fs::read_to_string(path()?) {
            Ok(text) => serde_json::from_str(&text).map_err(error),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
            Err(e) => Err(error(e)),
        }
    }

    fn save(all: &HashMap<String, String>) -> Result<(), ProviderError> {
        let path = path()?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(error)?;
        }
        std::fs::write(&path, serde_json::to_string_pretty(all).map_err(error)?).map_err(error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).map_err(error)?;
        }
        Ok(())
    }

    pub fn get(account: &str) -> Result<Option<String>, ProviderError> {
        Ok(load()?.remove(account))
    }

    pub fn set(account: &str, key: &str) -> Result<(), ProviderError> {
        let mut all = load()?;
        all.insert(account.into(), key.into());
        save(&all)
    }

    pub fn delete(account: &str) -> Result<(), ProviderError> {
        let mut all = load()?;
        if all.remove(account).is_some() {
            save(&all)?;
        }
        Ok(())
    }
}

fn get_account(account: &str) -> Result<Option<String>, ProviderError> {
    // Held across the read, so concurrent callers wait for one prompt.
    let mut cache = cache().lock().unwrap();
    if let Some(k) = cache.get(account) {
        return Ok(k.clone());
    }
    let k = store::get(account)?;
    cache.insert(account.into(), k.clone());
    Ok(k)
}

fn set_account(account: &str, key: &str) -> Result<(), ProviderError> {
    store::set(account, key.trim())?;
    cache()
        .lock()
        .unwrap()
        .insert(account.into(), Some(key.trim().into()));
    Ok(())
}

fn delete_account(account: &str) -> Result<(), ProviderError> {
    store::delete(account)?;
    cache().lock().unwrap().insert(account.into(), None);
    Ok(())
}

/// Key of an AI provider, by provider id.
pub fn get(provider_id: &str) -> Result<Option<String>, ProviderError> {
    get_account(&format!("provider:{provider_id}"))
}

pub fn set(provider_id: &str, key: &str) -> Result<(), ProviderError> {
    set_account(&format!("provider:{provider_id}"), key)
}

pub fn delete(provider_id: &str) -> Result<(), ProviderError> {
    delete_account(&format!("provider:{provider_id}"))
}

/// The key to use for a provider. Providers that work without a key (local
/// servers, most OpenAI-compatible ones) aren't blocked by an unreadable
/// keychain; they just go without a key.
pub fn key_for(
    p: &crate::settings::schema::ProviderConfig,
) -> Result<Option<String>, ProviderError> {
    match get(&p.id) {
        Err(_) if !p.kind.requires_key() => Ok(None),
        other => other,
    }
}

/// Key of a tool service agents use (Brave Search).
pub fn get_service(service: &str) -> Result<Option<String>, ProviderError> {
    get_account(&format!("tool:{service}"))
}

pub fn set_service(service: &str, key: &str) -> Result<(), ProviderError> {
    if key.trim().is_empty() {
        return delete_account(&format!("tool:{service}"));
    }
    set_account(&format!("tool:{service}"), key)
}

/// Key of a speech service that isn't an AI provider (Deepgram).
pub fn get_speech(service: &str) -> Result<Option<String>, ProviderError> {
    get_account(&format!("speech:{service}"))
}

pub fn set_speech(service: &str, key: &str) -> Result<(), ProviderError> {
    if key.trim().is_empty() {
        return delete_account(&format!("speech:{service}"));
    }
    set_account(&format!("speech:{service}"), key)
}

#[cfg(all(test, debug_assertions))]
mod tests {
    #[test]
    fn dev_store_round_trips_a_key() {
        let dir = tempfile::tempdir().unwrap();
        super::init(dir.path().to_path_buf());
        super::set("test", " sk-123 ").unwrap();
        assert_eq!(super::get("test").unwrap().as_deref(), Some("sk-123"));
        let file = std::fs::read_to_string(dir.path().join("dev-secrets.json")).unwrap();
        assert!(file.contains("provider:test"));
        super::delete("test").unwrap();
        assert_eq!(super::get("test").unwrap(), None);
    }
}
