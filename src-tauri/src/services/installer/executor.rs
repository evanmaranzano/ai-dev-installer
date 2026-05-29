#[cfg(target_os = "windows")]
use std::os::windows::fs::MetadataExt;
use std::path::{Component, Path, PathBuf};

use crate::error::AppError;
use crate::models::installer::{InstallStageId, InstallerLogEntry};

const CODEX_STORE_PRODUCT_ID: &str = "9PLM9XGG6VKS";
const CODEX_NPM_PACKAGE: &str = "@openai/codex";
const CLAUDE_CODE_PACKAGE_SPEC: &str = "@anthropic-ai/claude-code@2.1.150";

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

pub fn codex_install_commands() -> Result<Vec<PlannedCommand>, AppError> {
    Ok(vec![PlannedCommand {
        program: winget_program()?,
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
    }])
}

pub fn codex_npm_fallback_command() -> Result<PlannedCommand, AppError> {
    Ok(PlannedCommand {
        program: find_program_on_path_trusted("npm")
            .ok_or_else(|| trusted_program_missing("npm"))?,
        args: vec![
            "install".into(),
            "-g".into(),
            CODEX_NPM_PACKAGE.into(),
        ],
    })
}

pub fn microsoft_store_service_repair_commands() -> Result<Vec<PlannedCommand>, AppError> {
    let sc = windows_system_program("sc.exe")?;
    let services = ["AppXSvc", "ClipSVC", "InstallService", "StorSvc"];
    let mut commands = Vec::with_capacity(services.len() * 2);

    for service in services {
        commands.push(PlannedCommand {
            program: sc.clone(),
            args: vec!["config".into(), service.into(), "start=".into(), "demand".into()],
        });
        commands.push(PlannedCommand {
            program: sc.clone(),
            args: vec!["start".into(), service.into()],
        });
    }

    Ok(commands)
}

pub fn microsoft_store_product_uri() -> String {
    format!("ms-windows-store://pdp/?ProductId={CODEX_STORE_PRODUCT_ID}")
}

pub fn microsoft_store_product_page_command() -> Result<PlannedCommand, AppError> {
    Ok(PlannedCommand {
        program: windows_system_program("explorer.exe")?,
        args: vec![microsoft_store_product_uri()],
    })
}

pub fn claude_code_install_commands() -> Result<Vec<PlannedCommand>, AppError> {
    Ok(vec![PlannedCommand {
        program: find_program_on_path_trusted("npm")
            .ok_or_else(|| trusted_program_missing("npm"))?,
        args: vec![
            "install".into(),
            "-g".into(),
            CLAUDE_CODE_PACKAGE_SPEC.into(),
        ],
    }])
}

pub fn winget_program() -> Result<String, AppError> {
    winget_candidate_paths()
        .into_iter()
        .find(|path| path.exists() && is_trusted_winget_path(path))
        .map(|path| path.display().to_string())
        .or_else(|| find_program_on_path_trusted("winget"))
        .ok_or_else(|| trusted_program_missing("winget"))
}

pub fn winget_candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(test)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(
            PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("WindowsApps")
                .join("winget.exe"),
        );
    }

    #[cfg(test)]
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

    if let Some(local_app_data) = dirs::data_local_dir() {
        candidates.push(
            local_app_data
                .join("Microsoft")
                .join("WindowsApps")
                .join("winget.exe"),
        );
    }

    candidates
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

pub(super) fn is_in_trusted_directory(path: &Path) -> bool {
    trusted_directory_roots()
        .into_iter()
        .any(|root| path_is_inside(&root, path))
        || trusted_file_paths()
            .into_iter()
            .any(|root| path_matches_trusted_file(&root, path))
}

fn is_trusted_winget_path(path: &Path) -> bool {
    if path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| !name.eq_ignore_ascii_case("winget.exe"))
        .unwrap_or(true)
    {
        return false;
    }

    if is_in_trusted_directory(path) {
        return true;
    }

    winget_windows_apps_roots()
        .into_iter()
        .any(|root| path_is_inside(&root, path))
}

pub(super) fn find_program_on_path_trusted(program: &str) -> Option<String> {
    for path in known_program_candidate_paths(program) {
        if path.exists() && is_in_trusted_directory(&path) {
            return Some(path.display().to_string());
        }
    }

    if let Some(path_env) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path_env) {
            for candidate_name in executable_names(program) {
                let candidate = dir.join(candidate_name);
                if candidate.exists() && is_in_trusted_directory(&candidate) {
                    return Some(candidate.display().to_string());
                }
            }
        }
    }

    None
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
    for base in program_files_roots() {
        for relative_path in relative_paths {
            candidates.push(base.join(relative_path));
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
            program: windows_system_program("msiexec.exe")?,
            args,
        });
    }

    Ok(PlannedCommand {
        program: normalized,
        args: install_args.to_vec(),
    })
}

pub(super) fn windows_system_program(program: &str) -> Result<String, AppError> {
    windows_system_program_candidates(program)
        .into_iter()
        .find(|path| path.exists() && is_in_trusted_directory(path))
        .map(|path| path.display().to_string())
        .ok_or_else(|| trusted_program_missing(program))
}

fn windows_system_program_candidates(program: &str) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(test)]
    if let Some(system_root) = std::env::var_os("SystemRoot") {
        push_windows_program_candidates(&mut candidates, &PathBuf::from(system_root), program);
    }
    #[cfg(test)]
    if let Some(windir) = std::env::var_os("WINDIR") {
        push_windows_program_candidates(&mut candidates, &PathBuf::from(windir), program);
    }
    push_windows_program_candidates(&mut candidates, Path::new(r"C:\Windows"), program);

    candidates
}

fn push_windows_program_candidates(candidates: &mut Vec<PathBuf>, root: &Path, program: &str) {
    match program.to_ascii_lowercase().as_str() {
        "explorer.exe" => candidates.push(root.join(program)),
        _ => candidates.push(root.join("System32").join(program)),
    }
}

fn trusted_directory_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for root in [
        r"C:\Windows\System32",
        r"C:\Program Files",
        r"C:\Program Files (x86)",
    ] {
        roots.push(PathBuf::from(root));
    }

    #[cfg(test)]
    roots.extend(test_trusted_roots());

    roots
}

fn trusted_file_paths() -> Vec<PathBuf> {
    vec![PathBuf::from(r"C:\Windows\explorer.exe")]
}

fn program_files_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(test)]
    roots.extend(test_trusted_roots());

    roots.push(PathBuf::from(r"C:\Program Files"));
    roots.push(PathBuf::from(r"C:\Program Files (x86)"));

    roots
}

#[cfg(test)]
fn test_trusted_roots() -> Vec<PathBuf> {
    std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT")
        .into_iter()
        .flat_map(|value| std::env::split_paths(&value).collect::<Vec<_>>())
        .collect()
}

fn winget_windows_apps_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    #[cfg(test)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        roots.push(
            PathBuf::from(local_app_data)
                .join("Microsoft")
                .join("WindowsApps"),
        );
    }

    if let Some(local_app_data) = dirs::data_local_dir() {
        roots.push(local_app_data.join("Microsoft").join("WindowsApps"));
    }

    roots
}

fn path_is_inside(root: &Path, path: &Path) -> bool {
    // Reject junction/symlink points: check each component before canonicalization
    if has_reparse_point_ancestor(path) {
        return false;
    }

    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };

    path.starts_with(root)
}

#[cfg(target_os = "windows")]
fn has_reparse_point_ancestor(path: &Path) -> bool {
    let mut current = path.to_path_buf();
    loop {
        match std::fs::symlink_metadata(&current) {
            Ok(meta) => {
                if meta.file_type().is_symlink() || (meta.file_attributes() & 0x400) != 0 {
                    return true;
                }
            }
            Err(_) => return false,
        }
        if !current.pop() {
            return false;
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn has_reparse_point_ancestor(_path: &Path) -> bool {
    false
}

fn path_matches_trusted_file(root: &Path, path: &Path) -> bool {
    let Ok(root) = root.canonicalize() else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };

    path == root
}

fn trusted_program_missing(program: &str) -> AppError {
    AppError {
        code: "installer_trusted_program_missing".into(),
        message: format!("Trusted installer program not found: {program}"),
        details: None,
    }
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
