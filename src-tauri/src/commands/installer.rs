use crate::error::AppError;
use crate::models::installer::InstallerSnapshot;
use crate::services::installer::service::InstallerService;
use tauri::AppHandle;

#[tauri::command]
pub async fn load_installer_snapshot() -> Result<InstallerSnapshot, AppError> {
    tauri::async_runtime::spawn_blocking(|| InstallerService::production().load_snapshot())
        .await
        .map_err(|error| AppError {
            code: "installer_task_failed".into(),
            message: "Installer task failed".into(),
            details: Some(error.to_string()),
        })?
}

#[tauri::command]
pub async fn refresh_installer_snapshot() -> Result<InstallerSnapshot, AppError> {
    tauri::async_runtime::spawn_blocking(|| InstallerService::production().refresh_snapshot())
        .await
        .map_err(|error| AppError {
            code: "installer_task_failed".into(),
            message: "Installer task failed".into(),
            details: Some(error.to_string()),
        })?
}

#[tauri::command]
pub async fn start_install_flow(app: AppHandle, flow: String) -> Result<(), AppError> {
    let service = InstallerService::production();
    service.validate_flow(&flow)?;
    let guard = service.reserve_flow()?;

    tauri::async_runtime::spawn(async move {
        let _ = service.run_reserved_flow(&app, &flow, guard).await;
    });

    Ok(())
}

#[tauri::command]
pub async fn retry_current_install_stage(app: AppHandle) -> Result<(), AppError> {
    let service = InstallerService::production();
    let guard = service.reserve_flow()?;

    tauri::async_runtime::spawn(async move {
        let _ = service.retry_current_stage_reserved(&app, guard).await;
    });

    Ok(())
}

#[tauri::command]
pub async fn retry_install_all(app: AppHandle) -> Result<(), AppError> {
    let service = InstallerService::production();
    let guard = service.reserve_flow()?;

    tauri::async_runtime::spawn(async move {
        let _ = service.run_reserved_flow(&app, "install_all", guard).await;
    });

    Ok(())
}
