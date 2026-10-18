//! Secrets (API keys). On Windows they live in the Credential Manager, never in
//! SQLite or settings; tests use an in-memory store.

use std::sync::Mutex;

use crate::{Result, ServiceError};

#[cfg(windows)]
const SERVICE: &str = "sleuth";

pub(crate) enum Secrets {
    /// The OS credential store.
    System,
    /// In memory, for tests (and platforms without a supported store).
    Memory(Mutex<std::collections::HashMap<String, String>>),
}

impl Secrets {
    pub(crate) fn system() -> Self {
        if cfg!(windows) { Secrets::System } else { Secrets::memory() }
    }

    pub(crate) fn memory() -> Self {
        Secrets::Memory(Mutex::default())
    }

    pub(crate) fn get(&self, name: &str) -> Result<Option<String>> {
        match self {
            #[cfg(windows)]
            Secrets::System => match entry(name)?.get_password() {
                Ok(v) => Ok(Some(v)),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(store_err(e)),
            },
            #[cfg(not(windows))]
            Secrets::System => Ok(None),
            Secrets::Memory(m) => {
                Ok(m.lock().unwrap_or_else(|e| e.into_inner()).get(name).cloned())
            }
        }
    }

    pub(crate) fn set(&self, name: &str, value: &str) -> Result<()> {
        match self {
            #[cfg(windows)]
            Secrets::System => entry(name)?.set_password(value).map_err(store_err),
            #[cfg(not(windows))]
            Secrets::System => {
                Err(ServiceError::Internal("no credential store on this platform".into()))
            }
            Secrets::Memory(m) => {
                m.lock().unwrap_or_else(|e| e.into_inner()).insert(name.into(), value.into());
                Ok(())
            }
        }
    }

    pub(crate) fn delete(&self, name: &str) -> Result<()> {
        match self {
            #[cfg(windows)]
            Secrets::System => match entry(name)?.delete_credential() {
                Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(store_err(e)),
            },
            #[cfg(not(windows))]
            Secrets::System => Ok(()),
            Secrets::Memory(m) => {
                m.lock().unwrap_or_else(|e| e.into_inner()).remove(name);
                Ok(())
            }
        }
    }
}

#[cfg(windows)]
fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(store_err)
}

#[cfg(windows)]
fn store_err(e: keyring::Error) -> ServiceError {
    ServiceError::Internal(format!("credential store: {e}"))
}
