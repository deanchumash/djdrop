# djdrop Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build djdrop — a floating 72px circle desktop widget that accepts dropped URLs/text, downloads tracks at best quality via yt-dlp/spotdl/qobuz-dlp, shows BPM+key in a queue panel, and lets you drag downloads straight into Serato.

**Architecture:** Tauri v2 (Rust backend + React 19 frontend). Rust handles window management, config, credential retrieval, and subprocess spawning. React renders the circle UI, queue panel, search bar, and settings. IPC uses `invoke()` for commands and `listen()` for events. This is Plan 1 of 2 — no pool sidecar yet.

**Tech Stack:** Tauri v2, React 19, TypeScript (strict), Vite, Rust (tokio async), yt-dlp, spotdl, qobuz-dlp, aubio, keyfinder-cli, 1Password CLI (`op`), tauri-plugin-drag, tauri-plugin-keyring, Vitest + @testing-library/react

## Global Constraints

- Tauri v2 — not v1; APIs differ significantly
- Window: 72×72px, `decorations: false`, `transparent: true`, `alwaysOnTop: true`, `resizable: false`, `skipTaskbar: true`
- yt-dlp flags for MP3: `-x --audio-format mp3 --audio-quality 0 -P <output_dir>`
- qobuz-dlp downloads FLAC; no conversion needed
- spotdl flags: `download <url> --output <output_dir> --format mp3 --bitrate 320k`
- All IPC events prefixed: `download:progress`, `download:done`, `download:error`, `analysis:done`, `search:results`
- IPC command names: `snake_case`
- Config file location: Tauri app config dir (`app_handle.path().app_config_dir()`) — `config.toml`
- Credentials NEVER written to disk, NEVER passed as CLI args (env vars only)
- All Rust modules have `#[cfg(test)]` unit tests; all React components have Vitest tests
- No pool sidecar in this plan — pool drops show `error: pool sidecar not connected`

---

## File Map

```
djdrop/
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   └── src/
│       ├── main.rs          # calls lib::run()
│       ├── lib.rs           # Tauri builder, registers all commands
│       ├── config.rs        # Config struct, read/write config.toml
│       ├── router.rs        # URL/text → DownloadSource enum
│       ├── downloader.rs    # spawn yt-dlp / spotdl / qobuz-dlp, emit IPC events
│       ├── analyzer.rs      # spawn aubio + keyfinder-cli, parse output
│       └── credentials.rs   # op read + keychain fallback
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── App.css
│   ├── types.ts             # shared TS types mirroring Rust structs
│   ├── hooks/
│   │   ├── useDownloads.ts  # download state, IPC listeners
│   │   └── useConfig.ts     # config state, invoke get_config/save_config
│   └── components/
│       ├── Circle.tsx        # 72px circle, drop target, progress ring
│       ├── SearchBar.tsx     # inline search expansion
│       ├── QueuePanel.tsx    # expanding queue above circle
│       ├── QueueItem.tsx     # single item: name, source, BPM, key, drag handle
│       └── SettingsPanel.tsx # floating settings card
└── src/
    └── test/
        └── setup.ts         # Vitest setup, mock @tauri-apps/api/core
```

---

### Task 1: Scaffold — Tauri + React + floating circle window

**Files:**
- Create: project root (run scaffold command)
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/Cargo.toml`
- Create: `src/App.tsx`, `src/App.css`
- Create: `src/components/Circle.tsx`
- Create: `src/test/setup.ts`

**Interfaces:**
- Produces: running Tauri app with a 72px transparent floating circle; `<Circle />` component accepting `onDrop(input: string): void` prop

- [ ] **Step 1: Scaffold the project**

```bash
cd C:/Users/dank/dev
npm create tauri-app@latest djdrop -- --template react-ts --manager npm
cd djdrop
npm install
```

- [ ] **Step 2: Replace `src-tauri/tauri.conf.json`**

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
    "icon": ["icons/32x32.png","icons/128x128.png","icons/128x128@2x.png","icons/icon.icns","icons/icon.ico"]
  }
}
```

- [ ] **Step 3: Update `src-tauri/Cargo.toml` dependencies**

```toml
[package]
name = "djdrop"
version = "0.1.0"
edition = "2021"

[lib]
name = "djdrop_lib"
crate-type = ["lib", "cdylib", "staticlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

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

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 4: Create `src/App.css`**

```css
html, body, #root {
  margin: 0; padding: 0;
  width: 72px; height: 72px;
  overflow: hidden;
  background: transparent;
  user-select: none;
}

.circle {
  width: 72px; height: 72px;
  border-radius: 50%;
  background: #1a1a2e;
  display: flex; align-items: center; justify-content: center;
  color: white; font-size: 24px;
  cursor: default;
  transition: background 0.15s, box-shadow 0.15s;
  position: relative;
}

.circle[data-tauri-drag-region] { cursor: move; }

.circle.drag-over { box-shadow: 0 0 0 3px #4ade80; }

.progress-ring {
  position: absolute; top: 0; left: 0;
  width: 72px; height: 72px;
  transform: rotate(-90deg);
  pointer-events: none;
}
```

- [ ] **Step 5: Create `src/components/Circle.tsx`**

```tsx
import { useState } from 'react';

interface Props {
  onDrop: (input: string) => void;
  progress?: number; // 0–100, undefined = idle
}

export function Circle({ onDrop, progress }: Props) {
  const [isDragOver, setIsDragOver] = useState(false);

  const handleDragOver = (e: React.DragEvent) => { e.preventDefault(); setIsDragOver(true); };
  const handleDragLeave = () => setIsDragOver(false);
  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragOver(false);
    const input =
      e.dataTransfer.getData('text/uri-list') ||
      e.dataTransfer.getData('text/plain');
    if (input.trim()) onDrop(input.trim());
  };

  const circumference = 2 * Math.PI * 33; // r=33
  const dashOffset = progress != null
    ? circumference * (1 - progress / 100)
    : circumference;

  return (
    <div
      className={`circle${isDragOver ? ' drag-over' : ''}`}
      data-tauri-drag-region
      onDragOver={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {progress != null ? (
        <svg className="progress-ring" viewBox="0 0 72 72">
          <circle cx="36" cy="36" r="33" fill="none" stroke="#4ade80" strokeWidth="3"
            strokeDasharray={circumference} strokeDashoffset={dashOffset}
            style={{ transition: 'stroke-dashoffset 0.3s' }} />
        </svg>
      ) : null}
      ↓
    </div>
  );
}
```

- [ ] **Step 6: Create `src/App.tsx`**

```tsx
import './App.css';
import { Circle } from './components/Circle';

function App() {
  const handleDrop = (input: string) => {
    console.log('dropped:', input); // wired up in Task 4
  };

  return <Circle onDrop={handleDrop} />;
}

export default App;
```

- [ ] **Step 7: Install Vitest + testing library**

```bash
npm install -D vitest @vitest/ui jsdom @testing-library/react @testing-library/jest-dom
```

Add to `vite.config.ts`:
```ts
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: 'jsdom',
    setupFiles: ['./src/test/setup.ts'],
  },
});
```

- [ ] **Step 8: Create `src/test/setup.ts`**

```ts
import '@testing-library/jest-dom';
import { vi } from 'vitest';

// Mock Tauri APIs — not available in jsdom
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock('@tauri-apps/plugin-drag', () => ({
  startDrag: vi.fn(),
}));
```

- [ ] **Step 9: Write Circle component test**

Create `src/components/Circle.test.tsx`:
```tsx
import { render, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { Circle } from './Circle';

describe('Circle', () => {
  it('calls onDrop with uri-list data', () => {
    const onDrop = vi.fn();
    const { container } = render(<Circle onDrop={onDrop} />);
    const el = container.firstChild as HTMLElement;

    fireEvent.dragOver(el, { dataTransfer: { getData: () => 'https://soundcloud.com/test' } });
    fireEvent.drop(el, {
      dataTransfer: { getData: (type: string) =>
        type === 'text/uri-list' ? 'https://soundcloud.com/test' : '' },
    });

    expect(onDrop).toHaveBeenCalledWith('https://soundcloud.com/test');
  });

  it('falls back to text/plain if no uri-list', () => {
    const onDrop = vi.fn();
    const { container } = render(<Circle onDrop={onDrop} />);
    const el = container.firstChild as HTMLElement;

    fireEvent.drop(el, {
      dataTransfer: { getData: (type: string) =>
        type === 'text/plain' ? 'Kendrick Lamar HUMBLE' : '' },
    });

    expect(onDrop).toHaveBeenCalledWith('Kendrick Lamar HUMBLE');
  });

  it('shows progress ring when progress prop is set', () => {
    const { container } = render(<Circle onDrop={() => {}} progress={50} />);
    expect(container.querySelector('svg')).toBeTruthy();
  });
});
```

- [ ] **Step 10: Run tests**

```bash
npm run test -- --run
```
Expected: 3 tests pass.

- [ ] **Step 11: Launch dev app to verify floating circle**

```bash
npm run tauri dev
```
Expected: a small circle appears floating over other windows, draggable by the circle body.

- [ ] **Step 12: Commit**

```bash
git add -A
git commit -m "feat: Tauri + React scaffold with floating circle window"
```

---

### Task 2: Config module

**Files:**
- Create: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/lib.rs` (register `get_config`, `save_config` commands)
- Create: `src/types.ts`
- Create: `src/hooks/useConfig.ts`

**Interfaces:**
- Produces:
  - Rust: `pub fn read(app: &AppHandle) -> anyhow::Result<Config>`
  - Rust: `pub fn write(app: &AppHandle, config: &Config) -> anyhow::Result<()>`
  - Tauri commands: `get_config() -> Config`, `save_config(config: Config) -> ()`
  - React hook: `useConfig()` → `{ config: Config | null, saveConfig(c: Config): Promise<void> }`

- [ ] **Step 1: Create `src-tauri/src/config.rs`**

```rust
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchMode { BestMatch, ShowResults }

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SearchPriority { Pools, Youtube }

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolCredentialRefs {
    pub username_ref: String,
    pub password_ref: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolsConfig {
    pub bpmsupreme: PoolCredentialRefs,
    pub clubkillers: PoolCredentialRefs,
    pub livedjservice: PoolCredentialRefs,
    pub qobuz: PoolCredentialRefs,
}

impl Default for PoolsConfig {
    fn default() -> Self {
        Self {
            bpmsupreme: Default::default(),
            clubkillers: Default::default(),
            livedjservice: Default::default(),
            qobuz: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub output_dir: String,
    pub auto_download: bool,
    pub search_mode: SearchMode,
    pub search_priority: SearchPriority,
    pub pools: PoolsConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            output_dir: dirs::music_dir()
                .unwrap_or_default()
                .join("djdrop")
                .to_string_lossy()
                .into_owned(),
            auto_download: true,
            search_mode: SearchMode::BestMatch,
            search_priority: SearchPriority::Pools,
            pools: Default::default(),
        }
    }
}

fn config_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf> {
    let dir = app.path().app_config_dir().context("no config dir")?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join("config.toml"))
}

pub fn read(app: &tauri::AppHandle) -> Result<Config> {
    let path = config_path(app)?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = fs::read_to_string(&path)?;
    toml::from_str(&text).context("invalid config.toml")
}

pub fn write(app: &tauri::AppHandle, config: &Config) -> Result<()> {
    let path = config_path(app)?;
    let text = toml::to_string_pretty(config).context("serialize config")?;
    fs::write(path, text).context("write config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_auto_download_true() {
        let c = Config::default();
        assert!(c.auto_download);
        assert_eq!(c.search_mode, SearchMode::BestMatch);
        assert_eq!(c.search_priority, SearchPriority::Pools);
    }

    #[test]
    fn config_roundtrip_toml() {
        let original = Config::default();
        let text = toml::to_string_pretty(&original).unwrap();
        let parsed: Config = toml::from_str(&text).unwrap();
        assert_eq!(parsed.auto_download, original.auto_download);
        assert_eq!(parsed.output_dir, original.output_dir);
    }
}
```

Add `dirs = "5"` to `Cargo.toml` dependencies.

- [ ] **Step 2: Create `src-tauri/src/lib.rs`**

```rust
mod config;
mod router;
mod downloader;
mod analyzer;
mod credentials;

use tauri::Manager;

#[tauri::command]
async fn get_config(app: tauri::AppHandle) -> Result<config::Config, String> {
    config::read(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(app: tauri::AppHandle, config: config::Config) -> Result<(), String> {
    config::write(&app, &config).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_keyring::init())
        .invoke_handler(tauri::generate_handler![
            get_config,
            save_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running djdrop");
}
```

- [ ] **Step 3: Create `src/types.ts`**

```ts
export interface PoolCredentialRefs {
  username_ref: string;
  password_ref: string;
}

export interface PoolsConfig {
  bpmsupreme: PoolCredentialRefs;
  clubkillers: PoolCredentialRefs;
  livedjservice: PoolCredentialRefs;
  qobuz: PoolCredentialRefs;
}

export type SearchMode = 'best_match' | 'show_results';
export type SearchPriority = 'pools' | 'youtube';

export interface Config {
  output_dir: string;
  auto_download: boolean;
  search_mode: SearchMode;
  search_priority: SearchPriority;
  pools: PoolsConfig;
}

export type DownloadStatus = 'queued' | 'downloading' | 'analyzing' | 'done' | 'error';

export interface DownloadItem {
  id: string;
  input: string;
  track_name?: string;
  file_path?: string;
  source?: string;
  bpm?: number;
  key?: string;
  progress?: number;
  status: DownloadStatus;
  error?: string;
}

// IPC event payloads
export interface ProgressPayload { id: string; percent: number }
export interface DonePayload { id: string; file_path: string; track_name: string; source: string }
export interface ErrorPayload { id: string; message: string }
export interface AnalysisDonePayload { id: string; bpm: number; key: string }
export interface SearchResultItem { title: string; url: string; source: string }
export interface SearchResultsPayload { id: string; items: SearchResultItem[] }
```

- [ ] **Step 4: Create `src/hooks/useConfig.ts`**

```ts
import { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Config } from '../types';

export function useConfig() {
  const [config, setConfig] = useState<Config | null>(null);

  useEffect(() => {
    invoke<Config>('get_config').then(setConfig).catch(console.error);
  }, []);

  const saveConfig = async (updated: Config) => {
    await invoke('save_config', { config: updated });
    setConfig(updated);
  };

  return { config, saveConfig };
}
```

- [ ] **Step 5: Write Rust unit tests**

Run:
```bash
cd src-tauri && cargo test config
```
Expected:
```
test config::tests::default_config_has_auto_download_true ... ok
test config::tests::config_roundtrip_toml ... ok
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: config module with TOML read/write and useConfig hook"
```

---

### Task 3: URL router

**Files:**
- Create: `src-tauri/src/router.rs`

**Interfaces:**
- Produces: `pub fn route(input: &str) -> DownloadSource`
- Produces:
  ```rust
  pub enum DownloadSource { YtDlp, Spotdl, QobuzDlp, Pool(Pool), Search(String) }
  pub enum Pool { BpmSupreme, ClubKillers, LiveDjService }
  ```

- [ ] **Step 1: Write tests first**

Create `src-tauri/src/router.rs` with tests only:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn routes_youtube() { assert_eq!(route("https://www.youtube.com/watch?v=abc"), DownloadSource::YtDlp); }
    #[test] fn routes_youtu_be() { assert_eq!(route("https://youtu.be/abc"), DownloadSource::YtDlp); }
    #[test] fn routes_soundcloud() { assert_eq!(route("https://soundcloud.com/artist/track"), DownloadSource::YtDlp); }
    #[test] fn routes_spotify() { assert_eq!(route("https://open.spotify.com/track/abc"), DownloadSource::Spotdl); }
    #[test] fn routes_qobuz() { assert_eq!(route("https://www.qobuz.com/album/abc"), DownloadSource::QobuzDlp); }
    #[test] fn routes_bpmsupreme() { assert_eq!(route("https://app.bpmsupreme.com/track/123"), DownloadSource::Pool(Pool::BpmSupreme)); }
    #[test] fn routes_clubkillers() { assert_eq!(route("https://www.clubkillers.com/track/abc"), DownloadSource::Pool(Pool::ClubKillers)); }
    #[test] fn routes_livedjservice() { assert_eq!(route("https://www.livedjservice.com/track/abc"), DownloadSource::Pool(Pool::LiveDjService)); }
    #[test] fn routes_plain_text_to_search() { assert_eq!(route("Kendrick Lamar HUMBLE"), DownloadSource::Search("Kendrick Lamar HUMBLE".into())); }
    #[test] fn routes_unknown_url_to_search() { assert_eq!(route("https://example.com/audio"), DownloadSource::Search("https://example.com/audio".into())); }
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd src-tauri && cargo test router 2>&1 | head -20
```
Expected: compile error — `route`, `DownloadSource`, `Pool` not defined.

- [ ] **Step 3: Implement `src-tauri/src/router.rs`**

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Pool { BpmSupreme, ClubKillers, LiveDjService }

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadSource { YtDlp, Spotdl, QobuzDlp, Pool(Pool), Search(String) }

pub fn route(input: &str) -> DownloadSource {
    if let Ok(u) = url::Url::parse(input) {
        let host = u.host_str().unwrap_or("").to_lowercase();
        if host.contains("youtube.com") || host.contains("youtu.be") { return DownloadSource::YtDlp; }
        if host.contains("soundcloud.com") { return DownloadSource::YtDlp; }
        if host.contains("spotify.com") { return DownloadSource::Spotdl; }
        if host.contains("qobuz.com") { return DownloadSource::QobuzDlp; }
        if host.contains("bpmsupreme.com") { return DownloadSource::Pool(Pool::BpmSupreme); }
        if host.contains("clubkillers.com") { return DownloadSource::Pool(Pool::ClubKillers); }
        if host.contains("livedjservice.com") { return DownloadSource::Pool(Pool::LiveDjService); }
    }
    DownloadSource::Search(input.to_string())
}

#[cfg(test)]
mod tests { /* same as Step 1 */ }
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test router
```
Expected: 10 tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: URL router — hostname to DownloadSource mapping"
```

---

### Task 4: Downloader — yt-dlp + IPC events

**Files:**
- Create: `src-tauri/src/downloader.rs`
- Modify: `src-tauri/src/lib.rs` (add `start_download` command)

**Interfaces:**
- Consumes: `router::DownloadSource`, `config::Config`
- Produces: Tauri command `start_download(id: String, input: String) -> ()`
- Emits events: `download:progress {id, percent}`, `download:done {id, file_path, track_name, source}`, `download:error {id, message}`

- [ ] **Step 1: Write tests first in `src-tauri/src/downloader.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_args_include_mp3_flags() {
        let args = ytdlp_args("https://youtube.com/watch?v=x", "/tmp/out");
        assert!(args.contains(&"--audio-format".to_string()));
        assert!(args.contains(&"mp3".to_string()));
        assert!(args.contains(&"--audio-quality".to_string()));
        assert!(args.contains(&"0".to_string()));
        assert!(args.contains(&"-P".to_string()));
        assert!(args.contains(&"/tmp/out".to_string()));
    }

    #[test]
    fn spotdl_args_include_320k() {
        let args = spotdl_args("https://open.spotify.com/track/x", "/tmp/out");
        assert!(args.contains(&"320k".to_string()));
        assert!(args.contains(&"mp3".to_string()));
    }

    #[test]
    fn qobuz_args_include_url() {
        let args = qobuz_args("https://www.qobuz.com/album/x", "/tmp/out");
        assert!(args.contains(&"https://www.qobuz.com/album/x".to_string()));
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd src-tauri && cargo test downloader 2>&1 | head -10
```
Expected: compile error.

- [ ] **Step 3: Implement `src-tauri/src/downloader.rs`**

```rust
use crate::router::DownloadSource;
use serde::Serialize;
use std::process::Stdio;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Clone, Serialize)]
pub struct ProgressPayload { pub id: String, pub percent: u8 }
#[derive(Clone, Serialize)]
pub struct DonePayload { pub id: String, pub file_path: String, pub track_name: String, pub source: String }
#[derive(Clone, Serialize)]
pub struct ErrorPayload { pub id: String, pub message: String }

pub fn ytdlp_args(url: &str, output_dir: &str) -> Vec<String> {
    vec![
        "-x".into(), "--audio-format".into(), "mp3".into(),
        "--audio-quality".into(), "0".into(),
        "-P".into(), output_dir.into(),
        "--print".into(), "after_move:filepath".into(),
        "--no-playlist".into(),
        url.into(),
    ]
}

pub fn spotdl_args(url: &str, output_dir: &str) -> Vec<String> {
    vec![
        "download".into(), url.into(),
        "--output".into(), output_dir.into(),
        "--format".into(), "mp3".into(),
        "--bitrate".into(), "320k".into(),
    ]
}

pub fn qobuz_args(url: &str, output_dir: &str) -> Vec<String> {
    vec![url.into(), "-d".into(), output_dir.into()]
}

pub async fn run_download(app: AppHandle, id: String, source: DownloadSource, input: String, output_dir: String) {
    let result = match &source {
        DownloadSource::YtDlp => {
            run_ytdlp(&app, &id, &input, &output_dir).await
        }
        DownloadSource::Spotdl => {
            run_spotdl(&app, &id, &input, &output_dir).await
        }
        DownloadSource::QobuzDlp => {
            run_qobuz(&app, &id, &input, &output_dir).await
        }
        DownloadSource::Pool(_) => {
            Err("pool sidecar not connected".to_string())
        }
        DownloadSource::Search(query) => {
            let ytdlp_url = format!("ytsearch1:{}", query);
            run_ytdlp(&app, &id, &ytdlp_url, &output_dir).await
        }
    };

    if let Err(msg) = result {
        let _ = app.emit("download:error", ErrorPayload { id, message: msg });
    }
}

async fn run_ytdlp(app: &AppHandle, id: &str, url: &str, output_dir: &str) -> Result<(), String> {
    let args = ytdlp_args(url, output_dir);
    let mut child = Command::new("yt-dlp")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("yt-dlp not found: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let mut file_path = String::new();

    while let Ok(Some(line)) = lines.next_line().await {
        // yt-dlp --print after_move:filepath prints the output path
        if line.ends_with(".mp3") || line.ends_with(".m4a") {
            file_path = line.clone();
        }
        // Parse "[download]  42.3% of" for progress
        if let Some(pct) = parse_ytdlp_progress(&line) {
            let _ = app.emit("download:progress", ProgressPayload { id: id.to_string(), percent: pct });
        }
    }

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("yt-dlp exited with error".into());
    }

    let track_name = std::path::Path::new(&file_path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| url.to_string());

    let _ = app.emit("download:done", DonePayload {
        id: id.to_string(),
        file_path,
        track_name,
        source: "youtube".into(),
    });

    Ok(())
}

async fn run_spotdl(app: &AppHandle, id: &str, url: &str, output_dir: &str) -> Result<(), String> {
    let args = spotdl_args(url, output_dir);
    let status = Command::new("spotdl")
        .args(&args)
        .status()
        .await
        .map_err(|e| format!("spotdl not found: {e}"))?;

    if !status.success() { return Err("spotdl exited with error".into()); }

    // spotdl doesn't print filepath easily; scan output_dir for newest mp3
    let file_path = newest_mp3_in(output_dir)?;
    let track_name = std::path::Path::new(&file_path)
        .file_stem().map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let _ = app.emit("download:done", DonePayload {
        id: id.to_string(), file_path, track_name, source: "spotify".into()
    });
    Ok(())
}

async fn run_qobuz(app: &AppHandle, id: &str, url: &str, output_dir: &str) -> Result<(), String> {
    let args = qobuz_args(url, output_dir);
    let status = Command::new("qobuz-dlp")
        .args(&args)
        .status()
        .await
        .map_err(|e| format!("qobuz-dlp not found: {e}"))?;

    if !status.success() { return Err("qobuz-dlp exited with error".into()); }

    let file_path = newest_file_in(output_dir, &["flac","mp3"])?;
    let track_name = std::path::Path::new(&file_path)
        .file_stem().map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let _ = app.emit("download:done", DonePayload {
        id: id.to_string(), file_path, track_name, source: "qobuz".into()
    });
    Ok(())
}

fn parse_ytdlp_progress(line: &str) -> Option<u8> {
    // "[download]  42.3% of"
    let trimmed = line.trim();
    if !trimmed.starts_with("[download]") { return None; }
    let pct_str = trimmed.split_whitespace().nth(1)?;
    let pct_str = pct_str.trim_end_matches('%');
    pct_str.parse::<f64>().ok().map(|f| f as u8)
}

fn newest_mp3_in(dir: &str) -> Result<String, String> {
    newest_file_in(dir, &["mp3"])
}

fn newest_file_in(dir: &str, exts: &[&str]) -> Result<String, String> {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .filter_map(|e| e.ok())
        .filter(|e| {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
            exts.contains(&ext.as_str())
        })
        .collect();
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    entries.last()
        .map(|e| e.path().to_string_lossy().into_owned())
        .ok_or_else(|| "no audio file found in output dir".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ytdlp_args_include_mp3_flags() {
        let args = ytdlp_args("https://youtube.com/watch?v=x", "/tmp/out");
        assert!(args.contains(&"--audio-format".to_string()));
        assert!(args.contains(&"mp3".to_string()));
        assert!(args.contains(&"--audio-quality".to_string()));
        assert!(args.contains(&"0".to_string()));
        assert!(args.contains(&"-P".to_string()));
        assert!(args.contains(&"/tmp/out".to_string()));
    }

    #[test]
    fn spotdl_args_include_320k() {
        let args = spotdl_args("https://open.spotify.com/track/x", "/tmp/out");
        assert!(args.contains(&"320k".to_string()));
        assert!(args.contains(&"mp3".to_string()));
    }

    #[test]
    fn qobuz_args_include_url() {
        let args = qobuz_args("https://www.qobuz.com/album/x", "/tmp/out");
        assert!(args.contains(&"https://www.qobuz.com/album/x".to_string()));
    }

    #[test]
    fn parse_progress_extracts_percent() {
        assert_eq!(parse_ytdlp_progress("[download]  42.3% of 5.00MiB"), Some(42));
        assert_eq!(parse_ytdlp_progress("[download] 100% of 5.00MiB"), Some(100));
        assert_eq!(parse_ytdlp_progress("[info] Writing thumbnail"), None);
    }
}
```

- [ ] **Step 4: Add `start_download` command to `lib.rs`**

```rust
// Add to lib.rs imports
use crate::{config, router, downloader};
use uuid::Uuid;

#[tauri::command]
async fn start_download(app: tauri::AppHandle, id: String, input: String) -> Result<(), String> {
    let cfg = config::read(&app).map_err(|e| e.to_string())?;
    let source = router::route(&input);
    let output_dir = cfg.output_dir.clone();
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    tokio::spawn(downloader::run_download(app, id, source, input, output_dir));
    Ok(())
}

// Add to generate_handler![]: start_download,
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test downloader
```
Expected: 4 tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: downloader — yt-dlp/spotdl/qobuz-dlp subprocess + IPC events"
```

---

### Task 5: Analyzer — BPM + key post-download

**Files:**
- Create: `src-tauri/src/analyzer.rs`
- Modify: `src-tauri/src/downloader.rs` (call analyzer after done event)
- Modify: `src-tauri/src/lib.rs` (export analyzer)

**Interfaces:**
- Consumes: `file_path: String`, `id: String`, `AppHandle`
- Emits: `analysis:done { id, bpm, key }` (e.g. `{ id: "abc", bpm: 128, key: "Am" }`)

- [ ] **Step 1: Write failing tests in `src-tauri/src/analyzer.rs`**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aubio_bpm_output() {
        // aubio tempo outputs lines like: "128.000000\n"
        assert_eq!(parse_bpm("128.000000\n"), Some(128));
        assert_eq!(parse_bpm("  140.5\n"), Some(140));
        assert_eq!(parse_bpm("garbage"), None);
    }

    #[test]
    fn parses_keyfinder_output() {
        // keyfinder-cli outputs e.g. "Am\n" or "F#\n"
        assert_eq!(parse_key("Am\n"), Some("Am".to_string()));
        assert_eq!(parse_key("F#\n"), Some("F#".to_string()));
        assert_eq!(parse_key(""), None);
    }
}
```

- [ ] **Step 2: Run to confirm failure**

```bash
cd src-tauri && cargo test analyzer 2>&1 | head -10
```

- [ ] **Step 3: Implement `src-tauri/src/analyzer.rs`**

```rust
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Clone, Serialize)]
pub struct AnalysisDonePayload { pub id: String, pub bpm: u32, pub key: String }

pub async fn analyze(app: &AppHandle, id: &str, file_path: &str) {
    let bpm = run_aubio(file_path).await.unwrap_or(0);
    let key = run_keyfinder(file_path).await.unwrap_or_default();
    if bpm > 0 || !key.is_empty() {
        let _ = app.emit("analysis:done", AnalysisDonePayload {
            id: id.to_string(), bpm, key,
        });
    }
}

async fn run_aubio(file_path: &str) -> Option<u32> {
    let output = Command::new("aubio")
        .args(["tempo", file_path])
        .output().await.ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_bpm(&stdout)
}

async fn run_keyfinder(file_path: &str) -> Option<String> {
    let output = Command::new("keyfinder-cli")
        .args([file_path])
        .output().await.ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_key(&stdout)
}

pub fn parse_bpm(output: &str) -> Option<u32> {
    output.trim().parse::<f64>().ok().map(|f| f.round() as u32)
}

pub fn parse_key(output: &str) -> Option<String> {
    let s = output.trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_aubio_bpm_output() {
        assert_eq!(parse_bpm("128.000000\n"), Some(128));
        assert_eq!(parse_bpm("  140.5\n"), Some(140));
        assert_eq!(parse_bpm("garbage"), None);
    }

    #[test]
    fn parses_keyfinder_output() {
        assert_eq!(parse_key("Am\n"), Some("Am".to_string()));
        assert_eq!(parse_key("F#\n"), Some("F#".to_string()));
        assert_eq!(parse_key(""), None);
    }
}
```

- [ ] **Step 4: Wire analyzer into downloader — call after emitting `download:done`**

In `run_ytdlp`, after the `app.emit("download:done", ...)` line:
```rust
crate::analyzer::analyze(app, id, &file_path).await;
```
Add the same call in `run_spotdl` and `run_qobuz` after their `download:done` emits.

- [ ] **Step 5: Run tests**

```bash
cd src-tauri && cargo test analyzer
```
Expected: 2 tests pass.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat: analyzer — aubio BPM + keyfinder-cli key detection post-download"
```

---

### Task 6: Credentials module (1Password + keychain)

**Files:**
- Create: `src-tauri/src/credentials.rs`
- Modify: `src-tauri/src/lib.rs` (add `get_credential`, `save_credential` commands)

**Interfaces:**
- Produces:
  - `pub async fn resolve(username_ref: &str, password_ref: &str) -> Result<(String,String), String>`
  - Tauri command `save_credential(pool: String, username: String, password: String) -> ()`

- [ ] **Step 1: Write tests first**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_op_reference() {
        assert!(is_op_ref("op://Personal/BPM Supreme/username"));
        assert!(!is_op_ref("myusername@email.com"));
        assert!(!is_op_ref(""));
    }

    #[test]
    fn empty_ref_returns_empty_string() {
        // Direct (non-op) values pass through unchanged
        assert_eq!(resolve_direct("myuser"), "myuser");
        assert_eq!(resolve_direct(""), "");
    }
}
```

- [ ] **Step 2: Implement `src-tauri/src/credentials.rs`**

```rust
use keyring::Entry;

const KEYRING_SERVICE: &str = "djdrop";

pub fn is_op_ref(s: &str) -> bool {
    s.starts_with("op://")
}

pub fn resolve_direct(s: &str) -> String {
    s.to_string()
}

/// Resolve a username_ref + password_ref to actual (username, password).
/// If refs start with "op://", call `op read`. Otherwise treat as literal values.
pub async fn resolve(username_ref: &str, password_ref: &str) -> Result<(String, String), String> {
    let username = if is_op_ref(username_ref) {
        op_read(username_ref).await?
    } else if username_ref.is_empty() {
        // Fall back to keychain for the service named by the ref key
        String::new()
    } else {
        username_ref.to_string()
    };

    let password = if is_op_ref(password_ref) {
        op_read(password_ref).await?
    } else {
        password_ref.to_string()
    };

    Ok((username, password))
}

async fn op_read(reference: &str) -> Result<String, String> {
    let output = tokio::process::Command::new("op")
        .args(["read", reference])
        .output()
        .await
        .map_err(|_| "1Password CLI (`op`) not found — install 1Password desktop app".to_string())?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("op read failed: {}", stderr.trim()));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Store credentials directly in OS keychain (fallback when op not available).
pub fn store_in_keychain(pool: &str, username: &str, password: &str) -> Result<(), String> {
    let entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:username"))
        .map_err(|e| e.to_string())?;
    entry.set_password(username).map_err(|e| e.to_string())?;

    let entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:password"))
        .map_err(|e| e.to_string())?;
    entry.set_password(password).map_err(|e| e.to_string())?;

    Ok(())
}

pub fn read_from_keychain(pool: &str) -> Result<(String, String), String> {
    let u_entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:username"))
        .map_err(|e| e.to_string())?;
    let username = u_entry.get_password().map_err(|e| e.to_string())?;

    let p_entry = Entry::new(KEYRING_SERVICE, &format!("{pool}:password"))
        .map_err(|e| e.to_string())?;
    let password = p_entry.get_password().map_err(|e| e.to_string())?;

    Ok((username, password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_op_reference() {
        assert!(is_op_ref("op://Personal/BPM Supreme/username"));
        assert!(!is_op_ref("myusername@email.com"));
        assert!(!is_op_ref(""));
    }

    #[test]
    fn empty_ref_returns_empty_string() {
        assert_eq!(resolve_direct("myuser"), "myuser");
        assert_eq!(resolve_direct(""), "");
    }
}
```

- [ ] **Step 3: Add commands to `lib.rs`**

```rust
#[tauri::command]
async fn save_credential(pool: String, username: String, password: String) -> Result<(), String> {
    credentials::store_in_keychain(&pool, &username, &password)
}

// Add to generate_handler![]: save_credential,
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri && cargo test credentials
```
Expected: 2 tests pass.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat: credentials — 1Password CLI (op read) + keychain fallback"
```

---

### Task 7: React UI — drop target, search bar, queue panel, drag-out

**Files:**
- Create: `src/hooks/useDownloads.ts`
- Create: `src/components/SearchBar.tsx`
- Create: `src/components/QueueItem.tsx`
- Create: `src/components/QueuePanel.tsx`
- Modify: `src/App.tsx` (wire everything together)
- Modify: `src/App.css` (queue panel + search styles)

**Interfaces:**
- Consumes: `invoke('start_download')`, `listen('download:*')`, `listen('analysis:done')`, `startDrag` from `@tauri-apps/plugin-drag`
- Produces: complete UI — circle + search expansion + queue panel

- [ ] **Step 1: Install drag plugin frontend**

```bash
npm install @tauri-apps/plugin-drag
```

- [ ] **Step 2: Create `src/hooks/useDownloads.ts`**

```ts
import { useState, useEffect, useCallback } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import type { DownloadItem, ProgressPayload, DonePayload, ErrorPayload, AnalysisDonePayload } from '../types';

export function useDownloads() {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [activeProgress, setActiveProgress] = useState<number | undefined>(undefined);

  useEffect(() => {
    const unlisteners = [
      listen<ProgressPayload>('download:progress', ({ payload }) => {
        setActiveProgress(payload.percent);
        setDownloads(prev => prev.map(d =>
          d.id === payload.id ? { ...d, progress: payload.percent, status: 'downloading' } : d
        ));
      }),
      listen<DonePayload>('download:done', ({ payload }) => {
        setActiveProgress(undefined);
        setDownloads(prev => prev.map(d =>
          d.id === payload.id
            ? { ...d, file_path: payload.file_path, track_name: payload.track_name, source: payload.source, status: 'analyzing' }
            : d
        ));
      }),
      listen<ErrorPayload>('download:error', ({ payload }) => {
        setActiveProgress(undefined);
        setDownloads(prev => prev.map(d =>
          d.id === payload.id ? { ...d, status: 'error', error: payload.message } : d
        ));
      }),
      listen<AnalysisDonePayload>('analysis:done', ({ payload }) => {
        setDownloads(prev => prev.map(d =>
          d.id === payload.id ? { ...d, bpm: payload.bpm, key: payload.key, status: 'done' } : d
        ));
      }),
    ];
    return () => { unlisteners.forEach(p => p.then(fn => fn())); };
  }, []);

  const startDownload = useCallback(async (input: string) => {
    const id = crypto.randomUUID();
    const item: DownloadItem = { id, input, status: 'queued' };
    setDownloads(prev => [item, ...prev]);
    await invoke('start_download', { id, input });
  }, []);

  return { downloads, activeProgress, startDownload };
}
```

- [ ] **Step 3: Create `src/components/SearchBar.tsx`**

```tsx
import { useState, useRef, useEffect } from 'react';

interface Props { onSearch: (query: string) => void; onClose: () => void }

export function SearchBar({ onSearch, onClose }: Props) {
  const [query, setQuery] = useState('');
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => { inputRef.current?.focus(); }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter' && query.trim()) { onSearch(query.trim()); onClose(); }
    if (e.key === 'Escape') onClose();
  };

  return (
    <div className="search-bar">
      <input
        ref={inputRef}
        value={query}
        onChange={e => setQuery(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder="Search or paste URL…"
      />
    </div>
  );
}
```

- [ ] **Step 4: Create `src/components/QueueItem.tsx`**

```tsx
import { startDrag } from '@tauri-apps/plugin-drag';
import type { DownloadItem } from '../types';

const SOURCE_ICONS: Record<string, string> = {
  youtube: '▶', soundcloud: '☁', spotify: '♫', qobuz: '◈',
  bpmsupreme: 'B', clubkillers: 'C', livedjservice: 'L',
};

interface Props { item: DownloadItem }

export function QueueItem({ item }: Props) {
  const handleDragStart = async (e: React.DragEvent) => {
    if (!item.file_path) return;
    e.preventDefault();
    await startDrag({ items: [item.file_path] });
  };

  const statusLabel = {
    queued: '…', downloading: `${item.progress ?? 0}%`,
    analyzing: 'analyzing', done: '', error: '✕',
  }[item.status];

  return (
    <div className={`queue-item queue-item--${item.status}`} draggable={!!item.file_path} onDragStart={handleDragStart}>
      <span className="queue-item__source">{SOURCE_ICONS[item.source ?? ''] ?? '?'}</span>
      <span className="queue-item__name">{item.track_name ?? item.input}</span>
      {item.bpm ? <span className="queue-item__bpm">{item.bpm}</span> : null}
      {item.key ? <span className="queue-item__key">{item.key}</span> : null}
      <span className="queue-item__status">{statusLabel}</span>
    </div>
  );
}
```

- [ ] **Step 5: Create `src/components/QueuePanel.tsx`**

```tsx
import { QueueItem } from './QueueItem';
import type { DownloadItem } from '../types';

interface Props { items: DownloadItem[]; onClose: () => void }

export function QueuePanel({ items, onClose }: Props) {
  if (items.length === 0) return null;
  return (
    <div className="queue-panel">
      <div className="queue-panel__header">
        <span>Recent</span>
        <button onClick={onClose}>✕</button>
      </div>
      {items.map(item => <QueueItem key={item.id} item={item} />)}
    </div>
  );
}
```

- [ ] **Step 6: Update `src/App.tsx`**

```tsx
import './App.css';
import { useState } from 'react';
import { Circle } from './components/Circle';
import { SearchBar } from './components/SearchBar';
import { QueuePanel } from './components/QueuePanel';
import { useDownloads } from './hooks/useDownloads';

function App() {
  const { downloads, activeProgress, startDownload } = useDownloads();
  const [showSearch, setShowSearch] = useState(false);
  const [showQueue, setShowQueue] = useState(false);

  const handleDrop = (input: string) => startDownload(input);
  const handleSearch = (query: string) => startDownload(query);

  return (
    <div className="app-root">
      {showQueue && <QueuePanel items={downloads} onClose={() => setShowQueue(false)} />}
      {showSearch && <SearchBar onSearch={handleSearch} onClose={() => setShowSearch(false)} />}
      <Circle
        onDrop={handleDrop}
        progress={activeProgress}
        onClickBody={() => setShowQueue(v => !v)}
        onClickSearch={() => setShowSearch(v => !v)}
      />
    </div>
  );
}

export default App;
```

Update `src/components/Circle.tsx` to accept `onClickBody` and `onClickSearch` props:
```tsx
interface Props {
  onDrop: (input: string) => void;
  progress?: number;
  onClickBody?: () => void;
  onClickSearch?: () => void;
}
// Add onClick={onClickBody} to the circle div
// Add a magnifying glass button inside: <button className="search-btn" onClick={e => { e.stopPropagation(); onClickSearch?.(); }}>🔍</button>
```

- [ ] **Step 7: Add queue + search CSS to `App.css`**

```css
.app-root { position: relative; width: 72px; }

.search-bar {
  position: absolute; bottom: 80px; left: -64px;
  background: #1a1a2e; border-radius: 8px; padding: 4px 8px;
  width: 200px; z-index: 10;
}
.search-bar input {
  background: none; border: none; outline: none;
  color: white; font-size: 13px; width: 100%;
}

.queue-panel {
  position: absolute; bottom: 80px; left: -64px;
  background: #1a1a2e; border-radius: 8px;
  width: 240px; max-height: 320px; overflow-y: auto;
  padding: 8px; z-index: 10;
}
.queue-panel__header {
  display: flex; justify-content: space-between; align-items: center;
  color: #888; font-size: 11px; margin-bottom: 6px;
}
.queue-panel__header button { background: none; border: none; color: #888; cursor: pointer; }

.queue-item {
  display: flex; align-items: center; gap: 6px;
  padding: 4px 2px; font-size: 12px; color: white;
  border-radius: 4px; cursor: grab;
}
.queue-item--error { color: #f87171; }
.queue-item__name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.queue-item__bpm, .queue-item__key { color: #4ade80; font-size: 11px; }
.queue-item__status { color: #888; font-size: 11px; }

.search-btn {
  position: absolute; bottom: 4px; right: 4px;
  background: none; border: none; color: #888; font-size: 14px;
  cursor: pointer; padding: 2px; border-radius: 50%;
  -webkit-app-region: no-drag;
}
```

- [ ] **Step 8: Write component tests**

Create `src/components/QueueItem.test.tsx`:
```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { QueueItem } from './QueueItem';
import type { DownloadItem } from '../types';

const done: DownloadItem = {
  id: '1', input: 'https://youtube.com/watch?v=x',
  track_name: 'HUMBLE.', file_path: '/music/humble.mp3',
  source: 'youtube', bpm: 150, key: 'Am', status: 'done',
};

describe('QueueItem', () => {
  it('renders track name', () => {
    render(<QueueItem item={done} />);
    expect(screen.getByText('HUMBLE.')).toBeTruthy();
  });

  it('renders BPM and key', () => {
    render(<QueueItem item={done} />);
    expect(screen.getByText('150')).toBeTruthy();
    expect(screen.getByText('Am')).toBeTruthy();
  });

  it('shows input when no track_name yet', () => {
    const queued: DownloadItem = { id: '2', input: 'search query', status: 'queued' };
    render(<QueueItem item={queued} />);
    expect(screen.getByText('search query')).toBeTruthy();
  });
});
```

- [ ] **Step 9: Run tests**

```bash
npm run test -- --run
```
Expected: 6 tests pass (3 Circle + 3 QueueItem).

- [ ] **Step 10: Run dev app and test drop + queue**

```bash
npm run tauri dev
```
- Drag a YouTube URL from a browser onto the circle → should start downloading
- Circle shows progress ring
- Click circle → queue panel appears with download item
- When done, BPM and key should populate (requires aubio + keyfinder-cli installed)

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "feat: UI — circle drop target, search bar, queue panel with native drag-out"
```

---

### Task 8: Settings panel

**Files:**
- Create: `src/components/SettingsPanel.tsx`
- Modify: `src/App.tsx` (add right-click context menu trigger)
- Modify: `src/App.css`

**Interfaces:**
- Consumes: `useConfig()` hook
- Produces: floating settings card with all configurable options

- [ ] **Step 1: Create `src/components/SettingsPanel.tsx`**

```tsx
import { useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useConfig } from '../hooks/useConfig';
import type { Config } from '../types';

interface Props { onClose: () => void }

export function SettingsPanel({ onClose }: Props) {
  const { config, saveConfig } = useConfig();
  const [saving, setSaving] = useState(false);

  if (!config) return <div className="settings-panel">Loading…</div>;

  const update = async (patch: Partial<Config>) => {
    setSaving(true);
    await saveConfig({ ...config, ...patch });
    setSaving(false);
  };

  const handleCredentialSave = async (pool: string) => {
    const username = (document.getElementById(`${pool}-user`) as HTMLInputElement)?.value;
    const password = (document.getElementById(`${pool}-pass`) as HTMLInputElement)?.value;
    if (!username || !password) return;
    await invoke('save_credential', { pool, username, password });
  };

  const pools = ['bpmsupreme', 'clubkillers', 'livedjservice', 'qobuz'] as const;

  return (
    <div className="settings-panel">
      <div className="settings-panel__header">
        <span>Settings</span>
        <button onClick={onClose}>✕</button>
      </div>

      <label>Output folder
        <input value={config.output_dir} onChange={e => update({ output_dir: e.target.value })} />
      </label>

      <label>
        <input type="checkbox" checked={config.auto_download}
          onChange={e => update({ auto_download: e.target.checked })} />
        Auto-download
      </label>

      <label>Search behavior
        <select value={config.search_mode} onChange={e => update({ search_mode: e.target.value as Config['search_mode'] })}>
          <option value="best_match">Best match</option>
          <option value="show_results">Show results</option>
        </select>
      </label>

      <label>Search source priority
        <select value={config.search_priority} onChange={e => update({ search_priority: e.target.value as Config['search_priority'] })}>
          <option value="pools">Pools first</option>
          <option value="youtube">YouTube first</option>
        </select>
      </label>

      <hr />
      <p className="settings-panel__section">Credentials — paste op:// references or direct values</p>

      {pools.map(pool => {
        const refs = config.pools[pool];
        return (
          <div key={pool} className="settings-panel__pool">
            <strong>{pool}</strong>
            <input id={`${pool}-user`} defaultValue={refs.username_ref} placeholder="op://Vault/Item/username" />
            <input id={`${pool}-pass`} defaultValue={refs.password_ref} placeholder="op://Vault/Item/password" type="password" />
            <button onClick={() => {
              const u = (document.getElementById(`${pool}-user`) as HTMLInputElement)?.value;
              const p = (document.getElementById(`${pool}-pass`) as HTMLInputElement)?.value;
              update({ pools: { ...config.pools, [pool]: { username_ref: u, password_ref: p } } });
            }}>Save ref</button>
            <button onClick={() => handleCredentialSave(pool)}>Save to keychain</button>
          </div>
        );
      })}
      {saving && <span className="settings-panel__saving">Saving…</span>}
    </div>
  );
}
```

- [ ] **Step 2: Add right-click handler + settings to `App.tsx`**

```tsx
// Add to App.tsx state
const [showSettings, setShowSettings] = useState(false);

// Add right-click on the outer wrapper
<div className="app-root" onContextMenu={e => { e.preventDefault(); setShowSettings(v => !v); }}>
  {showSettings && <SettingsPanel onClose={() => setShowSettings(false)} />}
  ...
</div>
```

- [ ] **Step 3: Add settings CSS to `App.css`**

```css
.settings-panel {
  position: absolute; bottom: 80px; left: -120px;
  background: #1a1a2e; border-radius: 10px;
  padding: 12px; width: 280px; z-index: 20;
  color: white; font-size: 12px;
  box-shadow: 0 4px 20px rgba(0,0,0,0.5);
}
.settings-panel__header {
  display: flex; justify-content: space-between; margin-bottom: 10px;
  font-weight: 600;
}
.settings-panel__header button { background: none; border: none; color: #888; cursor: pointer; }
.settings-panel label { display: flex; flex-direction: column; gap: 3px; margin-bottom: 8px; }
.settings-panel input, .settings-panel select {
  background: #0f0f1a; border: 1px solid #333; border-radius: 4px;
  color: white; padding: 4px 6px; font-size: 11px;
}
.settings-panel__pool { margin-bottom: 10px; }
.settings-panel__pool strong { display: block; margin-bottom: 4px; color: #4ade80; }
.settings-panel__pool input { width: 100%; margin-bottom: 3px; box-sizing: border-box; }
.settings-panel__pool button { margin-right: 4px; background: #333; border: none; color: white; padding: 3px 8px; border-radius: 4px; cursor: pointer; font-size: 10px; }
.settings-panel__section { color: #888; margin: 4px 0; }
.settings-panel__saving { color: #4ade80; font-size: 10px; }
hr { border-color: #333; margin: 8px 0; }
```

- [ ] **Step 4: Write settings panel test**

Create `src/components/SettingsPanel.test.tsx`:
```tsx
import { render, screen } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { SettingsPanel } from './SettingsPanel';

vi.mocked(invoke).mockResolvedValue({
  output_dir: 'C:/Music/djdrop', auto_download: true,
  search_mode: 'best_match', search_priority: 'pools',
  pools: {
    bpmsupreme: { username_ref: '', password_ref: '' },
    clubkillers: { username_ref: '', password_ref: '' },
    livedjservice: { username_ref: '', password_ref: '' },
    qobuz: { username_ref: '', password_ref: '' },
  },
});

describe('SettingsPanel', () => {
  it('renders pool credential fields', async () => {
    render(<SettingsPanel onClose={() => {}} />);
    // Wait for config load
    await screen.findByText('bpmsupreme');
    expect(screen.getByText('clubkillers')).toBeTruthy();
    expect(screen.getByText('livedjservice')).toBeTruthy();
    expect(screen.getByText('qobuz')).toBeTruthy();
  });
});
```

- [ ] **Step 5: Run all tests**

```bash
npm run test -- --run
```
Expected: 7+ tests pass.

- [ ] **Step 6: Run dev app and verify settings**

```bash
npm run tauri dev
```
Right-click the circle → settings panel appears. Change output folder, toggle auto-download, enter op:// references. Click "Save ref" — verify `config.toml` updates in `%APPDATA%\djdrop\`.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat: settings panel — output dir, auto-download, search mode, credentials"
```

---

## What's Next

**Plan 2 — Record Pool Sidecar** covers:
- Node.js + Playwright sidecar scaffold (stdio JSON protocol)
- BPM Supreme scraper (login, search, scrape BPM/key, download)
- Club Killers scraper
- LiveDJService scraper
- Security audit of the sidecar module (credential flow, process isolation, log redaction)

The core app (this plan) already handles pool URL drops and searches with a graceful `"pool sidecar not connected"` error until Plan 2 is complete.
