<#
.SYNOPSIS
Full build pipeline for djdrop. Fetches external binaries then runs cargo tauri build.
Outputs the .msi installer path at the end.

Usage:
  .\scripts\build.ps1           # fetch + build
  .\scripts\build.ps1 -SkipFetch  # build only (binaries already downloaded)
#>
param([switch]$SkipFetch)

$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
Set-Location $root

if (-not $SkipFetch) {
    Write-Host "=== Fetching binaries ==="
    & "$PSScriptRoot\fetch-binaries.ps1"
    Write-Host ""
}

# Verify required binaries are present
$required = @("yt-dlp.exe", "ffmpeg.exe", "spotdl.exe")
foreach ($bin in $required) {
    $path = "src-tauri\binaries\$bin"
    if (-not (Test-Path $path)) {
        Write-Error "Missing required binary: $path`nRun .\scripts\fetch-binaries.ps1 first."
        exit 1
    }
}

Write-Host "=== Building djdrop ==="
cargo tauri build

$msi = Get-ChildItem "src-tauri\target\release\bundle\msi\*.msi" |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

if ($msi) {
    Write-Host ""
    Write-Host "=== Build complete ==="
    Write-Host "Installer: $($msi.FullName)"
    Write-Host ""
    Write-Host "To install on this machine: double-click the .msi"
    Write-Host "To install on your laptop:  copy the .msi file and double-click it there"
} else {
    Write-Error "Build succeeded but no .msi found. Check cargo tauri build output."
}
