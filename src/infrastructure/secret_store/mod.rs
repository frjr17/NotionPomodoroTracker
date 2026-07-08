//! Notion token storage in the Secret Service keyring (GNOME Keyring via
//! libsecret D-Bus API). The token never touches the SQLite file or disk.

use thiserror::Error;

const SERVICE: &str = "notion-pomodoro-tracker";
const USER: &str = "notion-api-token";

#[derive(Debug, Error)]
pub enum SecretError {
    #[error("keyring error: {0}")]
    Keyring(String),
}

fn entry() -> Result<keyring::Entry, SecretError> {
    keyring::Entry::new(SERVICE, USER).map_err(|e| SecretError::Keyring(e.to_string()))
}

pub fn store_token(token: &str) -> Result<(), SecretError> {
    entry()?
        .set_password(token.trim())
        .map_err(|e| SecretError::Keyring(e.to_string()))
}

pub fn load_token() -> Result<Option<String>, SecretError> {
    match entry()?.get_password() {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(SecretError::Keyring(e.to_string())),
    }
}

pub fn delete_token() -> Result<(), SecretError> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(SecretError::Keyring(e.to_string())),
    }
}
