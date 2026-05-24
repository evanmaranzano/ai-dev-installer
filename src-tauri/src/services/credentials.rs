use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::error::AppError;
use crate::models::ApiKeyStatus;

const API_KEY_ENTRY: &str = "google-ai-studio-api-key";

pub trait SecretBackend: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>, AppError>;
    fn set(&self, key: &str, value: &str) -> Result<(), AppError>;
    fn delete(&self, key: &str) -> Result<(), AppError>;
}

#[derive(Clone, Default)]
pub struct MemorySecretBackend {
    secrets: Arc<Mutex<HashMap<String, String>>>,
}

impl SecretBackend for MemorySecretBackend {
    fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        let secrets = self.secrets.lock().map_err(|_| AppError {
            code: "secret_backend_unavailable".to_string(),
            message: "Secret backend is unavailable".to_string(),
            details: None,
        })?;

        Ok(secrets.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let mut secrets = self.secrets.lock().map_err(|_| AppError {
            code: "secret_backend_unavailable".to_string(),
            message: "Secret backend is unavailable".to_string(),
            details: None,
        })?;

        secrets.insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), AppError> {
        let mut secrets = self.secrets.lock().map_err(|_| AppError {
            code: "secret_backend_unavailable".to_string(),
            message: "Secret backend is unavailable".to_string(),
            details: None,
        })?;

        secrets.remove(key);
        Ok(())
    }
}

pub struct PlaceholderSecretBackend;

impl SecretBackend for PlaceholderSecretBackend {
    fn get(&self, _key: &str) -> Result<Option<String>, AppError> {
        Err(AppError {
            code: "credential_backend_unimplemented".to_string(),
            message: "Credential backend is not configured in this environment".to_string(),
            details: None,
        })
    }

    fn set(&self, _key: &str, _value: &str) -> Result<(), AppError> {
        Err(AppError {
            code: "credential_backend_unimplemented".to_string(),
            message: "Credential backend is not configured in this environment".to_string(),
            details: None,
        })
    }

    fn delete(&self, _key: &str) -> Result<(), AppError> {
        Err(AppError {
            code: "credential_backend_unimplemented".to_string(),
            message: "Credential backend is not configured in this environment".to_string(),
            details: None,
        })
    }
}

#[cfg(target_os = "windows")]
pub struct WindowsCredentialBackend;

#[cfg(target_os = "windows")]
impl SecretBackend for WindowsCredentialBackend {
    fn get(&self, key: &str) -> Result<Option<String>, AppError> {
        let entry = keyring::Entry::new("AI Dev Installer", key).map_err(|error| AppError {
            code: "credential_backend_unavailable".to_string(),
            message: "Credential backend is unavailable".to_string(),
            details: Some(error.to_string()),
        })?;

        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(AppError {
                code: "credential_backend_unavailable".to_string(),
                message: "Credential backend is unavailable".to_string(),
                details: Some(error.to_string()),
            }),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new("AI Dev Installer", key).map_err(|error| AppError {
            code: "credential_backend_unavailable".to_string(),
            message: "Credential backend is unavailable".to_string(),
            details: Some(error.to_string()),
        })?;

        entry.set_password(value).map_err(|error| AppError {
            code: "credential_write_failed".to_string(),
            message: "Failed to store API key".to_string(),
            details: Some(error.to_string()),
        })
    }

    fn delete(&self, key: &str) -> Result<(), AppError> {
        let entry = keyring::Entry::new("AI Dev Installer", key).map_err(|error| AppError {
            code: "credential_backend_unavailable".to_string(),
            message: "Credential backend is unavailable".to_string(),
            details: Some(error.to_string()),
        })?;

        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError {
                code: "credential_delete_failed".to_string(),
                message: "Failed to clear API key".to_string(),
                details: Some(error.to_string()),
            }),
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub struct UnsupportedPlatformSecretBackend;

#[cfg(not(target_os = "windows"))]
impl SecretBackend for UnsupportedPlatformSecretBackend {
    fn get(&self, _key: &str) -> Result<Option<String>, AppError> {
        Err(AppError {
            code: "unsupported_platform".to_string(),
            message: "Credential storage is only implemented for Windows in this build".to_string(),
            details: None,
        })
    }

    fn set(&self, _key: &str, _value: &str) -> Result<(), AppError> {
        Err(AppError {
            code: "unsupported_platform".to_string(),
            message: "Credential storage is only implemented for Windows in this build".to_string(),
            details: None,
        })
    }

    fn delete(&self, _key: &str) -> Result<(), AppError> {
        Err(AppError {
            code: "unsupported_platform".to_string(),
            message: "Credential storage is only implemented for Windows in this build".to_string(),
            details: None,
        })
    }
}

pub struct CredentialStore {
    backend: Box<dyn SecretBackend>,
}

impl CredentialStore {
    pub fn new(backend: Box<dyn SecretBackend>) -> Self {
        Self { backend }
    }

    pub fn api_key_status(&self) -> Result<ApiKeyStatus, AppError> {
        let value = self.backend.get(API_KEY_ENTRY)?;
        Ok(match value {
            Some(secret) if !secret.trim().is_empty() => ApiKeyStatus::Configured,
            _ => ApiKeyStatus::Missing,
        })
    }

    pub fn read_api_key(&self) -> Result<Option<String>, AppError> {
        self.backend.get(API_KEY_ENTRY)
    }

    pub fn save_api_key(&self, api_key: &str) -> Result<(), AppError> {
        let trimmed = api_key.trim();
        if trimmed.is_empty() {
            return Err(AppError {
                code: "invalid_api_key".to_string(),
                message: "API key is required".to_string(),
                details: None,
            });
        }

        self.backend.set(API_KEY_ENTRY, trimmed)
    }

    pub fn clear_api_key(&self) -> Result<(), AppError> {
        self.backend.delete(API_KEY_ENTRY)
    }
}

#[cfg(target_os = "windows")]
pub fn production_credential_store() -> CredentialStore {
    CredentialStore::new(Box::new(WindowsCredentialBackend))
}

#[cfg(not(target_os = "windows"))]
pub fn production_credential_store() -> CredentialStore {
    CredentialStore::new(Box::new(UnsupportedPlatformSecretBackend))
}
