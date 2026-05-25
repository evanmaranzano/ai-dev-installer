use std::collections::HashSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::models::installer::{
    InstallStageId, InstallerComponentState, InstallerComponentStatus, InstallerSnapshot,
};
use crate::services::installer::executor::{find_program_on_path_trusted, is_in_trusted_directory};

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone)]
pub struct DetectedBinary {
    pub version: Option<String>,
    pub path: Option<String>,
}

pub trait EnvironmentProbe {
    fn is_admin(&self) -> bool;
    fn detect_binary(&self, command: &str) -> Option<DetectedBinary>;
    fn detect_known_install(&self, component_id: &str) -> Option<DetectedBinary>;
    fn detect_appx_package(&self, package_name: &str) -> Option<DetectedBinary>;
}

pub struct DetectExecutionEnvironment {
    refreshed_path: OnceLock<Option<OsString>>,
    #[cfg(test)]
    refreshed_path_build_count: AtomicUsize,
}

impl DetectExecutionEnvironment {
    pub fn new() -> Self {
        Self {
            refreshed_path: OnceLock::new(),
            #[cfg(test)]
            refreshed_path_build_count: AtomicUsize::new(0),
        }
    }

    fn detect_path(&self, command: &str) -> Option<String> {
        detect_known_binary_path(command)
            .or_else(|| find_program_on_path_trusted(command).map(PathBuf::from))
            .map(|path| path.display().to_string())
    }

    fn detect_version(&self, command: &str) -> Option<String> {
        let flag = version_flag(command);
        let output = self.command_output(command, &[flag]).ok()?;
        if !output.status.success() {
            return None;
        }

        first_non_empty_output_line(output)
    }

    fn detect_python_local_install(&self) -> Option<DetectedBinary> {
        let mut candidates = vec![
            std::path::PathBuf::from(r"C:\Program Files\Python312\python.exe"),
            std::path::PathBuf::from(r"C:\Program Files\Python313\python.exe"),
        ];

        // Per-user Python installs go to LOCALAPPDATA
        if let Some(local) = dirs::data_local_dir() {
            candidates.push(local.join(r"Programs\Python\Python312\python.exe"));
            candidates.push(local.join(r"Programs\Python\Python313\python.exe"));
        }

        for candidate in candidates {
            if candidate.exists() && is_trusted_detected_binary_path(&candidate) {
                let version = candidate
                    .to_str()
                    .and_then(|path| self.command_output(path, &["--version"]).ok())
                    .and_then(first_non_empty_output_line);

                return Some(DetectedBinary {
                    version,
                    path: Some(candidate.display().to_string()),
                });
            }
        }

        None
    }

    fn detect_appx_package_impl(&self, package_name: &str) -> Option<DetectedBinary> {
        let script = format!(
            "(Get-AppxPackage -Name '{package_name}' -ErrorAction SilentlyContinue | Select-Object -First 1 | ForEach-Object {{ \"{{0}}|{{1}}\" -f $_.Version, $_.InstallLocation }})"
        );
        let output = self
            .command_output(
                windows_system_tool_path("powershell.exe"),
                &["-NoProfile", "-Command", &script],
            )
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let line = String::from_utf8(output.stdout).ok()?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }

        let mut parts = trimmed.splitn(2, '|');
        let version = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let path = parts
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty());

        Some(DetectedBinary {
            version: version.map(|value| value.to_string()),
            path: path.map(|value| value.to_string()),
        })
    }

    fn command_output(
        &self,
        program: &str,
        args: &[&str],
    ) -> std::io::Result<std::process::Output> {
        let mut command = Command::new(program);
        command.args(args);
        if let Some(path) = self.refreshed_path_env() {
            command.env("PATH", path);
        }
        #[cfg(target_os = "windows")]
        {
            command.creation_flags(CREATE_NO_WINDOW);
        }
        command.output()
    }

    fn refreshed_path_env(&self) -> Option<OsString> {
        self.refreshed_path
            .get_or_init(|| {
                #[cfg(test)]
                self.refreshed_path_build_count
                    .fetch_add(1, Ordering::SeqCst);

                refreshed_path_env()
            })
            .clone()
    }

    #[cfg(test)]
    pub(super) fn refreshed_path_env_build_count_for_tests(&self) -> usize {
        self.refreshed_path_build_count.load(Ordering::SeqCst)
    }
}

impl Default for DetectExecutionEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvironmentProbe for DetectExecutionEnvironment {
    fn is_admin(&self) -> bool {
        self.command_output(windows_system_tool_path("net.exe"), &["session"])
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    fn detect_binary(&self, command: &str) -> Option<DetectedBinary> {
        let path = self
            .detect_path(command)
            .or_else(|| detect_known_binary_path(command).map(|path| path.display().to_string()));
        let version = match (command, path.as_deref()) {
            ("cc-switch" | "cc-switch.exe", _) => None,
            (_, Some(path)) => self.detect_version(path),
            _ => self.detect_version(command),
        };

        if path.is_none() && version.is_none() {
            None
        } else {
            Some(DetectedBinary { version, path })
        }
    }

    fn detect_known_install(&self, component_id: &str) -> Option<DetectedBinary> {
        match component_id {
            "cc_switch" => detect_cc_switch_local_install(),
            "python" => self.detect_python_local_install(),
            _ => None,
        }
    }

    fn detect_appx_package(&self, package_name: &str) -> Option<DetectedBinary> {
        self.detect_appx_package_impl(package_name)
    }
}

pub fn build_initial_snapshot(probe: &dyn EnvironmentProbe) -> InstallerSnapshot {
    let components = vec![
        component_from_detection(probe, "git", "Git"),
        python_component_from_detection(probe),
        component_from_detection(probe, "node", "Node.js"),
        cc_switch_component_from_detection(probe),
        codex_component_from_detection(probe),
        claude_code_component_from_detection(probe),
    ];

    InstallerSnapshot {
        current_stage: InstallStageId::Idle,
        progress_percent: 0,
        components,
        logs: vec![],
        last_error: if probe.is_admin() {
            None
        } else {
            Some("当前未以管理员身份运行。".into())
        },
    }
}

fn cc_switch_component_from_detection(probe: &dyn EnvironmentProbe) -> InstallerComponentState {
    let candidates = ["cc-switch", "cc-switch.exe"];

    for command in candidates {
        if let Some(found) = probe.detect_binary(command) {
            return InstallerComponentState {
                id: "cc_switch".into(),
                label: "CC Switch".into(),
                status: InstallerComponentStatus::Installed,
                detail: found.path.unwrap_or_else(|| "已检测到命令".into()),
                version: found.version,
            };
        }
    }

    if let Some(found) = probe.detect_known_install("cc_switch") {
        return InstallerComponentState {
            id: "cc_switch".into(),
            label: "CC Switch".into(),
            status: InstallerComponentStatus::Installed,
            detail: found.path.unwrap_or_else(|| "已检测到本地安装".into()),
            version: found.version,
        };
    }

    InstallerComponentState {
        id: "cc_switch".into(),
        label: "CC Switch".into(),
        status: InstallerComponentStatus::NotInstalled,
        detail: "未检测到".into(),
        version: None,
    }
}

fn detect_cc_switch_local_install() -> Option<DetectedBinary> {
    let path = std::env::var_os("LOCALAPPDATA")
        .map(|base| {
            std::path::PathBuf::from(base)
                .join("Programs")
                .join("CC Switch")
                .join("cc-switch.exe")
        })
        .filter(|path| path.exists())?;

    if !is_trusted_detected_binary_path(&path) {
        return None;
    }

    Some(DetectedBinary {
        version: None,
        path: Some(path.display().to_string()),
    })
}

fn codex_component_from_detection(probe: &dyn EnvironmentProbe) -> InstallerComponentState {
    if let Some(found) = probe.detect_binary("codex") {
        InstallerComponentState {
            id: "codex".into(),
            label: "Codex".into(),
            status: InstallerComponentStatus::Installed,
            detail: found.path.unwrap_or_else(|| "已检测到命令".into()),
            version: found.version,
        }
    } else if let Some(found) = probe.detect_appx_package("OpenAI.Codex") {
        InstallerComponentState {
            id: "codex".into(),
            label: "Codex".into(),
            status: InstallerComponentStatus::Installed,
            detail: found.path.unwrap_or_else(|| "Microsoft Store app".into()),
            version: found.version,
        }
    } else {
        InstallerComponentState {
            id: "codex".into(),
            label: "Codex".into(),
            status: InstallerComponentStatus::NotInstalled,
            detail: "未检测到".into(),
            version: None,
        }
    }
}

fn claude_code_component_from_detection(probe: &dyn EnvironmentProbe) -> InstallerComponentState {
    for command in ["claude", "claude.exe"] {
        if let Some(found) = probe.detect_binary(command) {
            return InstallerComponentState {
                id: "claude_code".into(),
                label: "Claude Code".into(),
                status: InstallerComponentStatus::Installed,
                detail: found.path.unwrap_or_else(|| "已检测到命令".into()),
                version: found.version,
            };
        }
    }

    // Also check npm global bin directory (%APPDATA%\npm) since npm install -g
    // may place claude.cmd outside the trusted path set.
    if let Some(npm_global_bin) = std::env::var_os("APPDATA").map(|p| {
        let mut path = std::path::PathBuf::from(p);
        path.push("npm");
        path
    }) {
        for name in ["claude.cmd", "claude.exe"] {
            let candidate = npm_global_bin.join(name);
            if candidate.exists() {
                return InstallerComponentState {
                    id: "claude_code".into(),
                    label: "Claude Code".into(),
                    status: InstallerComponentStatus::Installed,
                    detail: candidate.display().to_string(),
                    version: None,
                };
            }
        }
    }

    InstallerComponentState {
        id: "claude_code".into(),
        label: "Claude Code".into(),
        status: InstallerComponentStatus::NotInstalled,
        detail: "未检测到".into(),
        version: None,
    }
}

fn python_component_from_detection(probe: &dyn EnvironmentProbe) -> InstallerComponentState {
    if let Some(found) = probe.detect_binary("python") {
        let detail = found.path.unwrap_or_else(|| "已检测到命令".into());
        return InstallerComponentState {
            id: "python".into(),
            label: "Python".into(),
            status: InstallerComponentStatus::Installed,
            detail,
            version: found.version,
        };
    }

    if let Some(found) = probe.detect_known_install("python") {
        let detail = found.path.unwrap_or_else(|| "已检测到本地安装".into());
        return InstallerComponentState {
            id: "python".into(),
            label: "Python".into(),
            status: InstallerComponentStatus::Installed,
            detail,
            version: found.version,
        };
    }

    for command in ["py"] {
        if let Some(found) = probe.detect_binary(command) {
            let detail = found.path.unwrap_or_else(|| "已检测到命令".into());
            return InstallerComponentState {
                id: "python".into(),
                label: "Python".into(),
                status: InstallerComponentStatus::Installed,
                detail,
                version: found.version,
            };
        }
    }

    InstallerComponentState {
        id: "python".into(),
        label: "Python".into(),
        status: InstallerComponentStatus::NotInstalled,
        detail: "未检测到".into(),
        version: None,
    }
}

fn component_from_detection(
    probe: &dyn EnvironmentProbe,
    command: &str,
    label: &str,
) -> InstallerComponentState {
    match probe.detect_binary(command) {
        Some(found) => InstallerComponentState {
            id: component_id(label),
            label: label.to_string(),
            status: InstallerComponentStatus::Installed,
            detail: found.path.unwrap_or_else(|| "已检测到命令".into()),
            version: found.version,
        },
        None => InstallerComponentState {
            id: component_id(label),
            label: label.to_string(),
            status: InstallerComponentStatus::NotInstalled,
            detail: "未检测到".into(),
            version: None,
        },
    }
}

fn component_id(label: &str) -> String {
    label
        .to_ascii_lowercase()
        .replace('.', "")
        .replace(' ', "_")
}

fn detect_known_binary_path(command: &str) -> Option<PathBuf> {
    known_binary_candidates(command)
        .into_iter()
        .find(|path| path.exists() && is_trusted_detected_binary_path(path))
}

fn known_binary_candidates(command: &str) -> Vec<PathBuf> {
    let normalized = command.to_ascii_lowercase();
    let mut candidates = Vec::new();

    match normalized.as_str() {
        "git" | "git.exe" => {
            push_program_files_candidates(
                &mut candidates,
                &[r"Git\cmd\git.exe", r"Git\bin\git.exe"],
            );
        }
        "node" | "node.exe" => {
            push_program_files_candidates(&mut candidates, &[r"nodejs\node.exe"]);
        }
        "npm" | "npm.cmd" | "npm.exe" => {
            push_program_files_candidates(&mut candidates, &[r"nodejs\npm.cmd"]);
        }
        _ => {}
    }

    candidates
}

fn push_program_files_candidates(candidates: &mut Vec<PathBuf>, relative_paths: &[&str]) {
    #[cfg(test)]
    if let Some(root) = std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT") {
        for base in std::env::split_paths(&root) {
            for relative_path in relative_paths {
                candidates.push(base.join(relative_path));
            }
        }
    }

    for base in [
        PathBuf::from(r"C:\Program Files"),
        PathBuf::from(r"C:\Program Files (x86)"),
    ] {
        for relative_path in relative_paths {
            candidates.push(base.join(relative_path));
        }
    }
}

fn refreshed_path_env() -> Option<OsString> {
    let mut entries = Vec::new();
    let mut seen = HashSet::new();

    append_path_entries(std::env::var_os("PATH"), true, &mut entries, &mut seen);

    #[cfg(target_os = "windows")]
    {
        for (value, trusted_only) in registry_path_values() {
            append_path_entries(Some(value), trusted_only, &mut entries, &mut seen);
        }
        for path in known_path_entries() {
            push_unique_path(path, &mut entries, &mut seen);
        }
    }

    std::env::join_paths(entries).ok()
}

fn append_path_entries(
    value: Option<OsString>,
    trusted_only: bool,
    entries: &mut Vec<PathBuf>,
    seen: &mut HashSet<String>,
) {
    let Some(value) = value else {
        return;
    };

    for path in std::env::split_paths(&value) {
        if trusted_only && !is_trusted_detected_binary_path(&path) {
            continue;
        }
        push_unique_path(path, entries, seen);
    }
}

fn push_unique_path(path: PathBuf, entries: &mut Vec<PathBuf>, seen: &mut HashSet<String>) {
    if path.as_os_str().is_empty() {
        return;
    }

    let key = path.to_string_lossy().to_ascii_lowercase();
    if seen.insert(key) {
        entries.push(path);
    }
}

fn is_trusted_detected_binary_path(path: &Path) -> bool {
    is_in_trusted_directory(path)
}

#[cfg(target_os = "windows")]
fn registry_path_values() -> Vec<(OsString, bool)> {
    [
        (
            r"HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            "Path",
            true,
        ),
        (r"HKCU\Environment", "Path", true),
    ]
    .into_iter()
    .filter_map(|(key, value, trusted_only)| {
        read_registry_value(key, value).map(|path| (path, trusted_only))
    })
    .collect()
}

#[cfg(target_os = "windows")]
fn read_registry_value(key: &str, value: &str) -> Option<OsString> {
    let output = command_output_without_refreshed_path(
        windows_system_tool_path("reg.exe"),
        &["query", key, "/v", value],
    )
    .ok()?;
    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8(output.stdout).ok()?;
    stdout
        .lines()
        .filter_map(|line| parse_reg_query_value_line(line, value))
        .next()
        .map(expand_windows_env_vars)
        .map(OsString::from)
}

#[cfg(target_os = "windows")]
fn parse_reg_query_value_line(line: &str, value: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed
        .to_ascii_lowercase()
        .starts_with(&value.to_ascii_lowercase())
    {
        return None;
    }

    let mut parts = trimmed.split_whitespace();
    let _name = parts.next()?;
    let _kind = parts.next()?;
    let data = parts.collect::<Vec<_>>().join(" ");

    if data.is_empty() {
        None
    } else {
        Some(data)
    }
}

#[cfg(target_os = "windows")]
fn expand_windows_env_vars(value: String) -> String {
    let mut expanded = value;
    for (name, replacement) in std::env::vars() {
        expanded = expanded.replace(&format!("%{name}%"), &replacement);
        expanded = expanded.replace(&format!("%{}%", name.to_ascii_uppercase()), &replacement);
        expanded = expanded.replace(&format!("%{}%", name.to_ascii_lowercase()), &replacement);
    }
    expanded
}

#[cfg(target_os = "windows")]
fn known_path_entries() -> Vec<PathBuf> {
    known_binary_candidates("git")
        .into_iter()
        .chain(known_binary_candidates("node"))
        .chain(known_binary_candidates("npm"))
        .filter_map(|path| path.parent().map(Path::to_path_buf))
        .collect()
}

#[cfg(target_os = "windows")]
fn command_output_without_refreshed_path(
    program: &str,
    args: &[&str],
) -> std::io::Result<std::process::Output> {
    let mut command = Command::new(program);
    command.args(args);
    command.creation_flags(CREATE_NO_WINDOW);
    command.output()
}

#[cfg(target_os = "windows")]
fn windows_system_tool_path(program: &str) -> &str {
    match program.to_ascii_lowercase().as_str() {
        "powershell.exe" => r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe",
        "reg.exe" => r"C:\Windows\System32\reg.exe",
        "net.exe" => r"C:\Windows\System32\net.exe",
        _ => program,
    }
}

#[cfg(not(target_os = "windows"))]
fn windows_system_tool_path(program: &str) -> &str {
    program
}

fn first_non_empty_output_line(output: std::process::Output) -> Option<String> {
    let stdout = String::from_utf8(output.stdout).ok();
    let stderr = String::from_utf8(output.stderr).ok();

    stdout
        .into_iter()
        .chain(stderr)
        .flat_map(|text| {
            text.lines()
                .map(str::trim)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .find(|line| !line.is_empty())
}

fn version_flag(command: &str) -> &'static str {
    match command {
        "claude" => "-v",
        "py" => "--version",
        _ => "--version",
    }
}
