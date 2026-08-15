//! Secret storage.
//!
//! Secrets go to the operating system's own credential store — Windows
//! Credential Manager — rather than sitting in plain text inside our settings
//! file. Windows encrypts the entry against the logged-in user account, so it
//! cannot be read by another user on the machine, or by anyone copying the
//! config directory off it.
//!
//! What this does not protect against: code already running as you. Any
//! process under your account can ask the credential store for it, exactly as
//! this app does. That is inherent to storing a credential the app must be
//! able to use unattended, and no local scheme changes it.

use keyring::v1::Entry;

/// Namespace for our entries in the credential store.
const SERVICE: &str = "Fooocus-Front";
const CIVITAI_USER: &str = "civitai-api-key";

fn entry() -> Option<Entry> {
    Entry::new(SERVICE, CIVITAI_USER).ok()
}

pub fn civitai_key() -> Option<String> {
    entry()?
        .get_password()
        .ok()
        .filter(|key| !key.trim().is_empty())
}

pub fn set_civitai_key(key: &str) -> bool {
    let Some(entry) = entry() else { return false };

    if key.trim().is_empty() {
        // Deleting a credential that was never stored is a success for us.
        let _ = entry.delete_credential();
        return true;
    }

    entry.set_password(key.trim()).is_ok()
}

/// True when the platform credential store is usable at all.
///
/// If it is not — an unusual Windows configuration, or a locked-down machine —
/// callers fall back to the settings file rather than losing the feature.
pub fn available() -> bool {
    entry().is_some()
}
