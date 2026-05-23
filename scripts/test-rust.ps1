$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$env:CARGO_TARGET_DIR = Join-Path $repoRoot "src-tauri/target-audit"

cargo test --manifest-path (Join-Path $repoRoot "src-tauri/Cargo.toml")
