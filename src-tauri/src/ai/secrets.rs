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

/// The keychain as Helpy uses it: one entry per account. (Debug builds keep
/// keys in a file instead, so only release builds use these.)
#[cfg_attr(debug_assertions, allow(dead_code))]
trait Raw {
    fn get(&self, account: &str) -> Result<Option<String>, ProviderError>;
    fn set(&self, account: &str, value: &str) -> Result<(), ProviderError>;
    fn delete(&self, account: &str) -> Result<(), ProviderError>;
}

/// Every key in one keychain entry, read once per launch. macOS asks for the
/// keychain password per entry, so one entry means one prompt (none after
/// "Always Allow"), however many keys and logins Helpy keeps. Keys saved the
/// old way, an entry each, move in the first time they're read.
#[cfg_attr(debug_assertions, allow(dead_code))]
struct Bundle<R: Raw> {
    raw: R,
    keys: std::sync::Mutex<Option<HashMap<String, String>>>,
}

#[cfg_attr(debug_assertions, allow(dead_code))]
impl<R: Raw> Bundle<R> {
    const ACCOUNT: &'static str = "keys";

    fn new(raw: R) -> Self {
        Self { raw, keys: std::sync::Mutex::new(None) }
    }

    fn with<T>(&self, f: impl FnOnce(&mut HashMap<String, String>, &R) -> Result<T, ProviderError>) -> Result<T, ProviderError> {
        let mut keys = self.keys.lock().unwrap();
        if keys.is_none() {
            let all = match self.raw.get(Self::ACCOUNT)? {
                Some(text) => serde_json::from_str(&text).unwrap_or_default(),
                None => HashMap::new(),
            };
            *keys = Some(all);
        }
        f(keys.as_mut().expect("loaded above"), &self.raw)
    }

    fn save(raw: &R, all: &HashMap<String, String>) -> Result<(), ProviderError> {
        raw.set(Self::ACCOUNT, &serde_json::to_string(all).unwrap_or_default())
    }

    fn get(&self, account: &str) -> Result<Option<String>, ProviderError> {
        self.with(|all, raw| {
            if let Some(k) = all.get(account) {
                return Ok(Some(k.clone()));
            }
            // Saved before keys were kept together: move it in.
            let Some(k) = raw.get(account)? else { return Ok(None) };
            all.insert(account.into(), k.clone());
            Self::save(raw, all)?;
            let _ = raw.delete(account);
            Ok(Some(k))
        })
    }

    fn set(&self, account: &str, value: &str) -> Result<(), ProviderError> {
        self.with(|all, raw| {
            all.insert(account.into(), value.into());
            Self::save(raw, all)?;
            let _ = raw.delete(account);
            Ok(())
        })
    }

    fn delete(&self, account: &str) -> Result<(), ProviderError> {
        self.with(|all, raw| {
            if all.remove(account).is_some() {
                Self::save(raw, all)?;
            }
            raw.delete(account)
        })
    }
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

    /// One keychain entry per account.
    pub struct Keychain;

    impl super::Raw for Keychain {
        fn get(&self, account: &str) -> Result<Option<String>, ProviderError> {
            match entry(account)?.get_password() {
                Ok(k) => Ok(Some(k)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(keychain_error(e)),
            }
        }

        fn set(&self, account: &str, key: &str) -> Result<(), ProviderError> {
            entry(account)?.set_password(key).map_err(keychain_error)
        }

        fn delete(&self, account: &str) -> Result<(), ProviderError> {
            match entry(account)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(keychain_error(e)),
            }
        }
    }

    // macOS asks for the password per entry, so keys go together there.
    // Windows caps an entry at a few KB, so each key keeps its own.
    #[cfg(target_os = "macos")]
    fn keys() -> &'static super::Bundle<Keychain> {
        static KEYS: std::sync::OnceLock<super::Bundle<Keychain>> = std::sync::OnceLock::new();
        KEYS.get_or_init(|| super::Bundle::new(Keychain))
    }

    #[cfg(not(target_os = "macos"))]
    fn keys() -> &'static Keychain {
        &Keychain
    }

    pub fn get(account: &str) -> Result<Option<String>, ProviderError> {
        #[allow(unused_imports)]
        use super::Raw;
        keys().get(account)
    }

    pub fn set(account: &str, key: &str) -> Result<(), ProviderError> {
        #[allow(unused_imports)]
        use super::Raw;
        keys().set(account, key)
    }

    pub fn delete(account: &str) -> Result<(), ProviderError> {
        #[allow(unused_imports)]
        use super::Raw;
        keys().delete(account)
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

/// Key of a tool service agents use (Brave Search, Tavily).
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

#[cfg(test)]
mod bundle_tests {
    use super::*;

    /// A keychain that counts its reads, like the prompts macOS would show.
    #[derive(Default)]
    struct Fake {
        entries: std::sync::Mutex<HashMap<String, String>>,
        reads: std::sync::Mutex<Vec<String>>,
    }

    impl Raw for &Fake {
        fn get(&self, account: &str) -> Result<Option<String>, ProviderError> {
            self.reads.lock().unwrap().push(account.into());
            Ok(self.entries.lock().unwrap().get(account).cloned())
        }
        fn set(&self, account: &str, value: &str) -> Result<(), ProviderError> {
            self.entries.lock().unwrap().insert(account.into(), value.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<(), ProviderError> {
            self.entries.lock().unwrap().remove(account);
            Ok(())
        }
    }

    #[test]
    fn keys_live_in_one_entry_read_once() {
        let fake = Fake::default();
        let b = Bundle::new(&fake);
        b.set("provider:nebius", "nb").unwrap();
        b.set("tool:tavily", "tv").unwrap();
        // A fresh launch: every key comes from one read of one entry.
        let b = Bundle::new(&fake);
        fake.reads.lock().unwrap().clear();
        assert_eq!(b.get("provider:nebius").unwrap().as_deref(), Some("nb"));
        assert_eq!(b.get("tool:tavily").unwrap().as_deref(), Some("tv"));
        assert_eq!(*fake.reads.lock().unwrap(), ["keys"]);
        assert_eq!(fake.entries.lock().unwrap().len(), 1);
        b.delete("tool:tavily").unwrap();
        assert_eq!(Bundle::new(&fake).get("tool:tavily").unwrap(), None);
    }

    #[test]
    fn keys_saved_the_old_way_move_in() {
        let fake = Fake::default();
        fake.entries.lock().unwrap().insert("provider:openai".into(), "sk".into());
        let b = Bundle::new(&fake);
        assert_eq!(b.get("provider:openai").unwrap().as_deref(), Some("sk"));
        let entries = fake.entries.lock().unwrap().clone();
        assert!(!entries.contains_key("provider:openai"), "the old entry is gone");
        assert!(entries["keys"].contains("\"provider:openai\":\"sk\""));
        // Next launch it's read with the rest, not on its own.
        let b = Bundle::new(&fake);
        fake.reads.lock().unwrap().clear();
        assert_eq!(b.get("provider:openai").unwrap().as_deref(), Some("sk"));
        assert_eq!(*fake.reads.lock().unwrap(), ["keys"]);
    }
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
