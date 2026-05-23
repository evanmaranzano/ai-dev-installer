$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot

Push-Location $repoRoot
try {
    npm run release:check
    $env:CARGO_TARGET_DIR = Join-Path $repoRoot "src-tauri/target-release"
    npm run tauri -- build
}
finally {
    Pop-Location
}
