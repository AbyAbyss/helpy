//! API keys live in the OS keychain (Keychain on macOS, Credential Manager
//! on Windows, Secret Service on Linux), never in the settings file.

use super::error::{ErrorKind, ProviderError};

const SERVICE: &str = "com.helpy.app";

fn entry(provider_id: &str) -> Result<keyring::Entry, ProviderError> {
    keyring::Entry::new(SERVICE, &format!("provider:{provider_id}")).map_err(keychain_error)
}

fn keychain_error(e: keyring::Error) -> ProviderError {
    ProviderError::new(
        ErrorKind::Setup,
        format!("Couldn't use the system keychain: {e}"),
    )
}

pub fn get(provider_id: &str) -> Result<Option<String>, ProviderError> {
    match entry(provider_id)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(keychain_error(e)),
    }
}

pub fn set(provider_id: &str, key: &str) -> Result<(), ProviderError> {
    entry(provider_id)?
        .set_password(key.trim())
        .map_err(keychain_error)
}

pub fn delete(provider_id: &str) -> Result<(), ProviderError> {
    match entry(provider_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keychain_error(e)),
    }
}
