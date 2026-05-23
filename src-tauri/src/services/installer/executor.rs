use std::path::{Component, Path, PathBuf};

use crate::error::AppError;
use crate::models::installer::{InstallStageId, InstallerLogEntry};

const CODEX_STORE_PRODUCT_ID: &str = "9PLM9XGG6VKS";

pub struct StageExecutionResult {
    pub next_stage: InstallStageId,
    pub logs: Vec<InstallerLogEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedCommand {
    pub program: String,
    pub args: Vec<String>,
}

pub trait CommandRunner {
    fn run(
        &self,
        program: &str,
        args: &[String],
        stage: InstallStageId,
    ) -> Result<Vec<InstallerLogEntry>, AppError>;
}

pub fn codex_install_commands() -> Vec<PlannedCommand> {
    vec![PlannedCommand {
        program: winget_program(),
        args: vec![
            "install".into(),
            "--id".into(),
            CODEX_STORE_PRODUCT_ID.into(),
            "--source".into(),
            "msstore".into(),
            "--accept-source-agreements".into(),
            "--accept-package-agreements".into(),
            "--silent".into(),
        ],
    }]
}

pub fn microsoft_store_service_repair_commands() -> Vec<PlannedCommand> {
    ["AppXSvc", "ClipSVC", "InstallService"]
        .into_iter()
        .flat_map(|service| {
            [
                PlannedCommand {
                    program: "sc.exe".into(),
                    args: vec![
                        "config".into(),
                        service.into(),
                        "start=".into(),
                        "demand".into(),
                    ],
                },
                PlannedCommand {
                    program: "sc.exe".into(),
                    args: vec!["start".into(), service.into()],
                },
            ]
        })
        .collect()
}

pub fn microsoft_store_product_uri() -> String {
    format!("ms-windows-store://pdp/?ProductId={CODEX_STORE_PRODUCT_ID}")
}

pub fn microsoft_store_product_page_command() -> PlannedCommand {
    PlannedCommand {
        program: "explorer.exe".into(),
        args: vec![microsoft_store_product_uri()],
    }
}

pub fn claude_code_install_commands() -> Vec<PlannedCommand> {
    vec![PlannedCommand {
        program: find_program_on_path("npm").unwrap_or_else(|| "npm".into()),
        args: vec![
            "install".into(),
            "-g".into(),
            "@anthropic-ai/claude-code".into(),
        ],
    }]
}

pub fn winget_program() -> String {
    find_program_on_path("winget")
        .or_else(|| {
            winget_candidate_paths()
                .into_iter()
                .find(|path| path.exists())
                .map(|path| path.display().to_string())
        })
        .unwrap_or_else(|| "winget".into())
}

pub fn winget_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("WindowsApps")
                .join("winget.exe"),
        );
    }

    if let Some(user_profile) = std::env::var_os("USERPROFILE") {
        candidates.push(
            PathBuf::from(user_profile)
                .join("AppData")
                .join("Local")
                .join("Microsoft")
                .join("WindowsApps")
                .join("winget.exe"),
        );
    }

    candidates
}

fn find_program_on_path(program: &str) -> Option<String> {
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for candidate_name in executable_names(program) {
                let candidate = dir.join(candidate_name);
                if candidate.exists() {
                    return Some(candidate.display().to_string());
                }
            }
        }
    }

    known_program_candidate_paths(program)
        .into_iter()
        .find(|path| path.exists())
        .map(|path| path.display().to_string())
}

fn executable_names(program: &str) -> Vec<String> {
    let lower = program.to_ascii_lowercase();
    if lower.ends_with(".exe") || lower.ends_with(".cmd") || lower.ends_with(".bat") {
        vec![program.to_string()]
    } else {
        vec![
            format!("{program}.exe"),
            format!("{program}.cmd"),
            format!("{program}.bat"),
            program.to_string(),
        ]
    }
}

fn known_program_candidate_paths(program: &str) -> Vec<PathBuf> {
    let normalized = program.to_ascii_lowercase();
    let mut candidates = Vec::new();

    match normalized.as_str() {
        "npm" | "npm.cmd" | "npm.exe" | "npm.bat" => {
            push_base_candidates(&mut candidates, &[r"nodejs\npm.cmd"]);
        }
        _ => {}
    }

    candidates
}

fn push_base_candidates(candidates: &mut Vec<PathBuf>, relative_paths: &[&str]) {
    for env_name in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(env_name) {
            for relative_path in relative_paths {
                candidates.push(PathBuf::from(&base).join(relative_path));
            }
        }
    }

    for base in [r"C:\Program Files", r"C:\Program Files (x86)"] {
        for relative_path in relative_paths {
            candidates.push(Path::new(base).join(relative_path));
        }
    }
}

pub fn third_party_install_command(
    _component_id: &str,
    file_name: &str,
    install_args: &[String],
    resource_root: &Path,
) -> Result<PlannedCommand, AppError> {
    let resource_path = bundled_resource_path(file_name, resource_root)?;
    let normalized = normalize_installer_resource_path(&resource_path);

    if file_name.to_ascii_lowercase().ends_with(".msi") {
        let mut args = vec!["/i".into(), normalized];
        args.extend(install_args.iter().cloned());

        return Ok(PlannedCommand {
            program: "msiexec.exe".into(),
            args,
        });
    }

    Ok(PlannedCommand {
        program: normalized,
        args: install_args.to_vec(),
    })
}

pub fn bundled_resource_path(file_name: &str, resource_root: &Path) -> Result<PathBuf, AppError> {
    validate_bundled_resource_file_name(file_name)?;
    Ok(resource_root.join(file_name))
}

fn validate_bundled_resource_file_name(file_name: &str) -> Result<(), AppError> {
    if file_name.trim().is_empty() {
        return Err(invalid_resource_path(file_name));
    }

    let path = Path::new(file_name);
    if path.is_absolute() {
        return Err(invalid_resource_path(file_name));
    }

    let mut has_file_component = false;
    for component in path.components() {
        match component {
            Component::Normal(_) => has_file_component = true,
            _ => return Err(invalid_resource_path(file_name)),
        }
    }

    if !has_file_component {
        return Err(invalid_resource_path(file_name));
    }

    Ok(())
}

fn invalid_resource_path(file_name: &str) -> AppError {
    AppError {
        code: "installer_resource_path_invalid".into(),
        message: "Bundled installer resource path is invalid".into(),
        details: Some(file_name.to_string()),
    }
}

fn normalize_installer_resource_path(path: &Path) -> String {
    let raw = path.to_string_lossy();
    raw.strip_prefix(r"\\?\").unwrap_or(&raw).to_string()
}

pub fn command_display(command: &PlannedCommand) -> String {
    if command.args.is_empty() {
        command.program.clone()
    } else {
        format!("{} {}", command.program, command.args.join(" "))
    }
}

pub fn stage_sequence(flow: &str) -> Vec<InstallStageId> {
    match flow {
        "install_codex" => vec![
            InstallStageId::Preflight,
            InstallStageId::InstallGit,
            InstallStageId::InstallPython,
            InstallStageId::InstallNode,
            InstallStageId::InstallCcSwitch,
            InstallStageId::RefreshEnvironment,
            InstallStageId::InstallCodex,
            InstallStageId::Verify,
        ],
        "install_claude_code" => vec![
            InstallStageId::Preflight,
            InstallStageId::InstallGit,
            InstallStageId::InstallPython,
            InstallStageId::InstallNode,
            InstallStageId::InstallCcSwitch,
            InstallStageId::RefreshEnvironment,
            InstallStageId::InstallClaudeCode,
            InstallStageId::Verify,
        ],
        "install_all" => vec![
            InstallStageId::Preflight,
            InstallStageId::InstallGit,
            InstallStageId::InstallPython,
            InstallStageId::InstallNode,
            InstallStageId::InstallCcSwitch,
            InstallStageId::RefreshEnvironment,
            InstallStageId::InstallCodex,
            InstallStageId::InstallClaudeCode,
            InstallStageId::Verify,
        ],
        _ => vec![],
    }
}
