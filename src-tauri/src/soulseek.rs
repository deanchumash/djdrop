//! Soulseek as a download source.
//!
//! A bundled, headless slskd runs on loopback with the user's Soulseek login
//! and shares nothing. We search for the Spotify track, rank the results by
//! verified title/artist/duration and audio quality (FLAC, then 320 kbps MP3),
//! and download the best few from their peers.

use crate::matcher::{contains_phrase, core_title, normalize, TrackMeta};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// Same slack as the YouTube matcher: Spotify reports whole seconds.
const DURATION_TOLERANCE_SECS: f64 = 3.0;
/// How many ranked candidates to try before giving up on Soulseek.
const MAX_ATTEMPTS: usize = 3;
/// Peers often queue requests behind other users; don't wait forever for one.
const QUEUE_PATIENCE: Duration = Duration::from_secs(60);
const STALL_LIMIT: Duration = Duration::from_secs(60);
const TRANSFER_LIMIT: Duration = Duration::from_secs(15 * 60);
const IDLE_SHUTDOWN: Duration = Duration::from_secs(10 * 60);

struct Slskd {
    child: Child,
    port: u16,
    username: String,
    password: String,
    last_used: Instant,
}

static SESSION: Mutex<Option<Slskd>> = Mutex::const_new(None);

// ── credentials ──────────────────────────────────────────────────────────────

/// The user's Soulseek login: refs from Settings (literal or `op://`), else the
/// OS keychain entry saved by "Save to keychain". `Ok(None)` = not configured.
pub async fn credentials(app: &AppHandle) -> Result<Option<(String, String)>, String> {
    let cfg = crate::config::read(app).map_err(|e| e.to_string())?;
    let refs = &cfg.pools.soulseek;
    if !refs.username_ref.is_empty() || !refs.password_ref.is_empty() {
        let (u, p) = crate::credentials::resolve(&refs.username_ref, &refs.password_ref).await?;
        return Ok((!u.is_empty() && !p.is_empty()).then_some((u, p)));
    }
    Ok(crate::credentials::read_from_keychain("soulseek").ok().filter(|(u, p)| !u.is_empty() && !p.is_empty()))
}

// ── process management ───────────────────────────────────────────────────────

/// Stop slskd when djdrop exits.
pub fn shutdown() {
    if let Ok(mut guard) = SESSION.try_lock() {
        if let Some(s) = guard.as_mut() { let _ = s.child.start_kill(); }
    }
}

/// A port the OS will actually let us bind. Windows reserves large dynamic
/// ranges (Hyper-V/WSL), so fixed defaults like slskd's 50300 can be forbidden.
fn free_port(ip: &str) -> Result<u16, String> {
    let listener = std::net::TcpListener::bind((ip, 0)).map_err(|e| e.to_string())?;
    listener.local_addr().map(|a| a.port()).map_err(|e| e.to_string())
}

fn api(port: u16) -> String { format!("http://127.0.0.1:{port}/api/v0") }

fn http() -> Result<reqwest::Client, String> {
    // Loopback only: never route through a system proxy.
    reqwest::Client::builder().no_proxy().timeout(Duration::from_secs(30)).build().map_err(|e| e.to_string())
}

async fn touch(port: u16) {
    if let Some(s) = SESSION.lock().await.as_mut().filter(|s| s.port == port) { s.last_used = Instant::now(); }
}

async fn ensure_running(app: &AppHandle, username: &str, password: &str) -> Result<u16, String> {
    let mut guard = SESSION.lock().await;
    if let Some(s) = guard.as_mut() {
        let alive = matches!(s.child.try_wait(), Ok(None));
        if alive && s.username == username && s.password == password {
            s.last_used = Instant::now();
            return Ok(s.port);
        }
        let _ = s.child.kill().await;
    }
    *guard = None;
    let session = spawn(app, username, password).await?;
    let port = session.port;
    *guard = Some(session);
    tokio::spawn(reap_when_idle(port));
    Ok(port)
}

async fn reap_when_idle(port: u16) {
    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
        let mut guard = SESSION.lock().await;
        match guard.as_mut() {
            Some(s) if s.port == port => {
                let dead = !matches!(s.child.try_wait(), Ok(None));
                if dead || s.last_used.elapsed() > IDLE_SHUTDOWN {
                    let _ = s.child.kill().await;
                    *guard = None;
                    return;
                }
            }
            _ => return,
        }
    }
}

async fn spawn(app: &AppHandle, username: &str, password: &str) -> Result<Slskd, String> {
    let exe = crate::binaries::slskd(app)?;
    if !exe.exists() {
        return Err(format!("slskd not found at {exe:?}; run scripts/fetch-binaries.ps1"));
    }
    let data = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("slskd");
    let downloads = data.join("downloads");
    let incomplete = data.join("incomplete");
    for dir in [&downloads, &incomplete] { std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; }
    kill_stale(&data).await;

    let http_port = free_port("127.0.0.1")?;
    let listen_port = free_port("0.0.0.0")?;
    let log_path = data.join("slskd.log");
    let log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;

    let mut cmd = Command::new(&exe);
    cmd.env("SLSKD_APP_DIR", &data)
        .env("SLSKD_DOWNLOADS_DIR", &downloads)
        .env("SLSKD_INCOMPLETE_DIR", &incomplete)
        .env("SLSKD_SLSK_USERNAME", username)
        .env("SLSKD_SLSK_PASSWORD", password)
        .env("SLSKD_SLSK_LISTEN_PORT", listen_port.to_string())
        .env("SLSKD_HTTP_PORT", http_port.to_string())
        .env("SLSKD_HTTP_IP_ADDRESS", "127.0.0.1")
        .env("SLSKD_NO_HTTPS", "true")
        .env("SLSKD_NO_AUTH", "true")
        .env("SLSKD_HEADLESS", "true")
        .env("SLSKD_NO_VERSION_CHECK", "true")
        .env("SLSKD_NO_SHARE_SCAN", "true")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stderr(Stdio::from(log))
        .kill_on_drop(true);
    #[cfg(windows)] cmd.creation_flags(CREATE_NO_WINDOW);
    let mut child = cmd.spawn().map_err(|e| format!("failed to start slskd: {e}"))?;
    if let Some(pid) = child.id() { let _ = std::fs::write(data.join("slskd.pid"), pid.to_string()); }

    let client = http()?;
    let base = api(http_port);
    let started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("slskd exited ({status}): {}", log_hint(&log_path)));
        }
        if client.get(format!("{base}/application")).send().await.map(|r| r.status().is_success()).unwrap_or(false) { break; }
        if started.elapsed() > Duration::from_secs(30) { return Err("slskd did not start within 30 s".into()); }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    let login_started = Instant::now();
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            return Err(format!("slskd exited ({status}): {}", log_hint(&log_path)));
        }
        let state: Value = match client.get(format!("{base}/server")).send().await {
            Ok(r) => r.json().await.unwrap_or_default(),
            Err(_) => Value::Null,
        };
        if state["isLoggedIn"].as_bool() == Some(true) { break; }
        let hint = log_hint(&log_path);
        if hint.contains("LoginRejected") || hint.to_lowercase().contains("invalid username or password") {
            return Err("Soulseek rejected the username/password".into());
        }
        if login_started.elapsed() > Duration::from_secs(45) {
            return Err(format!("Soulseek login did not complete in 45 s: {hint}"));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    Ok(Slskd { child, port: http_port, username: username.into(), password: password.into(), last_used: Instant::now() })
}

/// An slskd left behind by a crashed djdrop would keep running forever.
async fn kill_stale(data: &Path) {
    let pid_file = data.join("slskd.pid");
    let Ok(pid) = std::fs::read_to_string(&pid_file) else { return };
    let pid = pid.trim();
    if pid.parse::<u32>().is_ok() {
        // The image-name filter guarantees a recycled PID never kills another program.
        let mut cmd = Command::new("taskkill");
        cmd.args(["/F", "/PID", pid, "/FI", "IMAGENAME eq slskd.exe"]);
        #[cfg(windows)] cmd.creation_flags(CREATE_NO_WINDOW);
        let _ = cmd.output().await;
    }
    let _ = std::fs::remove_file(pid_file);
}

/// The last error-looking line of slskd's log, for actionable messages.
fn log_hint(log: &Path) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    text.lines().rev()
        .find(|l| l.contains("ERR") || l.contains("WRN") || l.contains("FTL") || l.contains("Exception"))
        .or_else(|| text.lines().rev().find(|l| !l.trim().is_empty()))
        .unwrap_or("no log output").trim().to_string()
}

// ── ranking (pure) ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct Pick {
    pub username: String,
    pub filename: String,
    pub size: u64,
    pub extension: String,
}

/// Words that mark a different recording of the same song. Only the file name
/// is checked, and only when the Spotify title doesn't contain the word.
const OTHER_VERSION_WORDS: &[&str] = &[
    "instrumental", "karaoke", "acapella", "a cappella", "remix", "live",
    "cover", "sped up", "slowed", "reverb", "nightcore", "8d",
];

/// Rank order: FLAC, then 320 kbps CBR MP3, then high-bitrate VBR/256 kbps MP3.
/// Anything lossy below that is not worth taking over a YouTube match.
fn quality(file: &Value, extension: &str) -> Option<u32> {
    match extension {
        "flac" => Some(3000),
        "mp3" => {
            let kbps = file["bitRate"].as_u64()?;
            if file["isVariableBitRate"].as_bool() == Some(true) {
                (kbps >= 220).then_some(1800)
            } else if kbps >= 320 {
                Some(2000)
            } else {
                (kbps >= 256).then_some(1500)
            }
        }
        _ => None,
    }
}

/// Files whose name carries the track title and whose path names the lead
/// artist, with a length matching Spotify's, best first.
pub fn rank_files(meta: &TrackMeta, responses: &Value) -> Vec<Pick> {
    let Some(artist) = meta.artists.first().map(|a| normalize(a)) else { return Vec::new() };
    let title = core_title(&meta.name);
    let full_title = normalize(&meta.name);

    let mut scored: Vec<((u32, bool, i64, i64), Pick)> = Vec::new();
    for response in responses.as_array().into_iter().flatten() {
        let Some(username) = response["username"].as_str() else { continue };
        let free_slot = response["hasFreeUploadSlot"].as_bool().unwrap_or(false);
        let queue = response["queueLength"].as_i64().unwrap_or(i64::MAX / 2);
        let speed = response["uploadSpeed"].as_i64().unwrap_or(0);
        for file in response["files"].as_array().into_iter().flatten() {
            let Some(filename) = file["filename"].as_str() else { continue };
            let extension = file["extension"].as_str().map(|e| e.trim_start_matches('.').to_lowercase())
                .or_else(|| Path::new(&filename.replace('\\', "/")).extension().map(|e| e.to_string_lossy().to_lowercase()))
                .unwrap_or_default();
            let Some(score) = quality(file, &extension) else { continue };
            let Some(length) = file["length"].as_f64() else { continue };
            if (length - meta.duration_secs).abs() > DURATION_TOLERANCE_SECS { continue; }

            let path = normalize(filename);
            if !contains_phrase(&path, &title) || !contains_phrase(&path, &artist) { continue; }
            let base = normalize(filename.rsplit(['\\', '/']).next().unwrap_or(filename));
            let other_version = OTHER_VERSION_WORDS.iter()
                .any(|w| contains_phrase(&base, w) && !contains_phrase(&full_title, w));
            if other_version { continue; }

            scored.push((
                (score, free_slot, -queue, speed),
                Pick {
                    username: username.to_string(),
                    filename: filename.to_string(),
                    size: file["size"].as_u64().unwrap_or(0),
                    extension,
                },
            ));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, p)| p).collect()
}

/// The best file from each of the best peers, so a stuck peer doesn't block the rest.
fn top_candidates(ranked: Vec<Pick>) -> Vec<Pick> {
    let mut seen = std::collections::HashSet::new();
    ranked.into_iter().filter(|p| seen.insert(p.username.clone())).take(MAX_ATTEMPTS).collect()
}

/// Soulseek.NET serializes TransferStates as "Completed, Succeeded"; accept
/// the numeric flag form too.
fn state_text(state: &Value) -> String {
    if let Some(s) = state.as_str() { return s.to_string(); }
    let Some(n) = state.as_u64() else { return String::new() };
    const FLAGS: [(u64, &str); 13] = [
        (1, "Requested"), (2, "Queued"), (4, "Initializing"), (8, "InProgress"), (16, "Completed"),
        (32, "Succeeded"), (64, "Cancelled"), (128, "TimedOut"), (256, "Errored"), (512, "Rejected"),
        (1024, "Aborted"), (2048, "Locally"), (4096, "Remotely"),
    ];
    FLAGS.iter().filter(|(bit, _)| n & bit != 0).map(|(_, name)| *name).collect::<Vec<_>>().join(", ")
}

/// The transfer object for `filename`, wherever the response nests it.
fn find_transfer<'a>(value: &'a Value, filename: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) => {
            if map.get("filename").and_then(Value::as_str) == Some(filename) && map.contains_key("state") {
                return Some(value);
            }
            map.values().find_map(|v| find_transfer(v, filename))
        }
        Value::Array(items) => items.iter().find_map(|v| find_transfer(v, filename)),
        _ => None,
    }
}

// ── download ─────────────────────────────────────────────────────────────────

/// Search Soulseek for the track and download the best verified match into
/// `output_dir`. Returns the final file path.
pub async fn download(
    app: &AppHandle,
    id: &str,
    login: &(String, String),
    meta: &TrackMeta,
    output_dir: &str,
) -> Result<String, String> {
    let port = ensure_running(app, &login.0, &login.1).await?;
    let client = http()?;
    let base = api(port);

    let query = format!("{} {}", normalize(meta.artists.first().map(String::as_str).unwrap_or("")), core_title(&meta.name));
    let search_id = uuid::Uuid::new_v4().to_string();
    let created = client.post(format!("{base}/searches"))
        .json(&json!({
            "id": search_id, "searchText": query, "searchTimeout": 15000,
            "filterResponses": true, "minimumResponseFileCount": 1,
            "responseLimit": 200, "fileLimit": 10000,
        }))
        .send().await.map_err(|e| format!("slskd search failed: {e}"))?;
    if !created.status().is_success() {
        return Err(format!("slskd rejected the search: {}", created.text().await.unwrap_or_default().trim()));
    }

    let started = Instant::now();
    loop {
        touch(port).await;
        let state: Value = client.get(format!("{base}/searches/{search_id}")).send().await
            .map_err(|e| e.to_string())?.json().await.unwrap_or_default();
        if state["isComplete"].as_bool() == Some(true) || started.elapsed() > Duration::from_secs(30) { break; }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    let responses: Value = client.get(format!("{base}/searches/{search_id}/responses")).send().await
        .map_err(|e| e.to_string())?.json().await.unwrap_or_default();
    let _ = client.delete(format!("{base}/searches/{search_id}")).send().await;

    let candidates = top_candidates(rank_files(meta, &responses));
    if candidates.is_empty() {
        return Err("no peer shared a FLAC or 320 kbps MP3 matching the title, artist and duration".into());
    }

    let downloads = app.path().app_local_data_dir().map_err(|e| e.to_string())?.join("slskd").join("downloads");
    let mut failures = Vec::new();
    for pick in candidates {
        match fetch(app, id, &client, &base, port, &pick, &downloads).await {
            Ok(downloaded) => return place(meta, &downloaded, &pick.extension, Path::new(output_dir)),
            Err(e) => failures.push(format!("{}: {e}", pick.username)),
        }
    }
    Err(failures.join("; "))
}

async fn cancel(client: &reqwest::Client, base: &str, username: &str, transfer: Option<&Value>) {
    if let Some(tid) = transfer.and_then(|t| t["id"].as_str()) {
        let _ = client.delete(format!("{base}/transfers/downloads/{username}/{tid}?remove=true")).send().await;
    }
}

/// Queue one file with its peer and wait for it to land in slskd's downloads folder.
async fn fetch(
    app: &AppHandle,
    id: &str,
    client: &reqwest::Client,
    base: &str,
    port: u16,
    pick: &Pick,
    downloads: &Path,
) -> Result<PathBuf, String> {
    let queued = client.post(format!("{base}/transfers/downloads/{}", pick.username))
        .json(&json!([{ "filename": pick.filename, "size": pick.size }]))
        .send().await.map_err(|e| e.to_string())?;
    if !queued.status().is_success() {
        return Err(format!("slskd refused the download ({})", queued.status()));
    }

    let started = Instant::now();
    let mut queued_since: Option<Instant> = None;
    let mut last_bytes = 0u64;
    let mut last_movement = Instant::now();
    let mut last_percent: Option<u8> = None;
    let finished_id: Option<String>;
    loop {
        tokio::time::sleep(Duration::from_secs(2)).await;
        touch(port).await;
        let listing: Value = match client.get(format!("{base}/transfers/downloads/{}", pick.username)).send().await {
            Ok(r) => r.json().await.unwrap_or_default(),
            Err(_) => Value::Null,
        };
        let transfer = find_transfer(&listing, &pick.filename);
        let Some(t) = transfer else {
            if started.elapsed() > Duration::from_secs(20) { return Err("transfer disappeared from slskd".into()); }
            continue;
        };
        let state = state_text(&t["state"]);

        if state.contains("Succeeded") {
            finished_id = t["id"].as_str().map(str::to_string);
            break;
        }
        if state.contains("Completed") {
            cancel(client, base, &pick.username, transfer).await;
            return Err(format!("transfer ended: {state}"));
        }
        if started.elapsed() > TRANSFER_LIMIT {
            cancel(client, base, &pick.username, transfer).await;
            return Err("transfer took longer than 15 minutes".into());
        }
        if state.contains("Queued") || state.contains("Requested") {
            let since = *queued_since.get_or_insert_with(Instant::now);
            if since.elapsed() > QUEUE_PATIENCE {
                cancel(client, base, &pick.username, transfer).await;
                return Err("still queued by the peer after 60 s".into());
            }
            continue;
        }
        queued_since = None;

        let bytes = t["bytesTransferred"].as_u64().unwrap_or(0);
        if bytes != last_bytes { last_bytes = bytes; last_movement = Instant::now(); }
        if last_movement.elapsed() > STALL_LIMIT {
            cancel(client, base, &pick.username, transfer).await;
            return Err("transfer stalled for 60 s".into());
        }
        if let Some(pct) = t["percentComplete"].as_f64() {
            let pct = pct.clamp(0.0, 100.0) as u8;
            if last_percent != Some(pct) {
                last_percent = Some(pct);
                crate::downloader::emit_progress(app, id, pct);
            }
        }
    }

    let basename = pick.filename.rsplit(['\\', '/']).next().unwrap_or(&pick.filename);
    let found = find_downloaded(downloads, basename)
        .ok_or_else(|| format!("slskd reported success but {basename} is not in its downloads folder"))?;
    // Drop slskd's record of the transfer; the file itself is moved by the caller.
    if let Some(tid) = finished_id {
        let _ = client.delete(format!("{base}/transfers/downloads/{}/{tid}?remove=true", pick.username)).send().await;
    }
    Ok(found)
}

/// Newest file named `basename` anywhere under `dir`.
fn find_downloaded(dir: &Path, basename: &str) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() {
                stack.push(path);
            } else if path.file_name().map(|n| n.to_string_lossy() == basename).unwrap_or(false) {
                let modified = entry.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                if best.as_ref().map(|(t, _)| modified > *t).unwrap_or(true) { best = Some((modified, path)); }
            }
        }
    }
    best.map(|(_, p)| p)
}

fn sanitize(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    cleaned.trim().trim_end_matches('.').to_string()
}

/// Move the finished file out of slskd's folder as "Artist, Artist - Title.ext",
/// never overwriting an existing file.
fn place(meta: &TrackMeta, downloaded: &Path, extension: &str, output_dir: &Path) -> Result<String, String> {
    let stem = sanitize(&format!("{} - {}", meta.artists.join(", "), meta.name));
    let mut target = output_dir.join(format!("{stem}.{extension}"));
    let mut n = 2;
    while target.exists() {
        target = output_dir.join(format!("{stem} ({n}).{extension}"));
        n += 1;
    }
    if std::fs::rename(downloaded, &target).is_err() {
        std::fs::copy(downloaded, &target).map_err(|e| e.to_string())?;
        let _ = std::fs::remove_file(downloaded);
    }
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta() -> TrackMeta {
        TrackMeta { name: "Remedy".into(), artists: vec!["Lizzy Ashliegh".into(), "Jared Brady".into()], duration_secs: 156.0 }
    }
    fn file(name: &str, ext: &str, kbps: u64, vbr: bool, secs: f64) -> Value {
        json!({"filename": name, "extension": ext, "bitRate": kbps, "isVariableBitRate": vbr, "length": secs, "size": 1000})
    }
    fn peer(user: &str, free: bool, queue: i64, files: Vec<Value>) -> Value {
        json!({"username": user, "hasFreeUploadSlot": free, "queueLength": queue, "uploadSpeed": 500, "files": files})
    }

    #[test]
    fn prefers_flac_then_320_and_refuses_other_recordings() {
        let responses = json!([
            peer("mp3peer", true, 0, vec![file(r"@@m\Music\Lizzy Ashliegh\Remedy.mp3", "mp3", 320, false, 156.2)]),
            peer("flacpeer", false, 40, vec![file(r"@@f\flac\Lizzy Ashliegh - Remedy.flac", "flac", 0, false, 156.0)]),
            peer("lowpeer", true, 0, vec![file(r"@@l\Lizzy Ashliegh - Remedy.mp3", "mp3", 192, false, 156.0)]),
            peer("wrongsong", true, 0, vec![file(r"@@w\Lizzy Ashliegh - Fuck Shit.mp3", "mp3", 320, false, 156.0)]),
            peer("wronglength", true, 0, vec![file(r"@@x\Lizzy Ashliegh - Remedy.flac", "flac", 0, false, 97.0)]),
            peer("remix", true, 0, vec![file(r"@@r\Lizzy Ashliegh - Remedy (Club Remix).flac", "flac", 0, false, 156.0)]),
            peer("nolength", true, 0, vec![json!({"filename": r"@@n\Lizzy Ashliegh - Remedy.flac", "extension": "flac"})]),
        ]);
        let ranked = rank_files(&meta(), &responses);
        let users: Vec<_> = ranked.iter().map(|p| p.username.as_str()).collect();
        assert_eq!(users, ["flacpeer", "mp3peer"]);
        assert_eq!(ranked[0].extension, "flac");
    }

    #[test]
    fn among_equal_quality_prefers_free_slots_then_short_queues() {
        let f = |u: &str| file(&format!(r"@@{u}\Lizzy Ashliegh - Remedy.mp3"), "mp3", 320, false, 156.0);
        let responses = json!([
            peer("busy", false, 0, vec![f("busy")]),
            peer("free_long", true, 30, vec![f("free_long")]),
            peer("free_short", true, 2, vec![f("free_short")]),
        ]);
        let users: Vec<_> = rank_files(&meta(), &responses).into_iter().map(|p| p.username).collect();
        assert_eq!(users, ["free_short", "free_long", "busy"]);
    }

    #[test]
    fn vbr_and_256_are_accepted_but_ranked_below_320() {
        let responses = json!([
            peer("v0", true, 0, vec![file(r"@@a\Lizzy Ashliegh\Remedy.mp3", "mp3", 245, true, 156.0)]),
            peer("cbr256", true, 0, vec![file(r"@@b\Lizzy Ashliegh\Remedy.mp3", "mp3", 256, false, 156.0)]),
            peer("cbr320", false, 9, vec![file(r"@@c\Lizzy Ashliegh\Remedy.mp3", "mp3", 320, false, 156.0)]),
            peer("vbr128", true, 0, vec![file(r"@@d\Lizzy Ashliegh\Remedy.mp3", "mp3", 128, true, 156.0)]),
        ]);
        let users: Vec<_> = rank_files(&meta(), &responses).into_iter().map(|p| p.username).collect();
        assert_eq!(users, ["cbr320", "v0", "cbr256"]);
    }

    #[test]
    fn version_words_only_count_when_the_spotify_title_lacks_them() {
        let live = TrackMeta { name: "Remedy (Live)".into(), ..meta() };
        let responses = json!([peer("p", true, 0, vec![file(r"@@a\Lizzy Ashliegh - Remedy (Live).flac", "flac", 0, false, 156.0)])]);
        assert_eq!(rank_files(&live, &responses).len(), 1);
        assert!(rank_files(&meta(), &responses).is_empty());
    }

    #[test]
    fn only_the_best_file_per_peer_and_at_most_three_peers() {
        let f = |u: &str, ext: &str, kbps| file(&format!(r"@@{u}\Lizzy Ashliegh - Remedy.{ext}"), ext, kbps, false, 156.0);
        let responses = json!([
            peer("a", true, 0, vec![f("a", "mp3", 320), f("a", "flac", 0)]),
            peer("b", true, 0, vec![f("b", "flac", 0)]),
            peer("c", true, 0, vec![f("c", "flac", 0)]),
            peer("d", true, 0, vec![f("d", "flac", 0)]),
        ]);
        let picked = top_candidates(rank_files(&meta(), &responses));
        assert_eq!(picked.len(), 3);
        assert!(picked.iter().all(|p| p.extension == "flac"));
        assert_eq!(picked.iter().filter(|p| p.username == "a").count(), 1);
    }

    #[test]
    fn reads_transfer_state_and_finds_nested_transfer() {
        assert_eq!(state_text(&json!("Completed, Succeeded")), "Completed, Succeeded");
        assert_eq!(state_text(&json!(16 | 32)), "Completed, Succeeded");
        assert_eq!(state_text(&json!(2 | 4096)), "Queued, Remotely");

        let listing = json!({"username": "u", "directories": [{"directory": "d", "files": [
            {"id": "x", "filename": r"@@u\a.flac", "state": "InProgress", "percentComplete": 40.0},
            {"id": "y", "filename": r"@@u\b.flac", "state": "Queued, Remotely"}]}]});
        assert_eq!(find_transfer(&listing, r"@@u\b.flac").unwrap()["id"], "y");
        assert!(find_transfer(&listing, r"@@u\c.flac").is_none());
    }

    #[test]
    fn places_files_with_a_clean_name_and_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("out");
        std::fs::create_dir(&out).unwrap();
        let nested = root.path().join("dl").join("album");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("01 remedy.flac"), b"one").unwrap();
        assert_eq!(find_downloaded(&root.path().join("dl"), "01 remedy.flac"), Some(nested.join("01 remedy.flac")));

        let meta = TrackMeta { name: "Re:medy?".into(), ..meta() };
        let first = place(&meta, &nested.join("01 remedy.flac"), "flac", &out).unwrap();
        assert_eq!(Path::new(&first).file_name().unwrap(), "Lizzy Ashliegh, Jared Brady - Re_medy_.flac");
        std::fs::write(nested.join("01 remedy.flac"), b"two").unwrap();
        let second = place(&meta, &nested.join("01 remedy.flac"), "flac", &out).unwrap();
        assert!(second.ends_with("Re_medy_ (2).flac"));
        assert_eq!(std::fs::read(first).unwrap(), b"one");
    }
}
