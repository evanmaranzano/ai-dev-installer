param(
    [switch]$RequireLicenses
)

$ErrorActionPreference = "Stop"

$repoRoot = Split-Path -Parent $PSScriptRoot
$resourceRoot = Join-Path $repoRoot "src-tauri/resources/third_party"
$manifestPath = Join-Path $resourceRoot "manifest.json"
$licenseRoot = Join-Path $resourceRoot "LICENSES"
$failures = New-Object System.Collections.Generic.List[string]

function Add-Failure([string]$message) {
    $failures.Add($message) | Out-Null
}

function Test-SafeRelativePath([string]$path) {
    if ([string]::IsNullOrWhiteSpace($path)) {
        return $false
    }

    if ([System.IO.Path]::IsPathRooted($path)) {
        return $false
    }

    $parts = $path -split '[\\/]+' | Where-Object { $_ -ne "" }
    if ($parts.Count -eq 0) {
        return $false
    }

    foreach ($part in $parts) {
        if ($part -eq "." -or $part -eq "..") {
            return $false
        }
    }

    return $true
}

function Get-Sha256Hex([string]$path) {
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
        $bytes = [System.IO.File]::ReadAllBytes($path)
        $hash = $sha256.ComputeHash($bytes)
        return [System.BitConverter]::ToString($hash).Replace("-", "").ToLowerInvariant()
    }
    finally {
        $sha256.Dispose()
    }
}

if (-not (Test-Path -LiteralPath $manifestPath -PathType Leaf)) {
    throw "Missing third-party manifest: $manifestPath"
}

$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
if (-not $manifest.resources) {
    throw "Manifest has no resources: $manifestPath"
}

foreach ($resource in $manifest.resources) {
    if (-not (Test-SafeRelativePath $resource.file_name)) {
        Add-Failure "Invalid resource path for $($resource.component_id): $($resource.file_name)"
        continue
    }

    $payloadPath = Join-Path $resourceRoot $resource.file_name
    if (-not (Test-Path -LiteralPath $payloadPath -PathType Leaf)) {
        Add-Failure "Missing payload for $($resource.component_id): $payloadPath"
        continue
    }

    $actualHash = Get-Sha256Hex $payloadPath
    $expectedHash = [string]$resource.sha256
    if ($actualHash -ne $expectedHash.ToLowerInvariant()) {
        Add-Failure "SHA256 mismatch for $($resource.component_id): expected $expectedHash, got $actualHash"
    }

    if ($RequireLicenses) {
        $licensePath = Join-Path $licenseRoot "$($resource.component_id).txt"
        if (-not (Test-Path -LiteralPath $licensePath -PathType Leaf)) {
            Add-Failure "Missing license file for $($resource.component_id): $licensePath"
        }
    }
}

if ($failures.Count -gt 0) {
    $failures | ForEach-Object { Write-Error $_ }
    exit 1
}

Write-Host "Verified $($manifest.resources.Count) third-party payload(s)."
