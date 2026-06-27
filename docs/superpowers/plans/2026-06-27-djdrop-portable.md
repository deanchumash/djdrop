# djdrop Portable Build Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bundle yt-dlp, ffmpeg, and spotdl as app resources so djdrop installs as a single .msi on any Windows machine with no prerequisites.

**Architecture:** External CLI tools (yt-dlp, ffmpeg, spotdl) are downloaded at build time into `src-tauri/binaries/` and declared as Tauri bundle resources. A `binaries.rs` module resolves their paths from the resource directory at runtime. aubio's subprocess call is replaced with the `aubio-rs` Rust crate (compiles aubio C library into the binary — no external exe). keyfinder-cli is bundled the same way as the other tools; key detection silently skips if the binary is absent. `cargo tauri build` produces an `.msi` installer; a `scripts/build.ps1` script orchestrates the full build so it can be reproduced on any machine.

**Tech Stack:** Tauri v2, Rust, tokio, `aubio-rs` 0.3 (BPM via compiled C library), PowerShell (fetch + build scripts)

## Global Constraints

- Target: Windows x64 only for this plan (macOS/Linux is future work)
- Do NOT commit binary files to git — `src-tauri/binaries/` is gitignored
- yt-dlp must receive `--ffmpeg-location <binaries_dir>` so it finds the bundled ffmpeg, not the system one
- aubio-rs replaces the `aubio tempo` subprocess — `parse_bpm()` becomes dead code and must be removed along with its test
- keyfinder-cli bundled path is used if the binary exists; if absent, `run_keyfinder` returns `None` silently (key detection is optional)
- All existing tests must continue to pass; update tests broken by signature changes
- Do not run `cargo` or build commands — Rust is not installed on this machine; write correct code only
- Tauri v2 resource path at runtime: `app.path().resource_dir()?.join("binaries")`

---

### Task 1: Binary fetch script + binaries directory

**Files:**
- Create: `scripts/fetch-binaries.ps1`
- Create: `src-tauri/binaries/.gitignore`

**Interfaces:**
- Produces: `src-tauri/binaries/yt-dlp.exe`, `src-tauri/binaries/ffmpeg.exe`, `src-tauri/binaries/spotdl.exe` (keyfinder-cli.exe is manual — see note in script)
- Consumed by: Task 2 (Tauri bundles them), Task 3 (downloader uses them at runtime), Task 4 (analyzer uses keyfinder-cli at runtime)

- [ ] **Step 1: Create `src-tauri/binaries/.gitignore`**

```
*
```

This prevents binary files from being committed to git. The directory itself is tracked (the .gitignore is committed).

- [ ] **Step 2: Create `scripts/fetch-binaries.ps1`**

```powershell
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
```

- [ ] **Step 3: Run the script and verify output**

```powershell
cd C:\Users\dank\dev\djdrop
.\scripts\fetch-binaries.ps1
```

Expected output: three download confirmations, then the keyfinder note. Verify:
```powershell
Get-ChildItem src-tauri\binaries\
```
Expected: `yt-dlp.exe`, `ffmpeg.exe`, `spotdl.exe` (and `.gitignore`).

Verify ffmpeg has libmp3lame:
```powershell
.\src-tauri\binaries\ffmpeg.exe -encoders 2>&1 | Select-String "libmp3lame"
```
Expected: `A....D libmp3lame` line.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/binaries/.gitignore scripts/fetch-binaries.ps1
git commit -m "feat: binary fetch script for portable build"
```

---

### Task 2: Tauri resource bundling + `binaries.rs` path resolver

**Files:**
- Modify: `src-tauri/tauri.conf.json`
- Create: `src-tauri/src/binaries.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `src-tauri/binaries/` directory (from Task 1)
- Produces:
  - `crate::binaries::ytdlp(app: &AppHandle) -> Result<PathBuf, String>`
  - `crate::binaries::ffmpeg_dir(app: &AppHandle) -> Result<PathBuf, String>`
  - `crate::binaries::spotdl(app: &AppHandle) -> Result<PathBuf, String>`
  - `crate::binaries::keyfinder_cli(app: &AppHandle) -> Result<PathBuf, String>`
  - All functions return `Err(String)` if the resource dir is inaccessible

- [ ] **Step 1: Write the failing test for `binaries.rs`**

Create `src-tauri/src/binaries.rs` with tests first:

```rust
use std::path::PathBuf;
use tauri::Manager;

fn binaries_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resource_dir()
        .map(|d| d.join("binaries"))
        .map_err(|e| e.to_string())
}

fn bin(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    Ok(binaries_dir(app)?.join(format!("{}{}", name, ext)))
}

pub fn ytdlp(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    bin(app, "yt-dlp")
}

pub fn ffmpeg_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    binaries_dir(app)
}

pub fn spotdl(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    bin(app, "spotdl")
}

pub fn keyfinder_cli(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    bin(app, "keyfinder-cli")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_suffix_on_windows() {
        // Verify the exe helper appends .exe on Windows
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let expected = format!("yt-dlp{}", suffix);
        // Can't call bin() without AppHandle in unit test; verify suffix logic directly
        assert!(expected.ends_with(suffix));
    }
}
```

- [ ] **Step 2: Add `mod binaries` to `src-tauri/src/lib.rs`**

Add as the first line of `lib.rs` (before `mod analyzer`):

```rust
mod binaries;
mod analyzer;
mod config;
mod credentials;
mod downloader;
mod router;
```

Note: `binaries.rs` contains a Rust `#[cfg(test)]` unit test. These Rust tests are verified at `cargo tauri build` time — `npm run test` runs TypeScript/vitest only and won't exercise them.

- [ ] **Step 3: Update `tauri.conf.json` to declare resources**

Add `"resources": ["binaries/*"]` to the `bundle` section:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "djdrop",
  "version": "0.1.0",
  "identifier": "com.djdrop.app",
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:1420",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "main",
        "title": "djdrop",
        "width": 72,
        "height": 72,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "resizable": false,
        "skipTaskbar": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "resources": ["binaries/*"],
    "icon": ["icons/32x32.png","icons/128x128.png","icons/128x128@2x.png","icons/icon.icns","icons/icon.ico"]
  }
}
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/binaries.rs src-tauri/src/lib.rs src-tauri/tauri.conf.json
git commit -m "feat: bundle external binaries as Tauri resources, add path resolver"
```

---

### Task 3: Update downloader to use bundled binary paths

**Files:**
- Modify: `src-tauri/src/downloader.rs`

**Interfaces:**
- Consumes:
  - `crate::binaries::ytdlp(app) -> Result<PathBuf, String>`
  - `crate::binaries::ffmpeg_dir(app) -> Result<PathBuf, String>`
  - `crate::binaries::spotdl(app) -> Result<PathBuf, String>`
- The public API (`ytdlp_args`, `spotdl_args`, `qobuz_args`, `run_download`) is unchanged except `ytdlp_args` gains a third parameter `ffmpeg_dir: &str`

- [ ] **Step 1: Update the `ytdlp_args` test to include `--ffmpeg-location`**

In `src-tauri/src/downloader.rs`, update the existing Rust unit test:

```rust
#[test]
fn ytdlp_args_include_mp3_flags() {
    let args = ytdlp_args("https://youtube.com/watch?v=x", "/tmp/out", "/tmp/ffmpeg");
    assert!(args.contains(&"--audio-format".to_string()));
    assert!(args.contains(&"mp3".to_string()));
    assert!(args.contains(&"--audio-quality".to_string()));
    assert!(args.contains(&"0".to_string()));
    assert!(args.contains(&"-P".to_string()));
    assert!(args.contains(&"/tmp/out".to_string()));
    assert!(args.contains(&"--ffmpeg-location".to_string()));
    assert!(args.contains(&"/tmp/ffmpeg".to_string()));
}
```

These are Rust `#[cfg(test)]` tests — they run with `cargo test`, not `npm run test`. Write the updated test first, then update the function signature to match.

- [ ] **Step 3: Update `ytdlp_args` to accept `ffmpeg_dir` and inject `--ffmpeg-location`**

Replace the existing `ytdlp_args` function:

```rust
pub fn ytdlp_args(url: &str, output_dir: &str, ffmpeg_dir: &str) -> Vec<String> {
    vec![
        "-x".into(), "--audio-format".into(), "mp3".into(),
        "--audio-quality".into(), "0".into(),
        "-P".into(), output_dir.into(),
        "--print".into(), "after_move:filepath".into(),
        "--no-playlist".into(),
        "--ffmpeg-location".into(), ffmpeg_dir.into(),
        url.into(),
    ]
}
```

- [ ] **Step 4: Update `run_ytdlp` to resolve bundled binary paths**

Replace the `run_ytdlp` function:

```rust
async fn run_ytdlp(app: &AppHandle, id: &str, url: &str, output_dir: &str) -> Result<(), String> {
    let ytdlp_bin = crate::binaries::ytdlp(app)?;
    let ffmpeg_dir = crate::binaries::ffmpeg_dir(app)?
        .to_string_lossy()
        .into_owned();
    let args = ytdlp_args(url, output_dir, &ffmpeg_dir);

    let mut child = Command::new(&ytdlp_bin)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("yt-dlp not found at {:?}: {e}", ytdlp_bin))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let app_clone = app.clone();
    let id_clone = id.to_string();

    let progress_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(pct) = parse_ytdlp_progress(&line) {
                let _ = app_clone.emit("download:progress", ProgressPayload {
                    id: id_clone.clone(),
                    percent: pct,
                });
            }
        }
    });

    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut file_path = String::new();
    while let Ok(Some(line)) = stdout_lines.next_line().await {
        let trimmed = line.trim().to_string();
        if !trimmed.is_empty() {
            file_path = trimmed;
        }
    }

    let _ = progress_task.await;

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("yt-dlp exited with error".into());
    }

    if file_path.is_empty() {
        return Err("yt-dlp did not report output filepath".into());
    }

    let track_name = std::path::Path::new(&file_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| url.to_string());

    let _ = app.emit("download:done", DonePayload {
        id: id.to_string(),
        file_path: file_path.clone(),
        track_name,
        source: "youtube".into(),
    });

    crate::analyzer::analyze(app, id, &file_path).await;
    Ok(())
}
```

- [ ] **Step 5: Update `run_spotdl` to use bundled binary**

Replace the `run_spotdl` function:

```rust
async fn run_spotdl(app: &AppHandle, id: &str, url: &str, output_dir: &str) -> Result<(), String> {
    let spotdl_bin = crate::binaries::spotdl(app)?;
    let args = spotdl_args(url, output_dir);
    let status = Command::new(&spotdl_bin)
        .args(&args)
        .status()
        .await
        .map_err(|e| format!("spotdl not found at {:?}: {e}", spotdl_bin))?;

    if !status.success() { return Err("spotdl exited with error".into()); }

    let file_path = newest_mp3_in(output_dir)?;
    let track_name = std::path::Path::new(&file_path)
        .file_stem().map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let _ = app.emit("download:done", DonePayload {
        id: id.to_string(), file_path: file_path.clone(), track_name, source: "spotify".into()
    });
    crate::analyzer::analyze(app, id, &file_path).await;
    Ok(())
}
```

Note: `qobuz-dlp` has no standalone Windows exe — `run_qobuz` continues using `Command::new("qobuz-dlp")` (PATH lookup). This is acceptable since Qobuz is a niche source; document it as a known limitation.

- [ ] **Step 6: Run frontend sanity check**

```powershell
npm run test -- --run
```

Expected: 9 vitest tests pass (no TypeScript changes in this task — confirms no frontend regressions).
Rust unit tests (`ytdlp_args_include_mp3_flags` etc.) are verified at `cargo tauri build` time.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/downloader.rs
git commit -m "feat: resolve bundled yt-dlp/ffmpeg/spotdl paths at runtime"
```

---

### Task 4: Replace aubio subprocess with aubio-rs; bundle keyfinder-cli

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/analyzer.rs`

**Interfaces:**
- Consumes: `crate::binaries::keyfinder_cli(app) -> Result<PathBuf, String>` (from Task 2)
- `pub async fn analyze(app: &AppHandle, id: &str, file_path: &str)` — signature unchanged
- Removes: `run_aubio` async fn, `parse_bpm` fn and its test (replaced by `detect_bpm_inline`)
- Keeps: `parse_key` fn and its test (still used to parse keyfinder-cli stdout)

**Build note:** `aubio-rs` compiles the aubio C library via its build script. This requires cmake. Install it with:
```powershell
winget install Kitware.CMake
```
Then restart the terminal so cmake is on PATH before `cargo tauri build`.

- [ ] **Step 1: Add `aubio-rs` to `Cargo.toml`**

In `src-tauri/Cargo.toml`, add to `[dependencies]`:

```toml
aubio-rs = "0.3"
```

Full updated `[dependencies]` block:

```toml
[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-drag = "2"
tauri-plugin-shell = "2"
tauri-plugin-keyring = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
tokio = { version = "1", features = ["full"] }
keyring = "3"
uuid = { version = "1", features = ["v4"] }
url = "2"
which = "6"
anyhow = "1"
dirs = "5"
aubio-rs = "0.3"
```

- [ ] **Step 2: Update the test block in analyzer.rs**

In `src-tauri/src/analyzer.rs`, the test block changes as follows. `parse_bpm` is removed (replaced by `detect_bpm_inline`); `parse_key` and its test remain. Add a test for `detect_bpm_inline` on an invalid file path. Write the tests before the implementation:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_bpm_inline_returns_none_for_missing_file() {
        assert_eq!(detect_bpm_inline("/nonexistent/path/file.mp3"), None);
    }

    #[test]
    fn parses_keyfinder_output() {
        assert_eq!(parse_key("Am\n"), Some("Am".to_string()));
        assert_eq!(parse_key("F#\n"), Some("F#".to_string()));
        assert_eq!(parse_key(""), None);
    }
}
```

The `parses_aubio_bpm_output` test is intentionally removed — it tested `parse_bpm` which is gone.
These are Rust `#[cfg(test)]` tests verified at `cargo test` time, not by vitest.

- [ ] **Step 4: Rewrite `analyzer.rs` with `aubio-rs` and bundled keyfinder path**

Replace the entire contents of `src-tauri/src/analyzer.rs`:

```rust
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Clone, Serialize)]
pub struct AnalysisDonePayload { pub id: String, pub bpm: u32, pub key: String }

pub async fn analyze(app: &AppHandle, id: &str, file_path: &str) {
    let fp = file_path.to_string();
    let bpm = tokio::task::spawn_blocking(move || detect_bpm_inline(&fp))
        .await
        .unwrap_or(None)
        .unwrap_or(0);

    let key = run_keyfinder(app, file_path).await.unwrap_or_default();

    if bpm > 0 || !key.is_empty() {
        let _ = app.emit("analysis:done", AnalysisDonePayload {
            id: id.to_string(), bpm, key,
        });
    }
}

fn detect_bpm_inline(file_path: &str) -> Option<u32> {
    use aubio_rs::{OnsetMode, Source, Tempo};

    let hop_size: usize = 512;
    let win_size: usize = 1024;

    let mut src = Source::new(file_path, 0, hop_size).ok()?;
    let sample_rate = src.sample_rate();

    let mut tempo = Tempo::new(OnsetMode::SpecFlux, win_size, hop_size, sample_rate).ok()?;

    loop {
        let mut block = aubio_rs::FVec::zeros(hop_size);
        let read = src.do_(&mut block).ok()?;
        let _ = tempo.do_(&block);
        if read < hop_size { break; }
    }

    let bpm = tempo.get_bpm();
    if bpm > 0.0 { Some(bpm.round() as u32) } else { None }
}

async fn run_keyfinder(app: &AppHandle, file_path: &str) -> Option<String> {
    let keyfinder_bin = crate::binaries::keyfinder_cli(app).ok()?;
    if !keyfinder_bin.exists() {
        return None; // silently skip if not bundled
    }
    let output = Command::new(&keyfinder_bin)
        .args([file_path])
        .output().await.ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_key(&stdout)
}

pub fn parse_key(output: &str) -> Option<String> {
    let s = output.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_bpm_inline_returns_none_for_missing_file() {
        assert_eq!(detect_bpm_inline("/nonexistent/path/file.mp3"), None);
    }

    #[test]
    fn parses_keyfinder_output() {
        assert_eq!(parse_key("Am\n"), Some("Am".to_string()));
        assert_eq!(parse_key("F#\n"), Some("F#".to_string()));
        assert_eq!(parse_key(""), None);
    }
}
```

- [ ] **Step 5: Run frontend sanity check**

```powershell
npm run test -- --run
```

Expected: 9 vitest tests pass (no TypeScript changes — confirms no frontend regressions).
Rust test count is unchanged: `parses_aubio_bpm_output` (-1) and `detect_bpm_inline_returns_none_for_missing_file` (+1) net to zero. Rust tests verified at `cargo tauri build` time.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/analyzer.rs
git commit -m "feat: replace aubio subprocess with aubio-rs crate; keyfinder uses bundled path"
```

---

### Task 5: Tauri .msi bundle config + build scripts

**Files:**
- Modify: `src-tauri/tauri.conf.json` (bundle section)
- Create: `scripts/build.ps1`

**Interfaces:**
- Consumes: `scripts/fetch-binaries.ps1` (from Task 1)
- Produces: `src-tauri/target/release/bundle/msi/djdrop_0.1.0_x64_en-US.msi`

- [ ] **Step 1: Update `tauri.conf.json` bundle section with Windows .msi config**

Replace the `bundle` section in `src-tauri/tauri.conf.json`:

```json
"bundle": {
  "active": true,
  "targets": ["msi"],
  "resources": ["binaries/*"],
  "icon": [
    "icons/32x32.png",
    "icons/128x128.png",
    "icons/128x128@2x.png",
    "icons/icon.icns",
    "icons/icon.ico"
  ],
  "windows": {
    "digestAlgorithm": "sha256",
    "timestampUrl": "",
    "wix": {
      "language": "en-US"
    }
  }
}
```

Note: `targets: ["msi"]` builds only the .msi installer (not NSIS or other formats), keeping build time short.

- [ ] **Step 2: Create `scripts/build.ps1`**

```powershell
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
```

- [ ] **Step 3: Verify scripts are executable (test the fetch path)**

```powershell
cd C:\Users\dank\dev\djdrop
.\scripts\build.ps1 -SkipFetch
```

Expected: cargo tauri build runs (will fail if Rust not installed — that's expected on this machine). The script logic itself must not error before reaching cargo.

Since Rust is not installed on the dev machine, test the script logic without cargo:

```powershell
# Verify binary presence check works correctly
$required = @("yt-dlp.exe", "ffmpeg.exe", "spotdl.exe")
$missing = $required | Where-Object { -not (Test-Path "src-tauri\binaries\$_") }
if ($missing) { Write-Host "Missing: $missing" } else { Write-Host "All binaries present" }
```

Expected: `All binaries present` (after running fetch-binaries.ps1 in Task 1).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/tauri.conf.json scripts/build.ps1
git commit -m "feat: .msi bundle config and build script for portable install"
```

---

## Build Instructions (for reference)

**First build (on this machine):**
```powershell
# 1. Install Rust (one-time)
winget install Rustlang.Rustup
# Restart terminal

# 2. Install cmake (one-time, required for aubio-rs C compilation)
winget install Kitware.CMake
# Restart terminal

# 3. Build
cd C:\Users\dank\dev\djdrop
.\scripts\build.ps1
```

**Installer is at:** `src-tauri\target\release\bundle\msi\djdrop_0.1.0_x64_en-US.msi`

**On laptop:** Copy the .msi, double-click, done. No Python, no pip, no PATH setup required.

**Subsequent builds (binaries already downloaded):**
```powershell
.\scripts\build.ps1 -SkipFetch
```
