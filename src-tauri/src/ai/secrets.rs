//! API keys live in the OS keychain (Keychain on macOS, Credential Manager
//! on Windows, Secret Service on Linux), never in the settings file.

use super::error::{ErrorKind, ProviderError};

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

fn get_account(account: &str) -> Result<Option<String>, ProviderError> {
    match entry(account)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keychain_error(e)),
    }
}

fn set_account(account: &str, key: &str) -> Result<(), ProviderError> {
    entry(account)?
        .set_password(key.trim())
        .map_err(keychain_error)
}

fn delete_account(account: &str) -> Result<(), ProviderError> {
    match entry(account)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keychain_error(e)),
    }
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
