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
    $headers = @{}
    if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
    $release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest" -Headers $headers
    $asset = $release.assets | Where-Object { $_.name -like $pattern } | Select-Object -First 1
    if (-not $asset) { throw "No asset matching '$pattern' found in $repo latest release" }
    return $asset.browser_download_url
}

# yt-dlp standalone exe
Write-Host "[1/5] Downloading yt-dlp..."
$ytdlpUrl = Get-LatestGitHubAsset -repo "yt-dlp/yt-dlp" -pattern "yt-dlp.exe"
Invoke-WebRequest -Uri $ytdlpUrl -OutFile "$binDir\yt-dlp.exe" -UseBasicParsing
Write-Host "      yt-dlp: $([math]::Round((Get-Item "$binDir\yt-dlp.exe").Length / 1MB))MB"

# ffmpeg build (includes libmp3lame); use GitHub's build if gyan.dev is unavailable.
Write-Host "[2/5] Downloading ffmpeg..."
$ffmpegZip = "$env:TEMP\ffmpeg-essentials-djdrop.zip"
try {
    Invoke-WebRequest -Uri "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip" -OutFile $ffmpegZip -UseBasicParsing
} catch {
    Write-Warning "gyan.dev ffmpeg download failed; using BtbN's Windows GPL build instead."
    $ffmpegUrl = Get-LatestGitHubAsset -repo "BtbN/FFmpeg-Builds" -pattern "ffmpeg-master-latest-win64-gpl.zip"
    Invoke-WebRequest -Uri $ffmpegUrl -OutFile $ffmpegZip -UseBasicParsing
}
$extractDir = "$env:TEMP\ffmpeg-essentials-djdrop"
if (Test-Path $extractDir) { Remove-Item $extractDir -Recurse -Force }
Expand-Archive -Path $ffmpegZip -DestinationPath $extractDir -Force
$ffmpegExe = Get-ChildItem $extractDir -Recurse -Filter "ffmpeg.exe" |
    Where-Object { $_.DirectoryName -like "*\bin" } |
    Select-Object -First 1
Copy-Item $ffmpegExe.FullName "$binDir\ffmpeg.exe"
Copy-Item (Join-Path $ffmpegExe.DirectoryName "ffprobe.exe") "$binDir\ffprobe.exe"
Remove-Item $extractDir -Recurse -Force
Remove-Item $ffmpegZip -Force
Write-Host "      ffmpeg + ffprobe: $([math]::Round((Get-Item "$binDir\ffmpeg.exe").Length / 1MB))MB"

# Upstream's Windows exe freezes an old yt-dlp. Build the latest spotDL
# release with a current yt-dlp so Spotify audio survives YouTube changes.
Write-Host "[3/5] Building spotdl with current yt-dlp..."
$headers = @{}
if ($env:GITHUB_TOKEN) { $headers['Authorization'] = "Bearer $env:GITHUB_TOKEN" }
$release = Invoke-RestMethod "https://api.github.com/repos/spotDL/spotify-downloader/releases/latest" -Headers $headers
$version = $release.tag_name.TrimStart('v')
$buildDir = "$env:TEMP\djdrop-spotdl-build"
if (Test-Path $buildDir) { Remove-Item $buildDir -Recurse -Force }
git clone --quiet --depth 1 --branch $release.tag_name "https://github.com/spotDL/spotify-downloader.git" $buildDir
if ($LASTEXITCODE -ne 0) { throw "Failed to fetch spotdl $version source" }
python -m venv "$buildDir\.venv"
if ($LASTEXITCODE -ne 0) { throw "Failed to create spotdl build environment" }
$python = "$buildDir\.venv\Scripts\python.exe"
& $python -m pip install --quiet "spotdl==$version" "yt-dlp[default]" "pyinstaller>=6.11.1,<7"
if ($LASTEXITCODE -ne 0) { throw "Failed to install spotdl build dependencies" }
Push-Location $buildDir
try {
    & $python scripts/build.py
    if ($LASTEXITCODE -ne 0) { throw "Failed to build spotdl" }
    Copy-Item "dist\spotdl-$version-win32.exe" "$binDir\spotdl.exe"
} finally {
    Pop-Location
    Remove-Item $buildDir -Recurse -Force
}
Write-Host "      spotdl: $([math]::Round((Get-Item "$binDir\spotdl.exe").Length / 1MB))MB"

# yt-dlp (and spotdl's internal yt-dlp) need a JS runtime for YouTube.
Write-Host "[4/5] Downloading Deno..."
$denoUrl = Get-LatestGitHubAsset -repo "denoland/deno" -pattern "deno-x86_64-pc-windows-msvc.zip"
$denoZip = "$env:TEMP\deno-djdrop.zip"
Invoke-WebRequest -Uri $denoUrl -OutFile $denoZip -UseBasicParsing
Expand-Archive -Path $denoZip -DestinationPath $binDir -Force
Remove-Item $denoZip -Force
Write-Host "      deno: $([math]::Round((Get-Item "$binDir\deno.exe").Length / 1MB))MB"

# slskd: headless Soulseek client that djdrop drives over a loopback REST API.
Write-Host "[5/5] Downloading slskd..."
$slskdUrl = Get-LatestGitHubAsset -repo "slskd/slskd" -pattern "slskd-*-win-x64.zip"
$slskdZip = "$env:TEMP\slskd-djdrop.zip"
Invoke-WebRequest -Uri $slskdUrl -OutFile $slskdZip -UseBasicParsing
$slskdDir = "$env:TEMP\slskd-djdrop"
if (Test-Path $slskdDir) { Remove-Item $slskdDir -Recurse -Force }
Expand-Archive -Path $slskdZip -DestinationPath $slskdDir -Force
Copy-Item "$slskdDir\slskd.exe" "$binDir\slskd.exe"
Remove-Item $slskdDir -Recurse -Force
Remove-Item $slskdZip -Force
Write-Host "      slskd: $([math]::Round((Get-Item "$binDir\slskd.exe").Length / 1MB))MB"

Write-Host ""
Write-Host "NOTE: keyfinder-cli has no prebuilt Windows binary."
Write-Host "Key detection will be silently skipped unless you manually place"
Write-Host "a compiled keyfinder-cli.exe at:"
Write-Host "  $binDir\keyfinder-cli.exe"
Write-Host "Build from source: https://github.com/EvanPurkhiser/keyfinder-cli"
Write-Host ""
Write-Host "Done. Binaries written to: $binDir"
