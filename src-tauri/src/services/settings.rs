use std::fs;
use std::path::PathBuf;

use crate::error::AppError;
use crate::models::{
    AppSettings, GeminiModelOption, SettingsConnectionResult, WritableAppSettings,
};
use crate::services::chat::ChatService;
use crate::services::credentials::{production_credential_store, CredentialStore};
use crate::services::image::ImageService;
use crate::services::subtitles::SubtitleService;

const DEFAULT_CHAT_MODEL: &str = "gemini-2.0-flash";
const DEFAULT_IMAGE_MODEL: &str = "gemini-2.0-flash-preview-image-generation";
const DEFAULT_EXPORT_DIR: &str = "C:/exports";
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

pub trait GeminiConnectivityClient: Send + Sync {
    fn list_models(
        &self,
        api_key: &str,
        timeout_ms: u64,
    ) -> Result<Vec<GeminiModelOption>, AppError>;
}

pub struct ReqwestGeminiConnectivityClient;

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModelsResponse {
    #[serde(default)]
    models: Vec<GeminiModelRecord>,
    next_page_token: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModelRecord {
    name: String,
    #[serde(default)]
    base_model_id: Option<String>,
    #[serde(default)]
    display_name: Option<String>,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

fn normalize_model_id(record: &GeminiModelRecord) -> String {
    record
        .base_model_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| {
            record
                .name
                .strip_prefix("models/")
                .unwrap_or(record.name.as_str())
                .to_string()
        })
}

fn supports_image_generation(model_id: &str, display_name: &str) -> bool {
    let normalized = format!("{model_id} {display_name}").to_ascii_lowercase();
    normalized.contains("image") || normalized.contains("imagen")
}

impl GeminiConnectivityClient for ReqwestGeminiConnectivityClient {
    fn list_models(
        &self,
        api_key: &str,
        timeout_ms: u64,
    ) -> Result<Vec<GeminiModelOption>, AppError> {
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_millis(timeout_ms))
            .build()
            .map_err(|error| AppError {
                code: "connection_test_setup_failed".to_string(),
                message: "Failed to prepare connection test".to_string(),
                details: Some(error.to_string()),
            })?;

        let mut page_token: Option<String> = None;
        let mut models = Vec::new();

        loop {
            let mut request = client
                .get("https://generativelanguage.googleapis.com/v1beta/models")
                .header("x-goog-api-key", api_key)
                .query(&[("pageSize", "1000")]);

            if let Some(token) = page_token.as_deref() {
                request = request.query(&[("pageToken", token)]);
            }

            let response = request.send().map_err(|error| AppError {
                code: "connection_test_failed".to_string(),
                message: "Failed to reach Gemini Developer API".to_string(),
                details: Some(error.to_string()),
            })?;

            let status = response.status();
            let body = response.text().map_err(|error| AppError {
                code: "connection_test_failed".to_string(),
                message: "Failed to read Gemini model list response".to_string(),
                details: Some(error.to_string()),
            })?;

            if !status.is_success() {
                return Err(AppError {
                    code: "connection_test_failed".to_string(),
                    message: format!("Gemini model list request failed with HTTP {}", status),
                    details: Some(body),
                });
            }

            let parsed: GeminiModelsResponse =
                serde_json::from_str(&body).map_err(|error| AppError {
                    code: "connection_test_failed".to_string(),
                    message: "Failed to parse Gemini model list response".to_string(),
                    details: Some(error.to_string()),
                })?;

            models.extend(parsed.models.into_iter().map(|record| {
                let model_id = normalize_model_id(&record);
                let display_name = record
                    .display_name
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or_else(|| model_id.clone());
                let supports_chat = record
                    .supported_generation_methods
                    .iter()
                    .any(|method| method == "generateContent");
                let supports_image = supports_image_generation(&model_id, &display_name);

                GeminiModelOption {
                    model_id,
                    display_name,
                    supported_generation_methods: record.supported_generation_methods,
                    supports_chat,
                    supports_image,
                }
            }));

            if let Some(next_page_token) = parsed.next_page_token {
                if next_page_token.trim().is_empty() {
                    break;
                }
                page_token = Some(next_page_token);
            } else {
                break;
            }
        }

        models.sort_by(|left, right| {
            left.display_name
                .cmp(&right.display_name)
                .then(left.model_id.cmp(&right.model_id))
        });
        models.dedup_by(|left, right| left.model_id == right.model_id);

        Ok(models)
    }
}

pub struct SettingsService {
    credential_store: CredentialStore,
    settings_path: PathBuf,
    connectivity_client: Box<dyn GeminiConnectivityClient>,
}

fn default_settings() -> WritableAppSettings {
    WritableAppSettings {
        default_chat_model: DEFAULT_CHAT_MODEL.to_string(),
        default_image_model: DEFAULT_IMAGE_MODEL.to_string(),
        default_export_dir: DEFAULT_EXPORT_DIR.to_string(),
        request_timeout_ms: DEFAULT_TIMEOUT_MS,
    }
}

fn default_settings_path() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join("AI Dev Installer").join("settings.json")
}

fn temp_settings_path(settings_path: &PathBuf) -> PathBuf {
    settings_path.with_extension("json.new")
}

fn backup_settings_path(settings_path: &PathBuf) -> PathBuf {
    settings_path.with_extension("json.bak")
}

fn read_settings_file(path: &PathBuf) -> Result<WritableAppSettings, AppError> {
    let content = fs::read_to_string(path).map_err(|error| AppError {
        code: "settings_read_failed".to_string(),
        message: "Failed to read app settings".to_string(),
        details: Some(format!("{}: {}", path.display(), error)),
    })?;

    serde_json::from_str(&content).map_err(|error| AppError {
        code: "settings_parse_failed".to_string(),
        message: "Failed to parse app settings".to_string(),
        details: Some(format!("{}: {}", path.display(), error)),
    })
}

impl SettingsService {
    pub fn new(
        credential_store: CredentialStore,
        settings_path: PathBuf,
        connectivity_client: Box<dyn GeminiConnectivityClient>,
    ) -> Self {
        Self {
            credential_store,
            settings_path,
            connectivity_client,
        }
    }

    pub fn production() -> Self {
        Self::new(
            production_credential_store(),
            default_settings_path(),
            Box::new(ReqwestGeminiConnectivityClient),
        )
    }

    fn restore_primary_settings_file(
        &self,
        source_path: &PathBuf,
        settings: &WritableAppSettings,
    ) -> Result<WritableAppSettings, AppError> {
        if let Some(parent) = self.settings_path.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError {
                code: "settings_recovery_failed".to_string(),
                message: "Failed to prepare app settings recovery directory".to_string(),
                details: Some(error.to_string()),
            })?;
        }

        match fs::rename(source_path, &self.settings_path) {
            Ok(()) => Ok(settings.clone()),
            Err(_) => {
                let json = serde_json::to_string_pretty(settings).map_err(|error| AppError {
                    code: "settings_recovery_failed".to_string(),
                    message: "Failed to serialize recovered app settings".to_string(),
                    details: Some(error.to_string()),
                })?;

                fs::write(&self.settings_path, json).map_err(|error| AppError {
                    code: "settings_recovery_failed".to_string(),
                    message: "Failed to restore recovered app settings".to_string(),
                    details: Some(format!(
                        "source={} target={} error={}",
                        source_path.display(),
                        self.settings_path.display(),
                        error
                    )),
                })?;

                Ok(settings.clone())
            }
        }
    }

    fn recover_missing_settings_file(&self) -> Result<WritableAppSettings, AppError> {
        let temp_path = temp_settings_path(&self.settings_path);
        let backup_path = backup_settings_path(&self.settings_path);
        let has_temp = temp_path.exists();
        let has_backup = backup_path.exists();

        if !has_temp && !has_backup {
            return Ok(default_settings());
        }

        if has_temp {
            match read_settings_file(&temp_path) {
                Ok(settings) => return self.restore_primary_settings_file(&temp_path, &settings),
                Err(temp_error) if has_backup => match read_settings_file(&backup_path) {
                    Ok(settings) => {
                        return self.restore_primary_settings_file(&backup_path, &settings);
                    }
                    Err(backup_error) => {
                        return Err(AppError {
                            code: "settings_recovery_failed".to_string(),
                            message: "Failed to recover app settings".to_string(),
                            details: Some(format!(
                                "temp_error={} backup_error={}",
                                temp_error
                                    .details
                                    .unwrap_or_else(|| temp_error.message.clone()),
                                backup_error
                                    .details
                                    .unwrap_or_else(|| backup_error.message.clone())
                            )),
                        });
                    }
                },
                Err(error) => {
                    return Err(AppError {
                        code: "settings_recovery_failed".to_string(),
                        message: "Failed to recover app settings".to_string(),
                        details: Some(
                            error.details.unwrap_or_else(|| {
                                "Temporary settings file was invalid".to_string()
                            }),
                        ),
                    });
                }
            }
        }

        let settings = read_settings_file(&backup_path).map_err(|error| AppError {
            code: "settings_recovery_failed".to_string(),
            message: "Failed to recover app settings".to_string(),
            details: Some(
                error
                    .details
                    .unwrap_or_else(|| "Backup settings file was invalid".to_string()),
            ),
        })?;

        self.restore_primary_settings_file(&backup_path, &settings)
    }

    fn read_writable_settings(&self) -> Result<WritableAppSettings, AppError> {
        if !self.settings_path.exists() {
            return self.recover_missing_settings_file();
        }

        read_settings_file(&self.settings_path)
    }

    fn write_writable_settings(&self, settings: &WritableAppSettings) -> Result<(), AppError> {
        if let Some(parent) = self.settings_path.parent() {
            fs::create_dir_all(parent).map_err(|error| AppError {
                code: "settings_write_failed".to_string(),
                message: "Failed to prepare app settings directory".to_string(),
                details: Some(error.to_string()),
            })?;
        }

        let json = serde_json::to_string_pretty(settings).map_err(|error| AppError {
            code: "settings_write_failed".to_string(),
            message: "Failed to serialize app settings".to_string(),
            details: Some(error.to_string()),
        })?;

        let temp_path = temp_settings_path(&self.settings_path);
        let backup_path = backup_settings_path(&self.settings_path);

        fs::write(&temp_path, json).map_err(|error| AppError {
            code: "settings_write_failed".to_string(),
            message: "Failed to persist app settings".to_string(),
            details: Some(error.to_string()),
        })?;

        let had_existing_file = self.settings_path.exists();
        if had_existing_file {
            let _ = fs::remove_file(&backup_path);
            fs::rename(&self.settings_path, &backup_path).map_err(|error| {
                let _ = fs::remove_file(&temp_path);
                AppError {
                    code: "settings_write_failed".to_string(),
                    message: "Failed to stage existing app settings for replacement".to_string(),
                    details: Some(error.to_string()),
                }
            })?;
        }

        if let Err(error) = fs::rename(&temp_path, &self.settings_path) {
            let _ = fs::remove_file(&temp_path);
            if had_existing_file {
                let _ = fs::rename(&backup_path, &self.settings_path);
            }
            return Err(AppError {
                code: "settings_write_failed".to_string(),
                message: "Failed to finalize app settings write".to_string(),
                details: Some(error.to_string()),
            });
        }

        if had_existing_file {
            let _ = fs::remove_file(&backup_path);
        }

        Ok(())
    }

    pub fn load(&self) -> Result<AppSettings, AppError> {
        let settings = self.read_writable_settings()?;

        Ok(AppSettings {
            api_key_status: self.credential_store.api_key_status()?,
            default_chat_model: settings.default_chat_model,
            default_image_model: settings.default_image_model,
            default_export_dir: settings.default_export_dir,
            request_timeout_ms: settings.request_timeout_ms,
        })
    }

    pub fn save_api_key(&self, api_key: &str) -> Result<AppSettings, AppError> {
        self.credential_store.save_api_key(api_key)?;
        self.load()
    }

    pub fn clear_api_key(&self) -> Result<AppSettings, AppError> {
        self.credential_store.clear_api_key()?;
        self.load()
    }

    pub fn save_app_settings(
        &self,
        settings: WritableAppSettings,
    ) -> Result<AppSettings, AppError> {
        self.write_writable_settings(&settings)?;
        self.load()
    }

    pub fn test_api_key_connection(&self) -> Result<SettingsConnectionResult, AppError> {
        let stored_api_key = self
            .credential_store
            .read_api_key()?
            .ok_or_else(|| AppError {
                code: "missing_api_key".to_string(),
                message: "API key not configured".to_string(),
                details: None,
            })?;

        let settings = self.read_writable_settings()?;
        self.connectivity_client
            .list_models(&stored_api_key, settings.request_timeout_ms)
            .map(|_| SettingsConnectionResult {
                ok: true,
                message: "连接成功".to_string(),
            })
    }

    pub fn list_available_models(&self) -> Result<Vec<GeminiModelOption>, AppError> {
        let stored_api_key = self
            .credential_store
            .read_api_key()?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError {
                code: "missing_api_key".to_string(),
                message: "API key not configured".to_string(),
                details: None,
            })?;

        let settings = self.read_writable_settings()?;
        self.connectivity_client
            .list_models(&stored_api_key, settings.request_timeout_ms)
    }

    pub fn chat_service(&self) -> Result<ChatService, AppError> {
        let stored_api_key = self
            .credential_store
            .read_api_key()?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError {
                code: "missing_api_key".to_string(),
                message: "API key not configured".to_string(),
                details: None,
            })?;

        let settings = self.read_writable_settings()?;
        Ok(ChatService::production(
            stored_api_key,
            settings.request_timeout_ms,
        ))
    }

    pub fn image_service(&self) -> Result<ImageService, AppError> {
        let stored_api_key = self
            .credential_store
            .read_api_key()?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError {
                code: "missing_api_key".to_string(),
                message: "API key not configured".to_string(),
                details: None,
            })?;

        let settings = self.read_writable_settings()?;
        Ok(ImageService::production(
            stored_api_key,
            settings.request_timeout_ms,
        ))
    }

    pub fn subtitle_service(&self) -> Result<SubtitleService, AppError> {
        let stored_api_key = self
            .credential_store
            .read_api_key()?
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError {
                code: "missing_api_key".to_string(),
                message: "API key not configured".to_string(),
                details: None,
            })?;

        let settings = self.read_writable_settings()?;
        Ok(SubtitleService::production(
            stored_api_key,
            settings.request_timeout_ms,
        ))
    }
}
