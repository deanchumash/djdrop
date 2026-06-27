<#
.SYNOPSIS
Downloads external CLI tools into src-tauri/binaries/ for bundling with the Tauri app.
Run this before cargo tauri build.
#>
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$binDir = "$root\src-tauri\binaries"
New-Item -ItemType Directory -Force -Path $binDir | Out-Null

function Get-LatestGitHubAsset {
    param([string]$repo, [string]$pattern)
    $release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
    $asset = $release.assets | Where-Object { $_.name -like $pattern } | Select-Object -First 1
    if (-not $asset) { throw "No asset matching '$pattern' found in $repo latest release" }
    return $asset.browser_download_url
}

# yt-dlp standalone exe
Write-Host "[1/3] Downloading yt-dlp..."
$ytdlpUrl = Get-LatestGitHubAsset -repo "yt-dlp/yt-dlp" -pattern "yt-dlp.exe"
Invoke-WebRequest -Uri $ytdlpUrl -OutFile "$binDir\yt-dlp.exe" -UseBasicParsing
Write-Host "      yt-dlp: $([math]::Round((Get-Item "$binDir\yt-dlp.exe").Length / 1MB))MB"

# ffmpeg essentials build (includes libmp3lame)
Write-Host "[2/3] Downloading ffmpeg (gyan.dev essentials)..."
$ffmpegZip = "$env:TEMP\ffmpeg-essentials-djdrop.zip"
Invoke-WebRequest -Uri "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip" -OutFile $ffmpegZip -UseBasicParsing
$extractDir = "$env:TEMP\ffmpeg-essentials-djdrop"
if (Test-Path $extractDir) { Remove-Item $extractDir -Recurse -Force }
Expand-Archive -Path $ffmpegZip -DestinationPath $extractDir -Force
$ffmpegExe = Get-ChildItem $extractDir -Recurse -Filter "ffmpeg.exe" |
    Where-Object { $_.DirectoryName -like "*\bin" } |
    Select-Object -First 1
Copy-Item $ffmpegExe.FullName "$binDir\ffmpeg.exe"
Remove-Item $extractDir -Recurse -Force
Remove-Item $ffmpegZip -Force
Write-Host "      ffmpeg: $([math]::Round((Get-Item "$binDir\ffmpeg.exe").Length / 1MB))MB"

# spotdl standalone exe
Write-Host "[3/3] Downloading spotdl..."
$spotdlUrl = Get-LatestGitHubAsset -repo "spotDL/spotify-downloader" -pattern "spotdl-*-win32.exe"
Invoke-WebRequest -Uri $spotdlUrl -OutFile "$binDir\spotdl.exe" -UseBasicParsing
Write-Host "      spotdl: $([math]::Round((Get-Item "$binDir\spotdl.exe").Length / 1MB))MB"

Write-Host ""
Write-Host "NOTE: keyfinder-cli has no prebuilt Windows binary."
Write-Host "Key detection will be silently skipped unless you manually place"
Write-Host "a compiled keyfinder-cli.exe at:"
Write-Host "  $binDir\keyfinder-cli.exe"
Write-Host "Build from source: https://github.com/EvanPurkhiser/keyfinder-cli"
Write-Host ""
Write-Host "Done. Binaries written to: $binDir"
