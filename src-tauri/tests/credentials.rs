use std::path::PathBuf;

use ai_dev_installer::models::ApiKeyStatus;
use ai_dev_installer::services::credentials::{CredentialStore, MemorySecretBackend};
use ai_dev_installer::services::settings::{GeminiConnectivityClient, SettingsService};

use ai_dev_installer::error::AppError;
use ai_dev_installer::models::{GeminiModelOption, WritableAppSettings};

#[test]
fn saves_reads_and_clears_api_key_status_without_leaking_secret() {
    let backend = MemorySecretBackend::default();
    let store = CredentialStore::new(Box::new(backend));

    assert_eq!(store.api_key_status().unwrap(), ApiKeyStatus::Missing);

    store.save_api_key("abc123").unwrap();
    assert_eq!(store.api_key_status().unwrap(), ApiKeyStatus::Configured);

    store.clear_api_key().unwrap();
    assert_eq!(store.api_key_status().unwrap(), ApiKeyStatus::Missing);
}

#[test]
fn settings_service_reports_only_status_from_memory_backend() {
    let backend = MemorySecretBackend::default();
    let service = SettingsService::new(
        CredentialStore::new(Box::new(backend)),
        PathBuf::from("settings-test.json"),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    assert_eq!(service.load().unwrap().api_key_status, ApiKeyStatus::Missing);

    let settings = service.save_api_key("abc123").unwrap();
    assert_eq!(settings.api_key_status, ApiKeyStatus::Configured);

    let cleared = service.clear_api_key().unwrap();
    assert_eq!(cleared.api_key_status, ApiKeyStatus::Missing);
}

#[derive(Clone)]
struct FakeGeminiConnectivityClient {
    models: Vec<GeminiModelOption>,
    should_fail: bool,
}

impl FakeGeminiConnectivityClient {
    fn success() -> Self {
        Self {
            models: vec![GeminiModelOption {
                model_id: "gemini-2.0-flash".to_string(),
                display_name: "Gemini 2.0 Flash".to_string(),
                supported_generation_methods: vec!["generateContent".to_string()],
                supports_chat: true,
                supports_image: false,
            }],
            should_fail: false,
        }
    }

    fn failure() -> Self {
        Self {
            models: vec![],
            should_fail: true,
        }
    }
}

impl GeminiConnectivityClient for FakeGeminiConnectivityClient {
    fn list_models(
        &self,
        _api_key: &str,
        _timeout_ms: u64,
    ) -> Result<Vec<GeminiModelOption>, AppError> {
        if self.should_fail {
            return Err(AppError {
                code: "connection_test_failed".to_string(),
                message: "Failed to reach Gemini Developer API".to_string(),
                details: None,
            });
        }

        Ok(self.models.clone())
    }
}

#[test]
fn saves_and_loads_non_sensitive_app_settings_separately_from_api_key() {
    let settings_path = std::env::temp_dir().join("ai-dev-installer-settings-test.json");
    let _ = std::fs::remove_file(&settings_path);

    let service = SettingsService::new(
        CredentialStore::new(Box::new(MemorySecretBackend::default())),
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let saved = service
        .save_app_settings(WritableAppSettings {
            default_chat_model: "gemini-2.5-flash".to_string(),
            default_image_model: "imagen-3".to_string(),
            default_export_dir: "C:/exports".to_string(),
            request_timeout_ms: 15_000,
        })
        .unwrap();

    assert_eq!(saved.api_key_status, ApiKeyStatus::Missing);
    assert_eq!(saved.default_chat_model, "gemini-2.5-flash");
    assert_eq!(saved.default_image_model, "imagen-3");
    assert_eq!(saved.default_export_dir, "C:/exports");
    assert_eq!(saved.request_timeout_ms, 15_000);

    let persisted = std::fs::read_to_string(&settings_path).unwrap();
    assert!(persisted.contains("defaultChatModel"));
    assert!(!persisted.contains("abc123"));

    let _ = std::fs::remove_file(&settings_path);
}

#[test]
fn rejects_high_risk_default_export_directory_settings() {
    let settings_path = std::env::temp_dir().join("ai-dev-installer-settings-invalid-export.json");
    let _ = std::fs::remove_file(&settings_path);

    let service = SettingsService::new(
        CredentialStore::new(Box::new(MemorySecretBackend::default())),
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let error = service
        .save_app_settings(WritableAppSettings {
            default_chat_model: "gemini-2.5-flash".to_string(),
            default_image_model: "imagen-3".to_string(),
            default_export_dir: "C:/Windows/System32".to_string(),
            request_timeout_ms: 15_000,
        })
        .expect_err("system export directory should be rejected");

    assert_eq!(error.code, "invalid_export_dir");
    assert!(!settings_path.exists());
}

#[test]
fn tests_api_key_connection_through_injected_client() {
    let settings_path = std::env::temp_dir().join("ai-dev-installer-connection-test.json");
    let _ = std::fs::remove_file(&settings_path);

    let backend = MemorySecretBackend::default();
    let store = CredentialStore::new(Box::new(backend));
    store.save_api_key("abc123").unwrap();

    let service = SettingsService::new(
        store,
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let result = service.test_api_key_connection().unwrap();
    assert!(result.ok);
    assert_eq!(result.message, "连接成功");

    let _ = std::fs::remove_file(&settings_path);
}

#[test]
fn returns_connectivity_client_failure_result() {
    let settings_path = std::env::temp_dir().join("ai-dev-installer-connection-failure-test.json");
    let _ = std::fs::remove_file(&settings_path);

    let backend = MemorySecretBackend::default();
    let store = CredentialStore::new(Box::new(backend));
    store.save_api_key("abc123").unwrap();

    let service = SettingsService::new(
        store,
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::failure()),
    );

    let error = service.test_api_key_connection().unwrap_err();
    assert_eq!(error.code, "connection_test_failed");

    let _ = std::fs::remove_file(&settings_path);
}

#[test]
fn lists_available_models_through_injected_client() {
    let settings_path = std::env::temp_dir().join("ai-dev-installer-model-list-test.json");
    let _ = std::fs::remove_file(&settings_path);

    let backend = MemorySecretBackend::default();
    let store = CredentialStore::new(Box::new(backend));
    store.save_api_key("abc123").unwrap();

    let service = SettingsService::new(
        store,
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let models = service.list_available_models().unwrap();
    assert_eq!(models.len(), 1);
    assert_eq!(models[0].model_id, "gemini-2.0-flash");

    let _ = std::fs::remove_file(&settings_path);
}

#[test]
fn recovers_settings_from_temp_file_when_primary_is_missing() {
    let base_dir = std::env::temp_dir().join("ai-dev-installer-settings-recover-from-new");
    let _ = std::fs::remove_dir_all(&base_dir);
    std::fs::create_dir_all(&base_dir).unwrap();

    let settings_path = base_dir.join("settings.json");
    let temp_path = base_dir.join("settings.json.new");
    std::fs::write(
        &temp_path,
        r#"{"defaultChatModel":"gemini-2.5-flash","defaultImageModel":"imagen-3","defaultExportDir":"D:/exports","requestTimeoutMs":15000}"#,
    )
    .unwrap();

    let service = SettingsService::new(
        CredentialStore::new(Box::new(MemorySecretBackend::default())),
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let loaded = service.load().unwrap();

    assert_eq!(loaded.default_chat_model, "gemini-2.5-flash");
    assert_eq!(loaded.default_export_dir, "D:/exports");
    assert!(settings_path.exists());

    let restored = std::fs::read_to_string(&settings_path).unwrap();
    assert!(restored.contains("gemini-2.5-flash"));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn recovers_settings_from_backup_when_primary_is_missing() {
    let base_dir = std::env::temp_dir().join("ai-dev-installer-settings-recover-from-bak");
    let _ = std::fs::remove_dir_all(&base_dir);
    std::fs::create_dir_all(&base_dir).unwrap();

    let settings_path = base_dir.join("settings.json");
    let backup_path = base_dir.join("settings.json.bak");
    std::fs::write(
        &backup_path,
        r#"{"defaultChatModel":"gemini-2.0-flash","defaultImageModel":"imagen-3","defaultExportDir":"C:/exports","requestTimeoutMs":30000}"#,
    )
    .unwrap();

    let service = SettingsService::new(
        CredentialStore::new(Box::new(MemorySecretBackend::default())),
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let loaded = service.load().unwrap();

    assert_eq!(loaded.default_chat_model, "gemini-2.0-flash");
    assert_eq!(loaded.default_export_dir, "C:/exports");
    assert!(settings_path.exists());

    let restored = std::fs::read_to_string(&settings_path).unwrap();
    assert!(restored.contains("gemini-2.0-flash"));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn prefers_temp_settings_over_backup_when_both_exist() {
    let base_dir = std::env::temp_dir().join("ai-dev-installer-settings-recover-prefer-new");
    let _ = std::fs::remove_dir_all(&base_dir);
    std::fs::create_dir_all(&base_dir).unwrap();

    let settings_path = base_dir.join("settings.json");
    let temp_path = base_dir.join("settings.json.new");
    let backup_path = base_dir.join("settings.json.bak");

    std::fs::write(
        &temp_path,
        r#"{"defaultChatModel":"gemini-2.5-flash","defaultImageModel":"imagen-3","defaultExportDir":"D:/exports","requestTimeoutMs":15000}"#,
    )
    .unwrap();
    std::fs::write(
        &backup_path,
        r#"{"defaultChatModel":"gemini-2.0-flash","defaultImageModel":"imagen-3","defaultExportDir":"C:/exports","requestTimeoutMs":30000}"#,
    )
    .unwrap();

    let service = SettingsService::new(
        CredentialStore::new(Box::new(MemorySecretBackend::default())),
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let loaded = service.load().unwrap();

    assert_eq!(loaded.default_chat_model, "gemini-2.5-flash");
    assert_eq!(loaded.default_export_dir, "D:/exports");
    assert!(settings_path.exists());

    let restored = std::fs::read_to_string(&settings_path).unwrap();
    assert!(restored.contains("gemini-2.5-flash"));

    let _ = std::fs::remove_dir_all(&base_dir);
}

#[test]
fn falls_back_to_backup_when_temp_settings_are_invalid() {
    let base_dir = std::env::temp_dir().join("ai-dev-installer-settings-recover-invalid-new");
    let _ = std::fs::remove_dir_all(&base_dir);
    std::fs::create_dir_all(&base_dir).unwrap();

    let settings_path = base_dir.join("settings.json");
    let temp_path = base_dir.join("settings.json.new");
    let backup_path = base_dir.join("settings.json.bak");

    std::fs::write(&temp_path, "{invalid-json").unwrap();
    std::fs::write(
        &backup_path,
        r#"{"defaultChatModel":"gemini-2.0-flash","defaultImageModel":"imagen-3","defaultExportDir":"C:/exports","requestTimeoutMs":30000}"#,
    )
    .unwrap();

    let service = SettingsService::new(
        CredentialStore::new(Box::new(MemorySecretBackend::default())),
        settings_path.clone(),
        Box::new(FakeGeminiConnectivityClient::success()),
    );

    let loaded = service.load().unwrap();

    assert_eq!(loaded.default_chat_model, "gemini-2.0-flash");
    assert_eq!(loaded.default_export_dir, "C:/exports");
    assert!(settings_path.exists());

    let _ = std::fs::remove_dir_all(&base_dir);
}
