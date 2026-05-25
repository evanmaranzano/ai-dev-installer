use crate::error::AppError;
use crate::services::installer::environment::{
    build_initial_snapshot, DetectExecutionEnvironment, DetectedBinary, EnvironmentProbe,
};
use crate::services::installer::executor::{
    claude_code_install_commands, codex_install_commands, command_display,
    find_program_on_path_trusted, is_in_trusted_directory, microsoft_store_product_page_command,
    microsoft_store_product_uri, microsoft_store_service_repair_commands, stage_sequence,
    third_party_install_command, winget_candidate_paths,
};
use crate::services::installer::manifest::{verify_sha256, InstallerManifest};
use crate::services::installer::service::{
    codex_store_install_failure_error, component_status, mark_component_skipped_if_installed,
    sanitize_installer_command_output, timeout_cleanup_failure_detail,
    timeout_process_tree_kill_command, InstallerService, InstallerSessionState,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENV_LOCK: Mutex<()> = Mutex::new(());

struct FakeProbe;

impl EnvironmentProbe for FakeProbe {
    fn is_admin(&self) -> bool {
        true
    }

    fn detect_binary(&self, command: &str) -> Option<DetectedBinary> {
        match command {
            "git" => Some(DetectedBinary {
                version: Some("2.45.1".into()),
                path: Some("C:\\Program Files\\Git\\bin\\git.exe".into()),
            }),
            "python" => Some(DetectedBinary {
                version: Some("Python 3.12.10".into()),
                path: Some("C:\\Program Files\\Python312\\python.exe".into()),
            }),
            "node" => Some(DetectedBinary {
                version: Some("v22.9.0".into()),
                path: Some("C:\\Program Files\\nodejs\\node.exe".into()),
            }),
            "npm" => Some(DetectedBinary {
                version: Some("10.8.3".into()),
                path: Some("C:\\Program Files\\nodejs\\npm.cmd".into()),
            }),
            "claude" => Some(DetectedBinary {
                version: Some("0.0.77".into()),
                path: Some("C:\\Users\\Administrator\\AppData\\Roaming\\npm\\claude.cmd".into()),
            }),
            _ => None,
        }
    }

    fn detect_known_install(&self, _component_id: &str) -> Option<DetectedBinary> {
        match _component_id {
            "python" => Some(DetectedBinary {
                version: Some("Python 3.12.10".into()),
                path: Some("C:\\Program Files\\Python312\\python.exe".into()),
            }),
            _ => None,
        }
    }

    fn detect_appx_package(&self, _package_name: &str) -> Option<DetectedBinary> {
        None
    }
}

struct NonAdminProbe;

impl EnvironmentProbe for NonAdminProbe {
    fn is_admin(&self) -> bool {
        false
    }

    fn detect_binary(&self, command: &str) -> Option<DetectedBinary> {
        match command {
            "claude" => Some(DetectedBinary {
                version: None,
                path: None,
            }),
            _ => None,
        }
    }

    fn detect_known_install(&self, _component_id: &str) -> Option<DetectedBinary> {
        None
    }

    fn detect_appx_package(&self, _package_name: &str) -> Option<DetectedBinary> {
        None
    }
}

struct CodexStoreProbe;

impl EnvironmentProbe for CodexStoreProbe {
    fn is_admin(&self) -> bool {
        true
    }

    fn detect_binary(&self, _command: &str) -> Option<DetectedBinary> {
        None
    }

    fn detect_known_install(&self, _component_id: &str) -> Option<DetectedBinary> {
        None
    }

    fn detect_appx_package(&self, package_name: &str) -> Option<DetectedBinary> {
        if package_name == "OpenAI.Codex" {
            Some(DetectedBinary {
                version: Some("26.416.11627".into()),
                path: Some("Microsoft Store app".into()),
            })
        } else {
            None
        }
    }
}

#[test]
fn builds_snapshot_from_detected_machine_state() {
    let snapshot = build_initial_snapshot(&FakeProbe);

    assert_eq!(snapshot.current_stage, "idle");
    assert_eq!(snapshot.progress_percent, 0);
    assert_eq!(snapshot.last_error, None);
    assert_eq!(snapshot.components.len(), 6);
    assert_eq!(snapshot.components[0].id, "git");
    assert_eq!(snapshot.components[0].label, "Git");
    assert_eq!(snapshot.components[0].status, "installed");
    assert_eq!(
        snapshot.components[0].detail,
        "C:\\Program Files\\Git\\bin\\git.exe"
    );
    assert_eq!(snapshot.components[0].version.as_deref(), Some("2.45.1"));
    assert_eq!(snapshot.components[1].id, "python");
    assert_eq!(snapshot.components[1].label, "Python");
    assert_eq!(snapshot.components[1].status, "installed");
    assert_eq!(
        snapshot.components[1].detail,
        "C:\\Program Files\\Python312\\python.exe"
    );
    assert_eq!(
        snapshot.components[1].version.as_deref(),
        Some("Python 3.12.10")
    );
    assert_eq!(snapshot.components[2].id, "nodejs");
    assert_eq!(snapshot.components[2].label, "Node.js");
    assert_eq!(snapshot.components[2].status, "installed");
    assert_eq!(
        snapshot.components[2].detail,
        "C:\\Program Files\\nodejs\\node.exe"
    );
    assert_eq!(snapshot.components[2].version.as_deref(), Some("v22.9.0"));
    assert_eq!(snapshot.components[3].id, "cc_switch");
    assert_eq!(snapshot.components[3].status, "not_installed");
    assert_eq!(snapshot.components[3].detail, "未检测到");
    assert_eq!(snapshot.components[4].id, "codex");
    assert_eq!(snapshot.components[5].id, "claude_code");
    assert_eq!(snapshot.components[5].label, "Claude Code");
    assert_eq!(snapshot.components[5].status, "installed");
    assert_eq!(
        snapshot.components[5].detail,
        "C:\\Users\\Administrator\\AppData\\Roaming\\npm\\claude.cmd"
    );
    assert_eq!(snapshot.components[5].version.as_deref(), Some("0.0.77"));
    assert!(snapshot.logs.is_empty());
}

#[test]
fn builds_non_admin_snapshot_with_error_and_detected_fallback_detail() {
    let snapshot = build_initial_snapshot(&NonAdminProbe);

    assert_eq!(
        snapshot.last_error.as_deref(),
        Some("当前未以管理员身份运行。")
    );
    assert_eq!(snapshot.components[4].id, "codex");
    assert_eq!(snapshot.components[4].status, "not_installed");
    assert_eq!(snapshot.components[4].detail, "未检测到");
    assert_eq!(snapshot.components[4].version, None);
    assert_eq!(snapshot.components[5].id, "claude_code");
    assert_eq!(snapshot.components[5].status, "installed");
    assert_eq!(snapshot.components[5].detail, "已检测到命令");
    assert_eq!(snapshot.components[5].version, None);
}

#[test]
fn detects_codex_via_microsoft_store_package_probe() {
    let snapshot = build_initial_snapshot(&CodexStoreProbe);

    assert_eq!(snapshot.components[4].id, "codex");
    assert_eq!(snapshot.components[4].status, "installed");
    assert_eq!(snapshot.components[4].detail, "Microsoft Store app");
    assert_eq!(
        snapshot.components[4].version.as_deref(),
        Some("26.416.11627")
    );
    assert_eq!(snapshot.components[5].id, "claude_code");
    assert_eq!(snapshot.components[5].status, "not_installed");
}

#[test]
fn installer_snapshot_serializes_with_camel_case_contract() {
    let snapshot = build_initial_snapshot(&FakeProbe);
    let value = serde_json::to_value(&snapshot).expect("snapshot should serialize");

    assert_eq!(value["currentStage"], json!("idle"));
    assert_eq!(value["progressPercent"], json!(0));
    assert_eq!(value["lastError"], serde_json::Value::Null);
    assert!(value.get("current_stage").is_none());
    assert!(value.get("progress_percent").is_none());
    assert!(value.get("last_error").is_none());
}

#[test]
fn parses_third_party_manifest_and_verifies_order() {
    let manifest = crate::services::installer::manifest::InstallerManifest {
        resources: vec![
            crate::services::installer::manifest::BundledResource {
                component_id: "git".into(),
                version: "2.45.1".into(),
                file_name: "Git-2.45.1-64-bit.exe".into(),
                sha256: "abc".into(),
                install_command: vec!["/VERYSILENT".into()],
            },
            crate::services::installer::manifest::BundledResource {
                component_id: "nodejs".into(),
                version: "22.9.0".into(),
                file_name: "node-v22.9.0-x64.msi".into(),
                sha256: "def".into(),
                install_command: vec!["/qn".into()],
            },
        ],
    };

    assert_eq!(manifest.resources[0].component_id, "git");
    assert_eq!(manifest.resources[1].component_id, "nodejs");
}

#[test]
fn parses_manifest_json_and_finds_resource_by_component_id() {
    let manifest_json = include_str!("../../../resources/third_party/manifest.json");
    let manifest = InstallerManifest::from_json_str(manifest_json).expect("manifest should parse");

    assert_eq!(manifest.resources.len(), 4);
    assert_eq!(
        manifest
            .resource("cc_switch")
            .expect("cc_switch resource should exist")
            .file_name,
        "cc-switch/CC-Switch-v3.14.1-Windows.msi"
    );
}

#[test]
fn verifies_sha256_for_temp_file_contents() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_path = std::env::temp_dir().join(format!("installer-manifest-{unique}.bin"));
    let payload = b"installer-payload";
    fs::write(&temp_path, payload).expect("temp file should be written");

    let expected = hex::encode(Sha256::digest(payload));
    let verified = verify_sha256(&temp_path, &expected).expect("hash verification should work");
    let mismatch = verify_sha256(&temp_path, "deadbeef").expect("mismatch should not error");

    assert!(verified);
    assert!(!mismatch);

    fs::remove_file(temp_path).expect("temp file should be removed");
}

#[test]
fn verifies_sha256_for_payload_larger_than_hash_buffer() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_path = std::env::temp_dir().join(format!("installer-manifest-large-{unique}.bin"));
    let payload = vec![b'a'; 70 * 1024];
    fs::write(&temp_path, &payload).expect("large temp file should be written");

    let expected = hex::encode(Sha256::digest(&payload));
    let verified = verify_sha256(&temp_path, &expected).expect("hash verification should work");

    assert!(verified);

    fs::remove_file(temp_path).expect("large temp file should be removed");
}

#[test]
fn plans_codex_install_with_msstore_winget_product_id() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-winget-{unique}"));
    let winget_path = temp_root
        .join("Microsoft")
        .join("WindowsApps")
        .join("winget.exe");

    fs::create_dir_all(winget_path.parent().unwrap()).expect("fake winget dir should be created");
    fs::write(&winget_path, b"fake winget").expect("fake winget should be written");

    let original_local_app_data = std::env::var_os("LOCALAPPDATA");
    std::env::set_var("LOCALAPPDATA", &temp_root);

    let commands = codex_install_commands().expect("codex command should build");

    restore_env_var("LOCALAPPDATA", original_local_app_data);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, winget_path.display().to_string());
    assert_eq!(
        commands[0].args,
        vec![
            "install".to_string(),
            "--id".to_string(),
            "9PLM9XGG6VKS".to_string(),
            "--source".to_string(),
            "msstore".to_string(),
            "--accept-source-agreements".to_string(),
            "--accept-package-agreements".to_string(),
            "--silent".to_string()
        ]
    );
}

#[test]
fn plans_microsoft_store_service_repair_before_codex_install() {
    let commands =
        microsoft_store_service_repair_commands().expect("store repair commands should build");
    let command_lines: Vec<String> = commands.iter().map(command_display).collect();

    assert_eq!(commands.len(), 6);
    assert!(commands
        .iter()
        .all(|command| command.program.to_ascii_lowercase().ends_with("sc.exe")));
    assert!(command_lines
        .iter()
        .any(|line| line.ends_with("sc.exe config AppXSvc start= demand")));
    assert!(command_lines
        .iter()
        .any(|line| line.ends_with("sc.exe start AppXSvc")));
    assert!(command_lines
        .iter()
        .any(|line| line.ends_with("sc.exe config ClipSVC start= demand")));
    assert!(command_lines
        .iter()
        .any(|line| line.ends_with("sc.exe start ClipSVC")));
    assert!(command_lines
        .iter()
        .any(|line| line.ends_with("sc.exe config InstallService start= demand")));
    assert!(command_lines
        .iter()
        .any(|line| line.ends_with("sc.exe start InstallService")));
}

#[test]
fn plans_microsoft_store_product_page_wakeup_for_codex() {
    let command = microsoft_store_product_page_command().expect("store page command should build");

    assert_eq!(
        microsoft_store_product_uri(),
        "ms-windows-store://pdp/?ProductId=9PLM9XGG6VKS"
    );
    assert!(command
        .program
        .to_ascii_lowercase()
        .ends_with("explorer.exe"));
    assert_eq!(command.args, vec![microsoft_store_product_uri()]);
}

#[test]
fn codex_store_failure_error_mentions_service_repair_and_store_wakeup() {
    let error = codex_store_install_failure_error(AppError {
        code: "installer_command_failed_install_codex".into(),
        message: "Command failed: winget install".into(),
        details: Some("0x8A150044".into()),
    });

    assert_eq!(error.code, "installer_codex_store_install_failed");
    let details = error
        .details
        .expect("details should explain recovery actions");
    assert!(details.contains("Microsoft Store"));
    assert!(details.contains("App Installer"));
    assert!(details.contains("AppXSvc"));
    assert!(details.contains("ClipSVC"));
    assert!(details.contains("InstallService"));
    assert!(details.contains("ms-windows-store://pdp/?ProductId=9PLM9XGG6VKS"));
    assert!(details.contains("0x8A150044"));
}

#[test]
fn plans_claude_code_install_with_official_npm_package() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-official-npm-{unique}"));
    let npm_path = temp_root.join("nodejs").join("npm.cmd");

    fs::create_dir_all(npm_path.parent().unwrap()).expect("fake npm dir should be created");
    fs::write(&npm_path, b"@echo off\r\n").expect("fake npm should be written");

    let original_path = std::env::var_os("PATH");
    let original_test_trust_root = std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT");
    std::env::set_var("PATH", "");
    std::env::set_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", &temp_root);

    let commands = claude_code_install_commands().expect("claude command should build");

    restore_env_var("PATH", original_path);
    restore_env_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", original_test_trust_root);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program, npm_path.display().to_string());
    assert_eq!(
        commands[0].args,
        vec![
            "install".to_string(),
            "-g".to_string(),
            "@anthropic-ai/claude-code@2.1.150".to_string(),
        ]
    );
}

#[test]
fn environment_detection_ignores_untrusted_path_programs() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-env-spoof-{unique}"));
    let untrusted_dir = temp_root.join("user-bin");
    let spoofed_tool = untrusted_dir.join("evil-tool.cmd");

    fs::create_dir_all(&untrusted_dir).expect("untrusted dir should be created");
    fs::write(&spoofed_tool, b"@echo off\r\necho evil-tool 9.9.9\r\n")
        .expect("spoofed command should be written");

    let original_path = std::env::var_os("PATH");
    let original_program_files = std::env::var_os("ProgramFiles");
    let original_program_files_x86 = std::env::var_os("ProgramFiles(x86)");
    let original_test_trust_root = std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT");
    std::env::set_var("PATH", &untrusted_dir);
    std::env::set_var("ProgramFiles", temp_root.join("Program Files"));
    std::env::set_var("ProgramFiles(x86)", temp_root.join("Program Files (x86)"));
    std::env::remove_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT");

    let detected = DetectExecutionEnvironment::new().detect_binary("evil-tool");

    restore_env_var("PATH", original_path);
    restore_env_var("ProgramFiles", original_program_files);
    restore_env_var("ProgramFiles(x86)", original_program_files_x86);
    restore_env_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", original_test_trust_root);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert!(detected.is_none());
}

#[test]
fn plans_claude_code_install_with_refreshed_program_files_path() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-program-files-{unique}"));
    let node_dir = temp_root.join("nodejs");
    let npm_path = node_dir.join("npm.cmd");

    fs::create_dir_all(&node_dir).expect("fake node dir should be created");
    fs::write(&npm_path, b"@echo off\r\n").expect("fake npm command should be written");

    let original_path = std::env::var_os("PATH");
    let original_test_trust_root = std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT");
    std::env::set_var("PATH", "");
    std::env::set_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", &temp_root);

    let commands = claude_code_install_commands().expect("claude command should build");

    restore_env_var("PATH", original_path);
    restore_env_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", original_test_trust_root);
    fs::remove_dir_all(&temp_root).expect("fake program files dir should be removed");

    assert_eq!(commands[0].program, npm_path.display().to_string());
}

#[test]
fn rejects_executable_path_that_only_prefix_matches_trusted_directory() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-trusted-prefix-{unique}"));
    let trusted_root = temp_root.join("Program Files");
    let spoofed_root = temp_root.join("Program Files Evil");
    let spoofed_npm = spoofed_root.join("nodejs").join("npm.cmd");

    fs::create_dir_all(&trusted_root).expect("trusted root should be created");
    fs::create_dir_all(spoofed_npm.parent().unwrap()).expect("spoofed dir should be created");
    fs::write(&spoofed_npm, b"@echo off\r\n").expect("spoofed npm should be written");

    let original_test_trust_root = std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT");
    std::env::set_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", &trusted_root);

    let trusted = is_in_trusted_directory(&spoofed_npm);

    restore_env_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", original_test_trust_root);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert!(!trusted);
}

#[test]
fn does_not_fall_back_to_unqualified_program_from_untrusted_path() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-untrusted-program-{unique}"));
    let untrusted_dir = temp_root.join("bin");
    let untrusted_program = untrusted_dir.join("untrusted-tool.cmd");

    fs::create_dir_all(&untrusted_dir).expect("untrusted dir should be created");
    fs::write(&untrusted_program, b"@echo off\r\n").expect("untrusted program should be written");

    let original_path = std::env::var_os("PATH");
    std::env::set_var("PATH", &untrusted_dir);

    let found = find_program_on_path_trusted("untrusted-tool");

    restore_env_var("PATH", original_path);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert_eq!(found, None);
}

#[test]
fn does_not_trust_program_files_environment_override() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-program-files-env-{unique}"));
    let spoofed_npm = temp_root.join("nodejs").join("npm.cmd");

    fs::create_dir_all(spoofed_npm.parent().unwrap()).expect("spoofed dir should be created");
    fs::write(&spoofed_npm, b"@echo off\r\n").expect("spoofed npm should be written");

    let original_path = std::env::var_os("PATH");
    let original_program_files = std::env::var_os("ProgramFiles");
    let original_test_trust_root = std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT");
    std::env::set_var("PATH", "");
    std::env::set_var("ProgramFiles", &temp_root);
    std::env::remove_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT");

    let found = find_program_on_path_trusted("npm");

    restore_env_var("PATH", original_path);
    restore_env_var("ProgramFiles", original_program_files);
    restore_env_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", original_test_trust_root);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert_ne!(
        found.as_deref(),
        Some(
            spoofed_npm
                .to_str()
                .expect("spoofed npm path should be utf8")
        )
    );
}

#[test]
fn does_not_trust_programs_under_unapproved_windows_subdirectories() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-windows-temp-{unique}"));
    let windows_root = temp_root.join("Windows");
    let spoofed_tool = windows_root.join("Temp").join("temp-spoof.cmd");

    fs::create_dir_all(spoofed_tool.parent().unwrap()).expect("spoofed dir should be created");
    fs::write(&spoofed_tool, b"@echo off\r\n").expect("spoofed npm should be written");

    let original_path = std::env::var_os("PATH");
    let original_test_trust_root = std::env::var_os("AI_DEV_INSTALLER_TEST_TRUST_ROOT");
    std::env::set_var("PATH", spoofed_tool.parent().unwrap());
    std::env::set_var(
        "AI_DEV_INSTALLER_TEST_TRUST_ROOT",
        windows_root.join("System32"),
    );

    let found = find_program_on_path_trusted("temp-spoof");

    restore_env_var("PATH", original_path);
    restore_env_var("AI_DEV_INSTALLER_TEST_TRUST_ROOT", original_test_trust_root);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert_eq!(found, None);
}

#[test]
fn does_not_trust_path_candidate_from_local_app_data_microsoft_directory() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time should move forward")
        .as_nanos();
    let temp_root = std::env::temp_dir().join(format!("installer-localappdata-path-{unique}"));
    let microsoft_dir = temp_root.join("Microsoft").join("spoof");
    let spoofed_tool = microsoft_dir.join("localapp-spoof.cmd");

    fs::create_dir_all(&microsoft_dir).expect("spoofed microsoft dir should be created");
    fs::write(&spoofed_tool, b"@echo off\r\n").expect("spoofed tool should be written");

    let original_path = std::env::var_os("PATH");
    let original_local_app_data = std::env::var_os("LOCALAPPDATA");
    std::env::set_var("PATH", &microsoft_dir);
    std::env::set_var("LOCALAPPDATA", &temp_root);

    let found = find_program_on_path_trusted("localapp-spoof");

    restore_env_var("PATH", original_path);
    restore_env_var("LOCALAPPDATA", original_local_app_data);
    fs::remove_dir_all(&temp_root).expect("temp root should be removed");

    assert_eq!(found, None);
}

#[cfg(target_os = "windows")]
#[test]
fn timeout_cleanup_targets_the_child_process_tree() {
    let command = timeout_process_tree_kill_command(1234)
        .expect("taskkill cleanup command should be available");

    assert!(command
        .program
        .to_ascii_lowercase()
        .ends_with("taskkill.exe"));
    assert_eq!(
        command.args,
        vec![
            "/PID".to_string(),
            "1234".to_string(),
            "/T".to_string(),
            "/F".to_string()
        ]
    );
}

#[test]
fn loads_bundled_manifest_from_compiled_trust_root() {
    let manifest = InstallerManifest::bundled().expect("bundled manifest should parse");

    assert_eq!(manifest.resources.len(), 4);
    assert_eq!(
        manifest
            .resource("git")
            .expect("git resource should exist")
            .sha256,
        "2b96e7854f0520f0f6b709c21041d9801b1be44d5e1a0d9fa621b2fbc40f1983"
    );
}

#[test]
fn timeout_cleanup_failure_detail_includes_cleanup_command_and_reason() {
    let command = crate::services::installer::executor::PlannedCommand {
        program: "taskkill.exe".to_string(),
        args: vec![
            "/PID".to_string(),
            "1234".to_string(),
            "/T".to_string(),
            "/F".to_string(),
        ],
    };

    let detail = timeout_cleanup_failure_detail(&command, "exit code 128");

    assert!(detail.contains("taskkill.exe /PID 1234 /T /F"));
    assert!(detail.contains("exit code 128"));
}

#[test]
fn installer_command_output_is_redacted_and_limited_before_ui_logs() {
    let output = "Authorization: Bearer secret-token password hunter2 token npm-secret --api-key=cli-secret GITHUB_TOKEN=ghp_secret _authToken=npm-inline //registry.npmjs.org/:_authToken=npm-token https://user:pass@example.invalid/private\n".repeat(80);

    let sanitized = sanitize_installer_command_output(&output)
        .expect("non-empty diagnostic output should be retained");

    assert!(!sanitized.contains("secret-token"));
    assert!(!sanitized.contains("hunter2"));
    assert!(!sanitized.contains("npm-secret"));
    assert!(!sanitized.contains("cli-secret"));
    assert!(!sanitized.contains("ghp_secret"));
    assert!(!sanitized.contains("npm-inline"));
    assert!(!sanitized.contains("npm-token"));
    assert!(!sanitized.contains("user:pass"));
    assert!(sanitized.contains("[redacted]"));
    assert!(sanitized.len() <= 2051);
}

#[test]
fn plans_common_windowsapps_winget_fallback_paths() {
    let candidates = winget_candidate_paths();

    assert!(candidates
        .iter()
        .any(|path| path.ends_with(r"Microsoft\WindowsApps\winget.exe")));
}

#[test]
fn plans_msi_and_exe_install_commands_from_third_party_manifest_entries() {
    let msi_command = third_party_install_command(
        "nodejs",
        "node/node-v24.15.0-x64.msi",
        &["/qn".into(), "/norestart".into()],
        Path::new("C:/bundle/resources/third_party"),
    )
    .expect("msi command should build");
    let exe_command = third_party_install_command(
        "git",
        "git/Git-2.54.0-64-bit.exe",
        &["/VERYSILENT".into(), "/NORESTART".into()],
        Path::new("C:/bundle/resources/third_party"),
    )
    .expect("exe command should build");
    let python_command = third_party_install_command(
        "python",
        "python/python-3.12.10-amd64.exe",
        &[
            "/quiet".into(),
            "InstallAllUsers=1".into(),
            "PrependPath=1".into(),
            "Include_test=0".into(),
        ],
        Path::new("C:/bundle/resources/third_party"),
    )
    .expect("python command should build");

    assert!(msi_command
        .program
        .to_ascii_lowercase()
        .ends_with("msiexec.exe"));
    assert_eq!(
        msi_command.args,
        vec![
            "/i".to_string(),
            Path::new("C:/bundle/resources/third_party")
                .join("node/node-v24.15.0-x64.msi")
                .display()
                .to_string(),
            "/qn".to_string(),
            "/norestart".to_string()
        ]
    );
    assert_eq!(
        command_display(&msi_command),
        format!(
            "{} /i {} /qn /norestart",
            msi_command.program,
            Path::new("C:/bundle/resources/third_party")
                .join("node/node-v24.15.0-x64.msi")
                .display()
        )
    );

    assert_eq!(
        exe_command.program,
        Path::new("C:/bundle/resources/third_party")
            .join("git/Git-2.54.0-64-bit.exe")
            .display()
            .to_string()
    );
    assert_eq!(
        exe_command.args,
        vec!["/VERYSILENT".to_string(), "/NORESTART".to_string()]
    );
    assert_eq!(
        python_command.program,
        Path::new("C:/bundle/resources/third_party")
            .join("python/python-3.12.10-amd64.exe")
            .display()
            .to_string()
    );
    assert_eq!(
        python_command.args,
        vec![
            "/quiet".to_string(),
            "InstallAllUsers=1".to_string(),
            "PrependPath=1".to_string(),
            "Include_test=0".to_string()
        ]
    );
}

#[test]
fn strips_windows_extended_path_prefix_for_msi_installs() {
    let command = third_party_install_command(
        "nodejs",
        "node/node-v24.15.0-x64.msi",
        &["/qn".into(), "/norestart".into()],
        Path::new(r"\\?\C:\Users\HUAWEI\AppData\Local\Codex Deploy\resources\third_party"),
    )
    .expect("msi command should build from extended-length path");

    assert!(command
        .program
        .to_ascii_lowercase()
        .ends_with("msiexec.exe"));
    assert_eq!(
        command.args,
        vec![
            "/i".to_string(),
            r"C:\Users\HUAWEI\AppData\Local\Codex Deploy\resources\third_party\node\node-v24.15.0-x64.msi".to_string(),
            "/qn".to_string(),
            "/norestart".to_string()
        ]
    );
}

#[test]
fn rejects_third_party_manifest_paths_that_escape_resource_root() {
    let error = third_party_install_command(
        "git",
        "../evil.exe",
        &["/VERYSILENT".into()],
        Path::new("C:/bundle/resources/third_party"),
    )
    .expect_err("parent directory traversal should be rejected");

    assert_eq!(error.code, "installer_resource_path_invalid");
}

#[test]
fn stage_sequence_matches_expected_flow_ordering() {
    assert_eq!(
        stage_sequence("install_codex"),
        vec![
            crate::models::installer::InstallStageId::Preflight,
            crate::models::installer::InstallStageId::InstallGit,
            crate::models::installer::InstallStageId::InstallPython,
            crate::models::installer::InstallStageId::InstallNode,
            crate::models::installer::InstallStageId::InstallCcSwitch,
            crate::models::installer::InstallStageId::RefreshEnvironment,
            crate::models::installer::InstallStageId::InstallCodex,
            crate::models::installer::InstallStageId::Verify,
        ]
    );
    assert_eq!(
        stage_sequence("install_claude_code"),
        vec![
            crate::models::installer::InstallStageId::Preflight,
            crate::models::installer::InstallStageId::InstallGit,
            crate::models::installer::InstallStageId::InstallPython,
            crate::models::installer::InstallStageId::InstallNode,
            crate::models::installer::InstallStageId::InstallCcSwitch,
            crate::models::installer::InstallStageId::RefreshEnvironment,
            crate::models::installer::InstallStageId::InstallClaudeCode,
            crate::models::installer::InstallStageId::Verify,
        ]
    );
    assert_eq!(
        stage_sequence("install_all"),
        vec![
            crate::models::installer::InstallStageId::Preflight,
            crate::models::installer::InstallStageId::InstallGit,
            crate::models::installer::InstallStageId::InstallPython,
            crate::models::installer::InstallStageId::InstallNode,
            crate::models::installer::InstallStageId::InstallCcSwitch,
            crate::models::installer::InstallStageId::RefreshEnvironment,
            crate::models::installer::InstallStageId::InstallCodex,
            crate::models::installer::InstallStageId::InstallClaudeCode,
            crate::models::installer::InstallStageId::Verify,
        ]
    );
}

#[test]
fn stage_sequence_rejects_unknown_flow_instead_of_defaulting_to_codex() {
    assert!(stage_sequence("install_codez").is_empty());
}

fn restore_env_var(name: &str, value: Option<std::ffi::OsString>) {
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
}

#[test]
fn installer_service_returns_stage_names_for_requested_flow() {
    let service = InstallerService::production();

    assert_eq!(
        service
            .stage_sequence_for("install_codex")
            .expect("known flow should return stages"),
        vec![
            "Preflight".to_string(),
            "InstallGit".to_string(),
            "InstallPython".to_string(),
            "InstallNode".to_string(),
            "InstallCcSwitch".to_string(),
            "RefreshEnvironment".to_string(),
            "InstallCodex".to_string(),
            "Verify".to_string(),
        ]
    );
    assert_eq!(
        service
            .stage_sequence_for("install_claude_code")
            .expect("known flow should return stages"),
        vec![
            "Preflight".to_string(),
            "InstallGit".to_string(),
            "InstallPython".to_string(),
            "InstallNode".to_string(),
            "InstallCcSwitch".to_string(),
            "RefreshEnvironment".to_string(),
            "InstallClaudeCode".to_string(),
            "Verify".to_string(),
        ]
    );
    assert_eq!(
        service
            .stage_sequence_for("install_all")
            .expect("known flow should return stages"),
        vec![
            "Preflight".to_string(),
            "InstallGit".to_string(),
            "InstallPython".to_string(),
            "InstallNode".to_string(),
            "InstallCcSwitch".to_string(),
            "RefreshEnvironment".to_string(),
            "InstallCodex".to_string(),
            "InstallClaudeCode".to_string(),
            "Verify".to_string(),
        ]
    );
}

#[test]
fn installer_service_builds_snapshot_updates_for_requested_flow() {
    let service = InstallerService::production();
    let snapshots = service
        .snapshot_updates_for("install_codex")
        .expect("snapshot updates should build");

    assert_eq!(snapshots.len(), 9);
    assert_eq!(
        snapshots[0].current_stage,
        crate::models::installer::InstallStageId::Preflight
    );
    assert_eq!(snapshots[0].progress_percent, 11);
    assert_eq!(
        snapshots[6].current_stage,
        crate::models::installer::InstallStageId::InstallCodex
    );
    assert_eq!(snapshots[6].progress_percent, 77);
    assert_eq!(
        snapshots[8].current_stage,
        crate::models::installer::InstallStageId::Completed
    );
    assert_eq!(snapshots[8].progress_percent, 100);
    assert_eq!(snapshots[8].last_error, None);
}

#[test]
fn installer_service_builds_snapshot_updates_for_install_all_with_claude_code_last() {
    let service = InstallerService::production();
    let snapshots = service
        .snapshot_updates_for("install_all")
        .expect("snapshot updates should build");

    assert_eq!(snapshots.len(), 10);
    assert_eq!(
        snapshots[7].current_stage,
        crate::models::installer::InstallStageId::InstallClaudeCode
    );
    assert_eq!(snapshots[7].progress_percent, 80);
    assert_eq!(
        snapshots[8].current_stage,
        crate::models::installer::InstallStageId::Verify
    );
    assert_eq!(
        snapshots[9].current_stage,
        crate::models::installer::InstallStageId::Completed
    );
}

#[test]
fn installer_service_builds_snapshot_updates_for_install_claude_code_flow() {
    let service = InstallerService::production();
    let snapshots = service
        .snapshot_updates_for("install_claude_code")
        .expect("snapshot updates should build");

    assert_eq!(snapshots.len(), 9);
    assert_eq!(
        snapshots[6].current_stage,
        crate::models::installer::InstallStageId::InstallClaudeCode
    );
    assert_eq!(snapshots[6].progress_percent, 77);
    assert_eq!(
        snapshots[7].current_stage,
        crate::models::installer::InstallStageId::Verify
    );
    assert_eq!(
        snapshots[8].current_stage,
        crate::models::installer::InstallStageId::Completed
    );
}

#[test]
fn installer_service_retries_from_failed_stage() {
    let service = InstallerService::production();
    let snapshots = service
        .retry_snapshots_for_stage(crate::models::installer::InstallStageId::InstallCodex)
        .expect("retry snapshots should build");

    assert_eq!(snapshots.len(), 3);
    assert_eq!(
        snapshots[0].current_stage,
        crate::models::installer::InstallStageId::InstallCodex
    );
    assert_eq!(snapshots[0].progress_percent, 33);
    assert_eq!(
        snapshots[1].current_stage,
        crate::models::installer::InstallStageId::Verify
    );
    assert_eq!(
        snapshots[2].current_stage,
        crate::models::installer::InstallStageId::Completed
    );
    assert_eq!(snapshots[2].progress_percent, 100);
}

#[test]
fn installer_service_retries_from_claude_code_stage_with_verify_afterwards() {
    let service = InstallerService::production();
    let snapshots = service
        .retry_snapshots_for_stage(crate::models::installer::InstallStageId::InstallClaudeCode)
        .expect("retry snapshots should build");

    assert_eq!(snapshots.len(), 3);
    assert_eq!(
        snapshots[0].current_stage,
        crate::models::installer::InstallStageId::InstallClaudeCode
    );
    assert_eq!(
        snapshots[1].current_stage,
        crate::models::installer::InstallStageId::Verify
    );
    assert_eq!(
        snapshots[2].current_stage,
        crate::models::installer::InstallStageId::Completed
    );
}

#[test]
fn installer_session_state_tracks_failed_stage_and_flow_for_retry() {
    let mut state = InstallerSessionState::default();

    state.record_failure(
        "install_all",
        crate::models::installer::InstallStageId::InstallCodex,
    );

    assert_eq!(state.last_flow.as_deref(), Some("install_all"));
    assert_eq!(
        state.failed_stage,
        Some(crate::models::installer::InstallStageId::InstallCodex)
    );
}

#[test]
fn installer_service_rejects_concurrent_flow_reservations() {
    let service = InstallerService::production();
    let first = service
        .reserve_flow()
        .expect("first flow reservation should succeed");
    let second = service.reserve_flow();

    assert_eq!(
        second.expect_err("second reservation should fail").code,
        "installer_flow_already_running"
    );
    assert!(service.refresh_snapshot().is_ok());

    drop(first);
    assert!(service.reserve_flow().is_ok());
}

#[test]
fn installed_component_is_marked_skipped_for_install_stage() {
    let mut snapshot = build_initial_snapshot(&FakeProbe);
    let skipped = mark_component_skipped_if_installed(
        &mut snapshot,
        "git",
        crate::models::installer::InstallStageId::InstallGit,
    );
    let git = snapshot
        .components
        .iter()
        .find(|component| component.id == "git")
        .expect("git component should exist");

    assert!(skipped);
    assert_eq!(git.status, "skipped");
    assert_eq!(git.version.as_deref(), Some("2.45.1"));
    assert!(git.detail.contains("跳过本阶段"));
    assert_eq!(snapshot.logs.len(), 1);
    assert!(snapshot.logs[0].message.contains("跳过安装"));
}

#[test]
fn missing_component_is_not_marked_skipped_for_install_stage() {
    let mut snapshot = build_initial_snapshot(&FakeProbe);
    let skipped = mark_component_skipped_if_installed(
        &mut snapshot,
        "cc_switch",
        crate::models::installer::InstallStageId::InstallCcSwitch,
    );
    let cc_switch = snapshot
        .components
        .iter()
        .find(|component| component.id == "cc_switch")
        .expect("cc switch component should exist");

    assert!(!skipped);
    assert_eq!(cc_switch.status, "not_installed");
    assert!(snapshot.logs.is_empty());
}

#[test]
fn component_status_reports_current_install_state_without_refresh_side_effects() {
    let mut snapshot = build_initial_snapshot(&FakeProbe);
    let codex = snapshot
        .components
        .iter_mut()
        .find(|component| component.id == "codex")
        .expect("codex component should exist");

    codex.status = crate::models::installer::InstallerComponentStatus::Installing;
    codex.detail = "Codex 安装命令已完成，等待最终校验".into();

    assert_eq!(
        component_status(&snapshot.components, "codex"),
        Some(crate::models::installer::InstallerComponentStatus::Installing)
    );
}

#[test]
fn execution_environment_reuses_refreshed_path_within_single_probe() {
    let _guard = ENV_LOCK.lock().expect("env lock should not be poisoned");

    let probe = DetectExecutionEnvironment::new();
    let _ = probe.detect_binary("definitely-missing-ai-dev-installer-command");
    let _ = probe.detect_binary("definitely-missing-ai-dev-installer-command-again");

    assert_eq!(probe.refreshed_path_env_build_count_for_tests(), 1);
}
