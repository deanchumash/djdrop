# djdrop — Design Spec

**Date:** 2026-06-27
**Status:** Approved

## Overview

A floating circle desktop widget (always on top) that lets a DJ instantly download requested tracks during a set. Drop a URL or song name onto the circle; it downloads at the best available quality from the appropriate source. Downloads appear in a compact queue panel with BPM, key, and a native drag handle for dropping straight into Serato.

---

## Architecture

Three layers:

### 1. Tauri Shell (Rust)
- Floating always-on-top window (no taskbar entry, no title bar)
- IPC hub: routes messages between React frontend and backend processes
- Config management: reads/writes `config.toml` for non-secret settings
- Credential retrieval: calls `op read <reference>` (1Password CLI) or OS keychain fallback to fetch pool credentials at runtime
- Subprocess spawning: runs yt-dlp, spotdl, qobuz-dlp, aubio, and the Node.js pool sidecar
- Native file drag-out via `tauri-plugin-drag`

### 2. CLI Download Tools (external, on PATH)
| Tool | Source | Output |
|---|---|---|
| `yt-dlp` | YouTube, SoundCloud | MP3 320kbps (`-x --audio-format mp3 --audio-quality 0`) |
| `spotdl` | Spotify | MP3 320kbps (YouTube match) |
| `qobuz-dlp` | Qobuz | FLAC lossless (requires Qobuz account via 1Password) |
| `aubio` | post-download analysis | BPM + musical key |

### 3. Record Pool Sidecar (Node.js + Playwright, bundled)
Headless Playwright process managing sessions for BPM Supreme, Club Killers, and LiveDJService. Receives tasks from Rust via stdio JSON protocol. Handles login, search, BPM/key scraping, and file download per pool.

---

## URL Routing

The Rust backend inspects dropped URL hostnames and dispatches accordingly:

| Source | Hostname match | Tool |
|---|---|---|
| YouTube | `youtube.com`, `youtu.be` | yt-dlp |
| SoundCloud | `soundcloud.com` | yt-dlp |
| Spotify | `open.spotify.com` | spotdl |
| Qobuz | `qobuz.com` | qobuz-dlp |
| BPM Supreme | `app.bpmsupreme.com` | pool sidecar |
| Club Killers | `clubkillers.com` | pool sidecar |
| LiveDJService | `livedjservice.com` | pool sidecar |
| Plain text / search | — | configurable: pools first (default) or YouTube first |

---

## UI

### Floating Circle (idle state)
- 72px circle, always on top, no window chrome
- Download arrow icon in center
- Right-click → context menu: Settings, Quit

### Drop Interaction
- Any URL or text dragged over the circle: circle pulses as drop target
- On drop: dispatched immediately if auto-download is on; added to queue for review if off
- Circle edge becomes a progress ring while downloading

### Search Mode
- Click magnifying glass on circle → circle expands inline to a search bar
- Type and press Enter → searches according to configured source priority
- In "show results" mode: small results list appears above circle; click to download
- In "best match" mode (default): downloads immediately

### Queue Panel
- Click the circle body → panel expands upward
- Each item shows:
  - Track name
  - Source icon (SC / YT / Spotify / Qobuz / pool logo)
  - BPM (scraped from pool or analyzed via aubio)
  - Key (scraped from pool or analyzed via aubio)
  - Drag handle → native OS file drag into Serato or any other app
  - Small folder icon → reveals file in Explorer

### Completion Feedback
- Brief checkmark flash on circle on success
- Brief red X on failure (hover for error message)

### Settings Panel (opens as floating card)
- Output folder (file picker)
- Auto-download toggle (default: on)
- Search behavior: best match (default) / show results
- Search source priority: pools first (default) / YouTube first
- Record pool credentials:
  - BPM Supreme: 1Password reference fields for username + password (or direct entry fallback)
  - Club Killers: same
  - LiveDJService: same
- Qobuz: 1Password reference for username + password

---

## Credential Security

- **Primary path**: credentials stored in 1Password; user pastes `op://Vault/Item/field` references into settings. References saved to `config.toml` — safe to store, useless without 1Password session.
- **Fallback path**: if `op` CLI not installed, credentials entered directly and stored in OS keychain (Windows Credential Manager) via Tauri's keyring plugin.
- **Runtime retrieval**: Rust calls `op read <reference>` (or keychain read) and passes the live credential to the pool sidecar as an **environment variable** — never a command-line argument (visible in process listings).
- **In-flight**: credentials live only in the sidecar process's environment for the duration of the session. No credential is written to disk, logged, or kept in memory after the process exits.
- **Session cookies**: Playwright session stored in memory only. No persistent browser profile written to disk. Re-login occurs each app launch.
- **Logging**: the sidecar logger redacts any string matching the configured username or password pattern before writing output.
- **Security audit**: the record pool sidecar module is designated for a dedicated security review before v1 ship, covering credential handling, IPC protocol, and Playwright process isolation.

---

## Record Pool Sidecar Protocol

Rust ↔ Node.js sidecar communicate over stdin/stdout with newline-delimited JSON:

**Rust → Sidecar (task):**
```json
{ "id": "abc123", "type": "search", "pool": "bpmsupreme", "query": "Kendrick Lamar HUMBLE" }
{ "id": "abc124", "type": "download", "pool": "clubkillers", "url": "https://..." }
```

**Sidecar → Rust (result):**
```json
{ "id": "abc123", "status": "results", "items": [{ "title": "HUMBLE.", "bpm": 150, "key": "Am", "url": "..." }] }
{ "id": "abc124", "status": "done", "file": "/path/to/file.mp3", "bpm": 150, "key": "Am" }
{ "id": "abc124", "status": "error", "message": "Login failed" }
```

---

## Post-Download Audio Analysis

For tracks not sourced from a record pool (no BPM/key metadata available):
- After download completes, Rust spawns `aubio tempo` (BPM) and `keyfinder-cli` (musical key) on the file
- Results parsed and stored alongside the queue item
- Displayed in queue panel once analysis finishes (~2–5 seconds)
- BPM rounded to nearest integer; key displayed in standard notation (e.g., `Am`, `F#`)

---

## Settings File (`config.toml`)

```toml
output_dir = "C:/Users/dank/Music/djdrop"
auto_download = true
search_mode = "best_match"          # "best_match" | "show_results"
search_priority = "pools"           # "pools" | "youtube"

[pools.bpmsupreme]
username_ref = "op://Personal/BPM Supreme/username"
password_ref = "op://Personal/BPM Supreme/password"

[pools.clubkillers]
username_ref = "op://Personal/Club Killers/username"
password_ref = "op://Personal/Club Killers/password"

[pools.livedjservice]
username_ref = "op://Personal/LiveDJService/username"
password_ref = "op://Personal/LiveDJService/password"

[pools.qobuz]
username_ref = "op://Personal/Qobuz/username"
password_ref = "op://Personal/Qobuz/password"
```

---

## Files

| Path | Purpose |
|---|---|
| `src-tauri/src/main.rs` | Tauri entry, window setup, IPC handlers |
| `src-tauri/src/config.rs` | Config read/write |
| `src-tauri/src/credentials.rs` | 1Password CLI + keychain credential fetch |
| `src-tauri/src/downloader.rs` | Subprocess spawning for yt-dlp, spotdl, qobuz-dlp |
| `src-tauri/src/analyzer.rs` | aubio (BPM) + keyfinder-cli (key) subprocess, result parsing |
| `src-tauri/src/pool_sidecar.rs` | Spawn + communicate with Node.js sidecar |
| `src-tauri/src/router.rs` | URL hostname → tool routing |
| `sidecar/src/index.ts` | Node.js sidecar entry, stdin/stdout protocol |
| `sidecar/src/pools/bpmsupreme.ts` | BPM Supreme Playwright scraper |
| `sidecar/src/pools/clubkillers.ts` | Club Killers Playwright scraper |
| `sidecar/src/pools/livedjservice.ts` | LiveDJService Playwright scraper |
| `sidecar/src/logger.ts` | Redacting logger |
| `src/App.tsx` | React root |
| `src/components/Circle.tsx` | Floating circle widget |
| `src/components/QueuePanel.tsx` | Download queue with drag handles |
| `src/components/SearchBar.tsx` | Inline search expansion |
| `src/components/SettingsPanel.tsx` | Settings floating card |
| `src/hooks/useDownloads.ts` | Download state + IPC |
| `config.toml` | User settings (non-secret) |

---

## External Dependencies

| Dependency | Install method |
|---|---|
| `yt-dlp` | `pip install yt-dlp` |
| `spotdl` | `pip install spotdl` |
| `qobuz-dlp` | `pip install qobuz-dlp` |
| `aubio` | `pip install aubio` |
| `keyfinder-cli` | `winget install keyfinder-cli` or build from source |
| `op` (1Password CLI) | 1Password desktop app (bundled) |
| Node.js | bundled in sidecar via `pkg` or similar |
| Playwright | bundled in sidecar (`npm install playwright`) |

---

## Constraints & Edge Cases

- `op read` blocks until 1Password is unlocked; if the session has expired the sidecar will wait for the Rust side to receive the credential before starting download
- Record pool scrapers will break when pool UIs change — each pool module should be independently updatable
- `aubio` analysis runs after download completes; BPM/key fields show a spinner until ready
- Auto-download off + show-results mode: dropped plain text triggers a results list, not an immediate download
- If yt-dlp returns a 320kbps MP3 is unavailable, it falls back to best available audio and re-encodes
- All file writes go to `output_dir`; no temp files in system temp directories (avoids cross-job collisions)
