use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager};

use crate::error::AppError;
use crate::models::installer::{
    InstallStageId, InstallerComponentState, InstallerComponentStatus, InstallerLogEntry,
    InstallerSnapshot,
};
use crate::services::installer::environment::{build_initial_snapshot, DetectExecutionEnvironment};
use crate::services::installer::executor::{
    bundled_resource_path, claude_code_install_commands, codex_install_commands, command_display,
    microsoft_store_product_page_command, microsoft_store_product_uri,
    microsoft_store_service_repair_commands, stage_sequence, third_party_install_command,
    windows_system_program, PlannedCommand,
};
use crate::services::installer::manifest::{verify_sha256, InstallerManifest};

static INSTALLER_SESSION_STATE: OnceLock<Mutex<InstallerSessionState>> = OnceLock::new();
static INSTALLER_FLOW_RUNNING: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Default)]
pub struct InstallerSessionState {
    pub snapshot: Option<InstallerSnapshot>,
    pub last_flow: Option<String>,
    pub failed_stage: Option<InstallStageId>,
}

impl InstallerSessionState {
    pub fn record_failure(&mut self, flow: &str, stage: InstallStageId) {
        self.last_flow = Some(flow.to_string());
        self.failed_stage = Some(stage);
    }
}

pub struct InstallerService;

#[derive(Debug)]
pub struct InstallerFlowGuard;

impl Drop for InstallerFlowGuard {
    fn drop(&mut self) {
        INSTALLER_FLOW_RUNNING.store(false, Ordering::Release);
    }
}

impl InstallerService {
    pub fn production() -> Self {
        Self
    }

    pub fn validate_flow(&self, flow: &str) -> Result<(), AppError> {
        validate_flow(flow)
    }

    pub fn load_snapshot(&self) -> Result<InstallerSnapshot, AppError> {
        let guard = session_state().lock().map_err(|_| AppError {
            code: "installer_state_poisoned".into(),
            message: "Installer state lock failed".into(),
            details: None,
        })?;

        if let Some(snapshot) = guard.snapshot.clone() {
            Ok(snapshot)
        } else {
            Ok(build_execution_snapshot())
        }
    }

    pub fn refresh_snapshot(&self) -> Result<InstallerSnapshot, AppError> {
        if INSTALLER_FLOW_RUNNING.load(Ordering::Acquire) {
            return self.load_snapshot();
        }

        let snapshot = build_execution_snapshot();
        persist_latest_snapshot(snapshot.clone())?;
        Ok(snapshot)
    }

    pub fn reserve_flow(&self) -> Result<InstallerFlowGuard, AppError> {
        INSTALLER_FLOW_RUNNING
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| InstallerFlowGuard)
            .map_err(|_| AppError {
                code: "installer_flow_already_running".into(),
                message: "An installer flow is already running".into(),
                details: None,
            })
    }

    pub fn snapshot_updates_for(&self, flow: &str) -> Result<Vec<InstallerSnapshot>, AppError> {
        let mut snapshot = build_execution_snapshot();
        let stages = stage_sequence(flow);
        validate_stages(flow, &stages)?;
        let stage_count = stages.len();
        let mut snapshots = Vec::with_capacity(stage_count + 1);

        for (index, stage) in stages.iter().enumerate() {
            snapshot.current_stage = stage.clone();
            snapshot.progress_percent = (((index + 1) * 100) / (stage_count + 1)) as u8;
            snapshot.last_error = None;
            snapshot.logs.push(build_log_entry(
                stage.clone(),
                "info",
                format!("进入阶段 {}", format_stage(stage)),
            ));
            snapshots.push(snapshot.clone());
        }

        snapshot.current_stage = InstallStageId::Completed;
        snapshot.progress_percent = 100;
        snapshot.last_error = None;
        snapshot.logs.push(build_log_entry(
            InstallStageId::Completed,
            "info",
            "安装流程完成".into(),
        ));
        snapshots.push(snapshot);

        Ok(snapshots)
    }

    pub fn retry_snapshots_for_stage(
        &self,
        failed_stage: InstallStageId,
    ) -> Result<Vec<InstallerSnapshot>, AppError> {
        let mut snapshot = build_execution_snapshot();
        let flow = match failed_stage {
            InstallStageId::InstallCodex => "install_codex",
            InstallStageId::InstallClaudeCode => "install_claude_code",
            _ => "install_all",
        };
        let stages = stage_sequence(flow);
        let retry_start = stages
            .iter()
            .position(|stage| *stage == failed_stage)
            .unwrap_or(0);
        let retry_stages = &stages[retry_start..];
        let stage_count = retry_stages.len();
        let mut snapshots = Vec::with_capacity(stage_count + 1);

        for (index, stage) in retry_stages.iter().enumerate() {
            snapshot.current_stage = stage.clone();
            snapshot.progress_percent = (((index + 1) * 100) / (stage_count + 1)) as u8;
            snapshot.last_error = None;
            snapshot.logs.push(build_log_entry(
                stage.clone(),
                "info",
                format!("重试阶段 {}", format_stage(stage)),
            ));
            snapshots.push(snapshot.clone());
        }

        snapshot.current_stage = InstallStageId::Completed;
        snapshot.progress_percent = 100;
        snapshot.last_error = None;
        snapshot.logs.push(build_log_entry(
            InstallStageId::Completed,
            "info",
            "重试流程完成".into(),
        ));
        snapshots.push(snapshot);

        Ok(snapshots)
    }

    pub async fn run_flow(&self, app: &AppHandle, flow: &str) -> Result<(), AppError> {
        validate_flow(flow)?;
        let guard = self.reserve_flow()?;
        self.run_reserved_flow(app, flow, guard).await
    }

    pub async fn run_reserved_flow(
        &self,
        app: &AppHandle,
        flow: &str,
        _flow_guard: InstallerFlowGuard,
    ) -> Result<(), AppError> {
        validate_flow(flow)?;
        let mut snapshot = build_execution_snapshot();
        snapshot.logs.clear();

        {
            let mut guard = lock_session_state()?;
            guard.last_flow = Some(flow.to_string());
            guard.failed_stage = None;
            guard.snapshot = Some(snapshot.clone());
        }

        emit_snapshot(app, &snapshot)?;

        let stages = stage_sequence(flow);
        validate_stages(flow, &stages)?;
        let stage_count = stages.len();
        for (index, stage) in stages.iter().enumerate() {
            snapshot.current_stage = stage.clone();
            snapshot.progress_percent = (((index + 1) * 100) / (stage_count + 1)) as u8;
            snapshot.last_error = None;
            snapshot.logs.push(build_log_entry(
                stage.clone(),
                "info",
                format!("进入阶段 {}", format_stage(stage)),
            ));
            persist_latest_snapshot(snapshot.clone())?;
            emit_snapshot(app, &snapshot)?;

            match self
                .execute_stage(app, &mut snapshot, flow, stage.clone())
                .await
            {
                Ok(()) => {
                    persist_latest_snapshot(snapshot.clone())?;
                    emit_snapshot(app, &snapshot)?;
                }
                Err(error) => {
                    snapshot.current_stage = InstallStageId::Failed;
                    snapshot.last_error = Some(error.message.clone());
                    snapshot.logs.push(build_log_entry(
                        InstallStageId::Failed,
                        "error",
                        format!("阶段 {} 失败：{}", format_stage(stage), error.message),
                    ));

                    {
                        let mut guard = lock_session_state()?;
                        guard.record_failure(flow, stage.clone());
                        guard.snapshot = Some(snapshot.clone());
                    }

                    emit_snapshot(app, &snapshot)?;
                    return Err(error);
                }
            }
        }

        snapshot.current_stage = InstallStageId::Completed;
        snapshot.progress_percent = 100;
        snapshot.last_error = None;
        snapshot.logs.push(build_log_entry(
            InstallStageId::Completed,
            "info",
            "安装流程完成".into(),
        ));

        {
            let mut guard = lock_session_state()?;
            guard.failed_stage = None;
            guard.snapshot = Some(snapshot.clone());
        }
        emit_snapshot(app, &snapshot)?;
        Ok(())
    }

    pub async fn retry_current_stage(&self, app: &AppHandle) -> Result<(), AppError> {
        let guard = self.reserve_flow()?;
        self.retry_current_stage_reserved(app, guard).await
    }

    pub async fn retry_current_stage_reserved(
        &self,
        app: &AppHandle,
        guard: InstallerFlowGuard,
    ) -> Result<(), AppError> {
        let (flow, failed_stage) = {
            let guard = lock_session_state()?;
            let flow = guard.last_flow.clone().ok_or_else(|| AppError {
                code: "installer_retry_context_missing".into(),
                message: "No failed installer flow is available to retry".into(),
                details: None,
            })?;
            let stage = guard.failed_stage.clone().ok_or_else(|| AppError {
                code: "installer_retry_context_missing".into(),
                message: "No failed installer stage is available to retry".into(),
                details: None,
            })?;
            (flow, stage)
        };

        let flow_stages = stage_sequence(&flow);
        if !flow_stages.contains(&failed_stage) {
            return Err(AppError {
                code: "installer_retry_stage_mismatch".into(),
                message: "The failed stage does not belong to the last flow".into(),
                details: Some(format!(
                    "flow={flow}, failed_stage={failed_stage:?}, stages={flow_stages:?}"
                )),
            });
        }

        let retry_flow = match failed_stage {
            InstallStageId::InstallCodex => "install_codex",
            InstallStageId::InstallClaudeCode => match flow.as_str() {
                "install_claude_code" => "install_claude_code",
                _ => "install_all",
            },
            _ => flow.as_str(),
        };

        self.run_reserved_flow(app, retry_flow, guard).await
    }

    pub fn stage_sequence_for(&self, flow: &str) -> Result<Vec<String>, AppError> {
        let stages = stage_sequence(flow);
        validate_stages(flow, &stages)?;
        Ok(stages.into_iter().map(|stage| format!("{stage:?}")).collect())
    }

    async fn execute_stage(
        &self,
        app: &AppHandle,
        snapshot: &mut InstallerSnapshot,
        flow: &str,
        stage: InstallStageId,
    ) -> Result<(), AppError> {
        match stage {
            InstallStageId::Preflight => self.execute_preflight(snapshot),
            InstallStageId::InstallGit => {
                self.execute_third_party_install(app, snapshot, "git", InstallStageId::InstallGit)
            }
            InstallStageId::InstallPython => self.execute_third_party_install(
                app,
                snapshot,
                "python",
                InstallStageId::InstallPython,
            ),
            InstallStageId::InstallNode => self.execute_third_party_install(
                app,
                snapshot,
                "nodejs",
                InstallStageId::InstallNode,
            ),
            InstallStageId::InstallCcSwitch => self.execute_third_party_install(
                app,
                snapshot,
                "cc_switch",
                InstallStageId::InstallCcSwitch,
            ),
            InstallStageId::RefreshEnvironment => self.execute_refresh_environment(snapshot),
            InstallStageId::InstallCodex => self.execute_codex_install(app, snapshot),
            InstallStageId::InstallClaudeCode => {
                self.execute_claude_code_install(app, snapshot, flow)
            }
            InstallStageId::Verify => self.execute_verify(snapshot, flow),
            InstallStageId::Idle | InstallStageId::Completed | InstallStageId::Failed => Ok(()),
        }
        .map_err(|error| {
            let _ = app;
            error
        })
    }

    fn execute_preflight(&self, snapshot: &mut InstallerSnapshot) -> Result<(), AppError> {
        if snapshot.last_error.is_some() {
            snapshot.logs.push(build_log_entry(
                InstallStageId::Preflight,
                "error",
                "当前未以管理员身份运行，请关闭后用管理员身份重新打开".into(),
            ));
            return Err(AppError {
                code: "installer_requires_admin".into(),
                message: "Administrator privileges are required".into(),
                details: snapshot.last_error.clone(),
            });
        }
        Ok(())
    }

    fn execute_third_party_install(
        &self,
        app: &AppHandle,
        snapshot: &mut InstallerSnapshot,
        component_id: &str,
        stage: InstallStageId,
    ) -> Result<(), AppError> {
        if mark_component_skipped_if_installed(snapshot, component_id, stage.clone()) {
            persist_latest_snapshot(snapshot.clone())?;
            emit_snapshot(app, snapshot)?;
            return Ok(());
        }

        let manifest = self.load_bundled_manifest(app)?;
        let resource = manifest.resource(component_id).ok_or_else(|| AppError {
            code: "installer_resource_missing".into(),
            message: format!("Bundled resource not found for {component_id}"),
            details: None,
        })?;
        let resource_root = self.resource_root(app)?;
        let full_path = bundled_resource_path(&resource.file_name, &resource_root)?;

        if !verify_sha256(&full_path, &resource.sha256)? {
            return Err(AppError {
                code: "installer_resource_checksum_failed".into(),
                message: format!("Checksum verification failed for {}", resource.file_name),
                details: Some(full_path.display().to_string()),
            });
        }

        set_component_status(
            &mut snapshot.components,
            component_id,
            InstallerComponentStatus::Installing,
            format!("正在静默安装 {}，请勿关闭窗口", resource.version),
            None,
        );
        snapshot.logs.push(build_log_entry(
            stage.clone(),
            "info",
            format!("校验通过，准备安装 {}", resource.file_name),
        ));
        persist_latest_snapshot(snapshot.clone())?;
        emit_snapshot(app, snapshot)?;

        let command = third_party_install_command(
            component_id,
            &resource.file_name,
            &resource.install_command,
            &resource_root,
        )?;
        match self.run_command(&command, stage.clone()) {
            Ok(()) => {
                let refreshed = build_execution_snapshot();
                if let Some(updated) = refreshed
                    .components
                    .iter()
                    .find(|item| item.id == component_id)
                {
                    set_component_status(
                        &mut snapshot.components,
                        component_id,
                        updated.status.clone(),
                        updated.detail.clone(),
                        updated.version.clone(),
                    );
                }
                Ok(())
            }
            Err(error) => {
                set_component_status(
                    &mut snapshot.components,
                    component_id,
                    InstallerComponentStatus::Failed,
                    format!("{} 安装失败", resource.file_name),
                    None,
                );
                snapshot.logs.push(build_log_entry(
                    stage.clone(),
                    "error",
                    format!("{} 安装失败：{}", component_id, error.message),
                ));
                persist_latest_snapshot(snapshot.clone())?;
                emit_snapshot(app, snapshot)?;
                Err(error)
            }
        }
    }

    fn execute_refresh_environment(
        &self,
        snapshot: &mut InstallerSnapshot,
    ) -> Result<(), AppError> {
        let refreshed = build_execution_snapshot();
        for updated in refreshed.components {
            if snapshot.current_stage == InstallStageId::RefreshEnvironment
                && component_status(&snapshot.components, &updated.id)
                    == Some(InstallerComponentStatus::Installing)
            {
                continue;
            }

            set_component_status(
                &mut snapshot.components,
                &updated.id,
                updated.status,
                updated.detail,
                updated.version,
            );
        }
        snapshot.logs.push(build_log_entry(
            InstallStageId::RefreshEnvironment,
            "info",
            "已刷新环境探测结果".into(),
        ));
        Ok(())
    }

    fn execute_codex_install(
        &self,
        app: &AppHandle,
        snapshot: &mut InstallerSnapshot,
    ) -> Result<(), AppError> {
        if mark_component_skipped_if_installed(snapshot, "codex", InstallStageId::InstallCodex) {
            persist_latest_snapshot(snapshot.clone())?;
            emit_snapshot(app, snapshot)?;
            return Ok(());
        }

        set_component_status(
            &mut snapshot.components,
            "codex",
            InstallerComponentStatus::Installing,
            "正在通过 Microsoft Store / winget 安装，请稍候".into(),
            None,
        );
        snapshot.logs.push(build_log_entry(
            InstallStageId::InstallCodex,
            "info",
            "已启动 Codex Microsoft Store 安装命令".into(),
        ));
        persist_latest_snapshot(snapshot.clone())?;
        emit_snapshot(app, snapshot)?;

        self.repair_microsoft_store_services(snapshot)?;

        let codex_commands = codex_install_commands()?;
        if let Err(store_error) = self.run_commands(&codex_commands, InstallStageId::InstallCodex) {
            let _ = self.open_microsoft_store_product_page(snapshot);
            set_component_status(
                &mut snapshot.components,
                "codex",
                InstallerComponentStatus::Failed,
                "Codex 安装失败".into(),
                None,
            );
            let wrapped_error = codex_store_install_failure_error(store_error);
            snapshot.logs.push(build_log_entry(
                InstallStageId::InstallCodex,
                "error",
                format!("Codex 安装失败：{}", wrapped_error.message),
            ));
            persist_latest_snapshot(snapshot.clone())?;
            emit_snapshot(app, snapshot)?;
            return Err(wrapped_error);
        }

        set_component_status(
            &mut snapshot.components,
            "codex",
            InstallerComponentStatus::Installing,
            "Codex 安装命令已完成，等待最终校验".into(),
            None,
        );
        Ok(())
    }

    fn repair_microsoft_store_services(
        &self,
        snapshot: &mut InstallerSnapshot,
    ) -> Result<(), AppError> {
        let commands = microsoft_store_service_repair_commands()?;
        snapshot.logs.push(build_log_entry(
            InstallStageId::InstallCodex,
            "info",
            "正在检查并启动 Microsoft Store 相关服务".into(),
        ));

        for command in commands {
            if command.args.first().map(|s| s.as_str()) == Some("config") {
                if let Some(service) = command.args.get(1) {
                    if let Some(original) = query_service_start_type(service) {
                        snapshot.logs.push(build_log_entry(
                            InstallStageId::InstallCodex,
                            "info",
                            format!("{} {}", service, original),
                        ));
                    }
                }
            }

            if let Err(error) = self.run_command_with_timeout(&command, InstallStageId::InstallCodex, Duration::from_secs(30)) {
                snapshot.logs.push(build_log_entry(
                    InstallStageId::InstallCodex,
                    "warn",
                    format!(
                        "Microsoft Store 服务修复命令未完成：{}{}",
                        command_display(&command),
                        format_error_details(&error)
                    ),
                ));
            }
        }

        Ok(())
    }

    fn open_microsoft_store_product_page(
        &self,
        snapshot: &mut InstallerSnapshot,
    ) -> Result<(), AppError> {
        let command = microsoft_store_product_page_command()?;
        snapshot.logs.push(build_log_entry(
            InstallStageId::InstallCodex,
            "warn",
            format!("winget 安装失败，正在尝试打开 Microsoft Store 页面：{}", microsoft_store_product_uri()),
        ));
        self.run_command(&command, InstallStageId::InstallCodex)
    }

    fn execute_claude_code_install(
        &self,
        app: &AppHandle,
        snapshot: &mut InstallerSnapshot,
        flow: &str,
    ) -> Result<(), AppError> {
        if !matches!(flow, "install_all" | "install_claude_code") {
            return Ok(());
        }

        if mark_component_skipped_if_installed(
            snapshot,
            "claude_code",
            InstallStageId::InstallClaudeCode,
        ) {
            persist_latest_snapshot(snapshot.clone())?;
            emit_snapshot(app, snapshot)?;
            return Ok(());
        }

        set_component_status(
            &mut snapshot.components,
            "claude_code",
            InstallerComponentStatus::Installing,
            "正在通过 npm 安装 Claude Code，请稍候".into(),
            None,
        );
        snapshot.logs.push(build_log_entry(
            InstallStageId::InstallClaudeCode,
            "info",
            "已启动 Claude Code 安装命令".into(),
        ));
        persist_latest_snapshot(snapshot.clone())?;
        emit_snapshot(app, snapshot)?;

        let claude_code_commands = claude_code_install_commands()?;
        match self.run_commands(&claude_code_commands, InstallStageId::InstallClaudeCode) {
            Ok(()) => {
                set_component_status(
                    &mut snapshot.components,
                    "claude_code",
                    InstallerComponentStatus::Installing,
                    "Claude Code 安装命令已完成，等待最终校验".into(),
                    None,
                );
            }
            Err(error) => {
                set_component_status(
                    &mut snapshot.components,
                    "claude_code",
                    InstallerComponentStatus::Failed,
                    "Claude Code 安装失败".into(),
                    None,
                );
                snapshot.logs.push(build_log_entry(
                    InstallStageId::InstallClaudeCode,
                    "error",
                    format!("Claude Code 安装失败：{}", error.message),
                ));
                if let Some(details) = error.details.as_ref().filter(|value| !value.trim().is_empty()) {
                    snapshot.logs.push(build_log_entry(
                        InstallStageId::InstallClaudeCode,
                        "error",
                        details.clone(),
                    ));
                }
                persist_latest_snapshot(snapshot.clone())?;
                emit_snapshot(app, snapshot)?;
                return Err(error);
            }
        }

        Ok(())
    }

    fn execute_verify(&self, snapshot: &mut InstallerSnapshot, flow: &str) -> Result<(), AppError> {
        let refreshed = build_execution_snapshot();
        for updated in refreshed.components {
            set_component_status(
                &mut snapshot.components,
                &updated.id,
                updated.status,
                updated.detail,
                updated.version,
            );
        }

        let required_components: &[&str] = match flow {
            "install_codex" => &["git", "python", "nodejs", "cc_switch", "codex"],
            "install_claude_code" => &["git", "python", "nodejs", "cc_switch", "claude_code"],
            "install_all" => &["git", "python", "nodejs", "cc_switch", "codex", "claude_code"],
            _ => &["git", "python", "nodejs", "cc_switch", "codex"],
        };

        let failed: Vec<String> = snapshot
            .components
            .iter()
            .filter(|component| required_components.contains(&component.id.as_str()))
            .filter(|component| {
                !matches!(
                    component.status,
                    InstallerComponentStatus::Installed | InstallerComponentStatus::Skipped
                )
            })
            .map(|component| format!("{}={:?}", component.id, component.status))
            .collect();

        if failed.is_empty() {
            snapshot.logs.push(build_log_entry(
                InstallStageId::Verify,
                "info",
                "所有目标组件校验通过".into(),
            ));
            Ok(())
        } else {
            Err(AppError {
                code: "installer_verify_failed".into(),
                message: "One or more components failed verification".into(),
                details: Some(failed.join(", ")),
            })
        }
    }

    fn load_bundled_manifest(&self, app: &AppHandle) -> Result<InstallerManifest, AppError> {
        let root = self.resource_root(app)?;
        let manifest_path = root.join("manifest.json");
        let json = std::fs::read_to_string(&manifest_path).map_err(|error| AppError {
            code: "installer_manifest_read_failed".into(),
            message: "Failed to read bundled installer manifest".into(),
            details: Some(format!("{} ({error})", manifest_path.display())),
        })?;
        InstallerManifest::from_json_str(&json)
    }

    fn resource_root(&self, app: &AppHandle) -> Result<PathBuf, AppError> {
        app.path()
            .resolve("resources/third_party", BaseDirectory::Resource)
            .map_err(|error| AppError {
                code: "installer_resource_root_missing".into(),
                message: "Installer bundled resources directory not found".into(),
                details: Some(error.to_string()),
            })
    }

    fn run_commands(
        &self,
        commands: &[PlannedCommand],
        stage: InstallStageId,
    ) -> Result<(), AppError> {
        for command in commands {
            self.run_command(command, stage.clone())?;
        }

        Ok(())
    }

    fn run_command(&self, command: &PlannedCommand, stage: InstallStageId) -> Result<(), AppError> {
        self.run_command_with_timeout(command, stage, Duration::from_secs(600))
    }

    fn run_command_with_timeout(
        &self,
        command: &PlannedCommand,
        stage: InstallStageId,
        timeout: Duration,
    ) -> Result<(), AppError> {
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        #[cfg(target_os = "windows")]
        {
            process.creation_flags(CREATE_NO_WINDOW);
        }

        let mut child = process.spawn().map_err(|error| AppError {
            code: "installer_command_spawn_failed".into(),
            message: format!("Failed to start {}", command.program),
            details: Some(error.to_string()),
        })?;

        let deadline = Instant::now() + timeout;
        let mut timed_out = false;
        let mut cleanup_failure_detail = None;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        cleanup_failure_detail = terminate_child_process_tree(&mut child);
                        timed_out = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => {
                    cleanup_failure_detail = terminate_child_process_tree(&mut child);
                    break;
                }
            }
        }

        let output = child.wait_with_output().map_err(|error| AppError {
            code: "installer_command_output_failed".into(),
            message: format!("Failed to collect output from {}", command.program),
            details: Some(error.to_string()),
        })?;

        if output.status.success() && !timed_out {
            Ok(())
        } else if timed_out {
            Err(AppError {
                code: format!("installer_command_timeout_{}", format_stage(&stage)),
                message: format!(
                    "Command timed out after {}s: {}",
                    timeout.as_secs(),
                    command_display(command)
                ),
                details: cleanup_failure_detail,
            })
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let combined = if stderr.is_empty() {
                stdout
            } else if stdout.is_empty() {
                stderr
            } else {
                format!("{stderr}\n{stdout}")
            };
            Err(AppError {
                code: format!("installer_command_failed_{}", format_stage(&stage)),
                message: format!("Command failed: {}", command_display(command)),
                details: Some(combined),
            })
        }
    }

    fn emit_error(error: impl ToString) -> AppError {
        AppError {
            code: "installer_emit_failed".into(),
            message: "Failed to emit installer snapshot".into(),
            details: Some(error.to_string()),
        }
    }
}

fn session_state() -> &'static Mutex<InstallerSessionState> {
    INSTALLER_SESSION_STATE.get_or_init(|| Mutex::new(InstallerSessionState::default()))
}

fn build_execution_snapshot() -> InstallerSnapshot {
    let probe = DetectExecutionEnvironment::new();
    build_initial_snapshot(&probe)
}

fn validate_flow(flow: &str) -> Result<(), AppError> {
    validate_stages(flow, &stage_sequence(flow))
}

fn validate_stages(flow: &str, stages: &[InstallStageId]) -> Result<(), AppError> {
    if stages.is_empty() {
        Err(AppError {
            code: "invalid_installer_flow".into(),
            message: "Unknown installer flow".into(),
            details: Some(flow.to_string()),
        })
    } else {
        Ok(())
    }
}

fn lock_session_state() -> Result<std::sync::MutexGuard<'static, InstallerSessionState>, AppError> {
    session_state().lock().map_err(|_| AppError {
        code: "installer_state_poisoned".into(),
        message: "Installer state lock failed".into(),
        details: None,
    })
}

fn emit_snapshot(app: &AppHandle, snapshot: &InstallerSnapshot) -> Result<(), AppError> {
    app.emit("installer://snapshot", snapshot.clone())
        .map_err(InstallerService::emit_error)
}

fn terminate_child_process_tree(child: &mut std::process::Child) -> Option<String> {
    let mut cleanup_failure_detail = None;

    #[cfg(target_os = "windows")]
    match timeout_process_tree_kill_command(child.id()) {
        Ok(command) => {
            let mut process = Command::new(&command.program);
            process.args(&command.args);
            process.creation_flags(CREATE_NO_WINDOW);
            match process.status() {
                Ok(status) if status.success() => {}
                Ok(status) => {
                    cleanup_failure_detail =
                        Some(timeout_cleanup_failure_detail(&command, &status.to_string()));
                }
                Err(error) => {
                    cleanup_failure_detail =
                        Some(timeout_cleanup_failure_detail(&command, &error.to_string()));
                }
            }
        }
        Err(error) => {
            cleanup_failure_detail = Some(format!(
                "Timeout cleanup command could not be created: {}{}",
                error.message,
                format_error_details(&error)
            ));
        }
    }

    let _ = child.kill();
    cleanup_failure_detail
}

#[cfg(target_os = "windows")]
pub(super) fn timeout_process_tree_kill_command(pid: u32) -> Result<PlannedCommand, AppError> {
    Ok(PlannedCommand {
        program: windows_system_program("taskkill.exe")?,
        args: vec!["/PID".into(), pid.to_string(), "/T".into(), "/F".into()],
    })
}

pub(super) fn timeout_cleanup_failure_detail(command: &PlannedCommand, reason: &str) -> String {
    format!(
        "Timeout cleanup command failed: {} ({reason})",
        command_display(command)
    )
}

#[cfg(not(target_os = "windows"))]
pub(super) fn timeout_process_tree_kill_command(pid: u32) -> Result<PlannedCommand, AppError> {
    Ok(PlannedCommand {
        program: "kill".into(),
        args: vec!["-TERM".into(), pid.to_string()],
    })
}

fn persist_latest_snapshot(snapshot: InstallerSnapshot) -> Result<(), AppError> {
    let mut guard = lock_session_state()?;
    guard.snapshot = Some(snapshot);
    Ok(())
}

fn build_log_entry(stage: InstallStageId, level: &str, message: String) -> InstallerLogEntry {
    let timestamp = {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs() % 86400;
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    };
    InstallerLogEntry {
        timestamp,
        stage,
        level: level.into(),
        message,
    }
}

fn set_component_status(
    components: &mut [InstallerComponentState],
    component_id: &str,
    status: InstallerComponentStatus,
    detail: String,
    version: Option<String>,
) {
    if let Some(component) = components.iter_mut().find(|item| item.id == component_id) {
        component.status = status;
        component.detail = detail;
        component.version = version;
    }
}

pub(super) fn component_status(
    components: &[InstallerComponentState],
    component_id: &str,
) -> Option<InstallerComponentStatus> {
    components
        .iter()
        .find(|item| item.id == component_id)
        .map(|component| component.status.clone())
}

pub(super) fn mark_component_skipped_if_installed(
    snapshot: &mut InstallerSnapshot,
    component_id: &str,
    stage: InstallStageId,
) -> bool {
    let Some(component) = snapshot
        .components
        .iter_mut()
        .find(|item| item.id == component_id)
    else {
        return false;
    };

    if component.status != InstallerComponentStatus::Installed {
        return false;
    }

    let original_detail = component.detail.clone();
    let original_version = component.version.clone();
    component.status = InstallerComponentStatus::Skipped;
    component.detail = format!("已检测到安装，跳过本阶段：{original_detail}");
    component.version = original_version;

    snapshot.logs.push(build_log_entry(
        stage,
        "info",
        format!("{} 已安装，跳过安装", component.label),
    ));

    true
}

#[cfg(target_os = "windows")]
fn query_service_start_type(service: &str) -> Option<String> {
    let output = Command::new(windows_system_program("sc.exe").ok()?)
        .args(["qc", service])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.contains("START_TYPE") {
            return Some(line.trim().to_string());
        }
    }
    None
}

fn format_error_details(error: &AppError) -> String {
    error
        .details
        .as_ref()
        .filter(|details| !details.trim().is_empty())
        .map(|details| format!(" ({details})"))
        .unwrap_or_default()
}

pub(super) fn codex_store_install_failure_error(store_error: AppError) -> AppError {
    AppError {
        code: "installer_codex_store_install_failed".into(),
        message: "Codex desktop installation from Microsoft Store failed".into(),
        details: Some(format!(
            "{}{}. AI Dev Installer has tried to start Microsoft Store services AppXSvc, ClipSVC, and InstallService, then open {}. Please repair Microsoft Store, Microsoft App Installer, or winget if the page did not open.",
            store_error.message,
            format_error_details(&store_error),
            microsoft_store_product_uri()
        )),
    }
}

fn format_stage(stage: &InstallStageId) -> &'static str {
    match stage {
        InstallStageId::Idle => "idle",
        InstallStageId::Preflight => "preflight",
        InstallStageId::InstallGit => "install_git",
        InstallStageId::InstallPython => "install_python",
        InstallStageId::InstallNode => "install_node",
        InstallStageId::InstallCcSwitch => "install_cc_switch",
        InstallStageId::RefreshEnvironment => "refresh_environment",
        InstallStageId::InstallCodex => "install_codex",
        InstallStageId::InstallClaudeCode => "install_claude_code",
        InstallStageId::Verify => "verify",
        InstallStageId::Completed => "completed",
        InstallStageId::Failed => "failed",
    }
}
