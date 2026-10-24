use crate::matcher::TrackMeta;
use crate::router::DownloadSource;
use serde::Serialize;
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Serialize)]
pub struct ProgressPayload { pub id: String, pub percent: u8 }
#[derive(Clone, Serialize)]
struct StartedPayload { id: String }
#[derive(Clone, Serialize)]
pub struct DonePayload {
    pub id: String,
    pub file_path: String,
    pub track_name: String,
    pub source: String,
    /// Upper-case file extension, e.g. "MP3" or "FLAC".
    pub format: String,
    /// Average bitrate in kbps as measured by ffprobe.
    pub bitrate: Option<u32>,
}
#[derive(Clone, Serialize)]
pub struct ErrorPayload { pub id: String, pub message: String }

pub fn ytdlp_args(url: &str, output_dir: &str, ffmpeg_dir: &str) -> Vec<String> {
    vec![
        "-x".into(), "--audio-format".into(), "mp3".into(),
        "--audio-quality".into(), "0".into(),
        "-P".into(), output_dir.into(),
        "--print".into(), "after_move:filepath".into(),
        "--progress".into(), "--newline".into(),
        "--no-playlist".into(),
        "--ffmpeg-location".into(), ffmpeg_dir.into(),
        url.into(),
    ]
}

pub fn spotdl_args(url: &str, output_dir: &str, ffmpeg: &str, providers: &[&str]) -> Vec<String> {
    let mut args = Vec::with_capacity(11 + providers.len());
    args.extend(["download".into(), url.into(), "--audio".into()]);
    args.extend(providers.iter().map(|provider| (*provider).to_string()));
    args.extend([
        "--output".into(), output_dir.into(),
        "--format".into(), "mp3".into(),
        "--bitrate".into(), "320k".into(),
        "--ffmpeg".into(), ffmpeg.into(),
    ]);
    args
}

pub fn qobuz_args(url: &str, output_dir: &str) -> Vec<String> {
    vec![url.into(), "-d".into(), output_dir.into()]
}

/// Let both yt-dlp and spotdl's embedded yt-dlp find the bundled Deno runtime.
fn bundled_tools_path(app: &AppHandle) -> Result<std::ffi::OsString, String> {
    let tools = crate::binaries::ffmpeg_dir(app)?;
    let current = std::env::var_os("PATH").unwrap_or_default();
    std::env::join_paths(std::iter::once(tools).chain(std::env::split_paths(&current)))
        .map_err(|e| e.to_string())
}

pub async fn run_download(app: AppHandle, id: String, source: DownloadSource, input: String, output_dir: String) {
    #[cfg(windows)]
    crate::native_circle::download_started(&id);
    let _ = app.emit("download:started", StartedPayload { id: id.clone() });
    let result = match &source {
        DownloadSource::YtDlp => {
            run_ytdlp(&app, &id, &input, &output_dir).await
        }
        DownloadSource::Spotdl => {
            run_spotdl(&app, &id, &input, &output_dir).await
        }
        DownloadSource::QobuzDlp => {
            match crate::config::read(&app).map_err(|e| e.to_string()) {
                Err(e) => Err(e),
                Ok(cfg) => {
                    let fut = crate::credentials::resolve(
                        &cfg.pools.qobuz.username_ref,
                        &cfg.pools.qobuz.password_ref,
                    );
                    match fut.await {
                        Err(e) => Err(e),
                        Ok((username, password)) => {
                            run_qobuz(&app, &id, &input, &output_dir, &username, &password).await
                        }
                    }
                }
            }
        }
        DownloadSource::Pool(_) => {
            Err("pool sidecar not connected".to_string())
        }
        DownloadSource::Search(query) => {
            let prefix = crate::config::read(&app)
                .map(|cfg| matches!(cfg.search_priority, crate::config::SearchPriority::Soundcloud))
                .unwrap_or(false);
            let url = if prefix { format!("scsearch1:{}", query) } else { format!("ytsearch1:{}", query) };
            run_ytdlp(&app, &id, &url, &output_dir).await
        }
    };

    if let Err(msg) = result {
        #[cfg(windows)]
        crate::native_circle::download_finished(&id);
        let _ = app.emit("download:error", ErrorPayload { id, message: msg });
    }
}

async fn run_ytdlp(app: &AppHandle, id: &str, url: &str, output_dir: &str) -> Result<(), String> {
    // Check for SC purchase-link gate (gate.sc / hypeddit / dropbox / gdrive / direct file)
    // Only for direct SC URLs, not yt-dlp search prefixes
    if url.contains("soundcloud.com") {
        if let Some(gate_url) = crate::gate::check_sc_for_gate(url).await {
            return run_http_download(app, id, &gate_url, output_dir, "soundcloud").await;
        }
    }

    let ytdlp_bin = crate::binaries::ytdlp(app)?;
    let ffmpeg_dir = crate::binaries::ffmpeg_dir(app)?
        .to_string_lossy()
        .into_owned();
    let args = ytdlp_args(url, output_dir, &ffmpeg_dir);

    let mut child = {
        let mut cmd = Command::new(&ytdlp_bin);
        cmd.env("PATH", bundled_tools_path(app)?);
        cmd.args(&args).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(windows)] cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.spawn().map_err(|e| format!("yt-dlp not found at {:?}: {e}", ytdlp_bin))?
    };

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let app_clone = app.clone();
    let id_clone = id.to_string();

    // yt-dlp writes diagnostics to stderr; keep the actual provider error.
    let progress_task = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        let mut error = None;
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(pct) = parse_ytdlp_progress(&line) {
                emit_progress(&app_clone, &id_clone, pct);
            }
            if let Some(message) = line.strip_prefix("ERROR: ") {
                error = Some(message.to_string());
            }
        }
        error
    });

    // Read stdout for the filepath (--print after_move:filepath outputs one line)
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut file_path = String::new();
    while let Ok(Some(line)) = stdout_lines.next_line().await {
        if let Some(pct) = parse_ytdlp_progress(&line) {
            emit_progress(app, id, pct);
        } else if !line.trim().is_empty() {
            file_path = line.trim().to_string();
        }
    }

    let error = progress_task.await.map_err(|e| e.to_string())?;

    let status = child.wait().await.map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(error.unwrap_or_else(|| "yt-dlp exited with error".into()));
    }

    if file_path.is_empty() {
        return Err("yt-dlp did not report output filepath".into());
    }

    let source = if url.contains("soundcloud.com") || url.starts_with("scsearch") { "soundcloud" } else { "youtube" };
    finish_download(app, id, &file_path, Some(source)).await;
    Ok(())
}

async fn run_spotdl(app: &AppHandle, id: &str, url: &str, output_dir: &str) -> Result<(), String> {
    let spotdl_bin = crate::binaries::spotdl(app)?;
    let ffmpeg = crate::binaries::ffmpeg_dir(app)?.join(if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" });
    let tools_path = bundled_tools_path(app)?;
    // spotDL can exit successfully despite a failed audio download. Isolate
    // each attempt so a partial or older MP3 cannot be reported as its result.
    // spotDL removes a leading dot from --output, so use a visible job directory.
    let job_dir = std::path::Path::new(output_dir).join(format!("djdrop-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&job_dir).map_err(|e| e.to_string())?;
    let result = async {
        let output_path = std::path::Path::new(output_dir);
        let mut failures: Vec<String> = Vec::new();

        // Soulseek first when the user is logged in: it is the only source that
        // yields FLAC / 320 kbps files rather than a re-encoded ~130 kbps stream.
        let login = match crate::soulseek::credentials(app).await {
            Ok(login) => login,
            Err(e) => { failures.push(format!("Soulseek: {e}")); None }
        };
        let mut meta: Option<TrackMeta> = None;
        if login.is_some() && url.contains("/track/") {
            match spotify_track_meta(&spotdl_bin, &tools_path, url, &job_dir).await {
                Ok(m) => meta = Some(m),
                Err(e) => failures.push(format!("Soulseek: {e}")),
            }
        }

        let (file_path, source) = 'found: {
            if let (Some(login), Some(meta)) = (&login, &meta) {
                match crate::soulseek::download(app, id, login, meta, output_dir).await {
                    Ok(path) => break 'found (path, Some("soulseek")),
                    Err(e) => failures.push(format!("Soulseek: {e}")),
                }
            }

            match spotdl_attempt(
                &spotdl_bin, &ffmpeg, &tools_path, url,
                &["youtube", "youtube-music", "soundcloud"], &job_dir.join("primary"), output_path,
            ).await {
                Ok(path) => break 'found (path, None),
                Err(e) => failures.push(format!("Primary sources: {e}")),
            }

            // spotDL's own result filter can reject the right upload (e.g. an artist
            // channel titled "Remedy (feat. X)"); accept a YouTube result only when
            // title, lead artist and duration all agree with Spotify's metadata.
            let known = match &meta {
                Some(m) => Ok(Some(m.clone())),
                None if url.contains("/track/") => spotify_track_meta(&spotdl_bin, &tools_path, url, &job_dir).await.map(Some),
                None => Ok(None),
            };
            match known {
                Ok(Some(m)) => match verified_youtube_match(app, &tools_path, &m).await {
                    Ok(Some(youtube_url)) => match spotdl_attempt(
                        &spotdl_bin, &ffmpeg, &tools_path, &format!("{youtube_url}|{url}"),
                        &["youtube"], &job_dir.join("youtube-match"), output_path,
                    ).await {
                        Ok(path) => break 'found (path, None),
                        Err(e) => failures.push(format!("Verified YouTube match: {e}")),
                    },
                    Ok(None) => failures.push("Verified YouTube match: no result agreed on title, artist and duration".into()),
                    Err(e) => failures.push(format!("Verified YouTube match: {e}")),
                },
                Ok(None) => {} // albums/playlists have no single duration to verify
                Err(e) => failures.push(format!("Verified YouTube match: {e}")),
            }

            match spotdl_attempt(
                &spotdl_bin, &ffmpeg, &tools_path, url,
                &["bandcamp"], &job_dir.join("bandcamp"), output_path,
            ).await {
                Ok(path) => break 'found (path, None),
                Err(e) => failures.push(format!("Bandcamp fallback: {e}")),
            }
            return Err(failures.join("; "));
        };
        // `None` derives the icon from the URL spotDL recorded in the comment tag.
        finish_download(app, id, &file_path, source).await;
        Ok(())
    }.await;
    let _ = std::fs::remove_dir_all(job_dir);
    result
}

/// The Spotify track's title, artists and duration, via `spotdl save`.
async fn spotify_track_meta(
    spotdl_bin: &std::path::Path,
    tools_path: &std::ffi::OsStr,
    url: &str,
    work_dir: &std::path::Path,
) -> Result<TrackMeta, String> {
    let meta_file = work_dir.join("meta.spotdl");
    let mut save = Command::new(spotdl_bin);
    save.args(["save", url, "--save-file"]).arg(&meta_file)
        .env("PATH", tools_path).env("COLUMNS", "1000");
    #[cfg(windows)] save.creation_flags(CREATE_NO_WINDOW);
    let saved = save.output().await.map_err(|e| format!("spotdl metadata lookup failed: {e}"))?;
    if !saved.status.success() { return Err("spotdl could not read the track's Spotify metadata".into()); }
    let meta_json = std::fs::read_to_string(&meta_file).map_err(|e| e.to_string())?;
    crate::matcher::parse_track_meta(&meta_json).ok_or_else(|| "spotdl returned unreadable track metadata".into())
}

/// Search YouTube for the track and return the URL of a result that verifiably
/// is the same recording (title, lead artist and duration all agree).
async fn verified_youtube_match(
    app: &AppHandle,
    tools_path: &std::ffi::OsStr,
    meta: &TrackMeta,
) -> Result<Option<String>, String> {
    let mut search = Command::new(crate::binaries::ytdlp(app)?);
    search.args(["--flat-playlist", "--dump-json", "--no-warnings"])
        .arg(format!("ytsearch10:{}", crate::matcher::search_query(meta)))
        .env("PATH", tools_path);
    #[cfg(windows)] search.creation_flags(CREATE_NO_WINDOW);
    let found = search.output().await.map_err(|e| format!("YouTube search failed: {e}"))?;
    let candidates = crate::matcher::parse_candidates(&String::from_utf8_lossy(&found.stdout));
    Ok(crate::matcher::pick_candidate(meta, &candidates).map(|c| c.url.clone()))
}

async fn spotdl_attempt(
    spotdl_bin: &std::path::Path,
    ffmpeg: &std::path::Path,
    tools_path: &std::ffi::OsStr,
    url: &str,
    providers: &[&str],
    attempt_dir: &std::path::Path,
    output_dir: &std::path::Path,
) -> Result<String, String> {
    std::fs::create_dir(attempt_dir).map_err(|e| e.to_string())?;
    let args = spotdl_args(url, &attempt_dir.to_string_lossy(), &ffmpeg.to_string_lossy(), providers);
    let output = {
        let mut cmd = Command::new(spotdl_bin);
        cmd.args(&args).arg("--print-errors");
        // Rich wraps console output at 80 columns when it is not a terminal,
        // which would split the error message across lines.
        cmd.env("PATH", tools_path).env("COLUMNS", "1000");
        #[cfg(windows)] cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.output().await.map_err(|e| format!("spotdl not found at {:?}: {e}", spotdl_bin))?
    };

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let failure = spotdl_failure_message(&stderr).or_else(|| spotdl_failure_message(&stdout));
    if !output.status.success() {
        return Err(failure.unwrap_or_else(|| "spotdl exited with error".into()));
    }
    move_spotdl_outputs(attempt_dir, output_dir).map_err(|e| failure.unwrap_or(e))
}

fn move_spotdl_outputs(job_dir: &std::path::Path, output_dir: &std::path::Path) -> Result<String, String> {
    let newest = newest_mp3_in(&job_dir.to_string_lossy())?;
    let file_name = std::path::Path::new(&newest).file_name()
        .ok_or("spotdl returned an invalid filename")?;
    let file_path = output_dir.join(file_name);
    for entry in std::fs::read_dir(job_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() { continue; }
        let dest = output_dir.join(entry.file_name());
        if dest.exists() {
            std::fs::copy(entry.path(), &dest).map_err(|e| e.to_string())?;
        } else {
            std::fs::rename(entry.path(), &dest).map_err(|e| e.to_string())?;
        }
    }
    Ok(file_path.to_string_lossy().into_owned())
}

async fn run_qobuz(app: &AppHandle, id: &str, url: &str, output_dir: &str, username: &str, password: &str) -> Result<(), String> {
    let args = qobuz_args(url, output_dir);
    let status = {
        let mut cmd = Command::new("qobuz-dlp");
        cmd.args(&args).env("QOBUZ_EMAIL", username).env("QOBUZ_PASSWORD", password);
        #[cfg(windows)] cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.status().await.map_err(|e| format!("qobuz-dlp not found: {e}"))?
    };

    if !status.success() { return Err("qobuz-dlp exited with error".into()); }

    let file_path = newest_file_in(output_dir, &["flac", "mp3"])?;
    finish_download(app, id, &file_path, Some("qobuz")).await;
    Ok(())
}

/// Stream-download a direct URL (Dropbox, Google Drive, S3, etc.) to output_dir.
async fn run_http_download(
    app: &AppHandle,
    id: &str,
    url: &str,
    output_dir: &str,
    source: &str,
) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
        .build()
        .map_err(|e| e.to_string())?;

    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP download failed: {e}"))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status()));
    }

    let total_bytes = resp.content_length();

    // Derive filename: Content-Disposition header, then URL path
    let filename = {
        let from_cd = resp
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .and_then(|cd| {
                cd.find("filename=").map(|i| {
                    cd[i + 9..]
                        .trim_matches('"')
                        .trim_matches('\'')
                        .split('"')
                        .next()
                        .unwrap_or("")
                        .to_string()
                })
            });
        match from_cd {
            Some(n) if !n.is_empty() => n,
            _ => resp
                .url()
                .path_segments()
                .and_then(|segs| segs.last())
                .and_then(|s| if s.is_empty() { None } else { Some(s.to_string()) })
                .unwrap_or_else(|| format!("{}.mp3", id)),
        }
    };

    let file_path = format!("{}/{}", output_dir, filename);
    let mut file = tokio::fs::File::create(&file_path)
        .await
        .map_err(|e| e.to_string())?;

    let mut downloaded: u64 = 0;
    let mut last_percent = None;
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if let Some(total) = total_bytes.filter(|&total| total > 0) {
            let pct = ((downloaded as u128 * 100 / total as u128).min(100)) as u8;
            if last_percent != Some(pct) {
                emit_progress(app, id, pct);
                last_percent = Some(pct);
            }
        }
    }

    finish_download(app, id, &file_path, Some(source)).await;
    Ok(())
}

pub(crate) fn emit_progress(app: &AppHandle, id: &str, percent: u8) {
    #[cfg(windows)]
    crate::native_circle::download_progress(id, percent);
    let _ = app.emit("download:progress", ProgressPayload {
        id: id.to_string(),
        percent,
    });
}

/// Probe the finished file, announce it to the frontend, then analyze it.
/// `source` is the provider the audio actually came from; `None` derives it
/// from the source URL spotDL wrote into the file's comment tag.
async fn finish_download(app: &AppHandle, id: &str, file_path: &str, source: Option<&str>) {
    let info = crate::media::probe(app, file_path).await;
    let source = source.map(str::to_string)
        .or_else(|| info.source_url.as_deref().and_then(crate::media::source_from_url).map(str::to_string))
        .unwrap_or_else(|| "spotify".into());
    let track_name = std::path::Path::new(file_path)
        .file_stem().map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    #[cfg(windows)]
    crate::native_circle::download_finished(id);
    let _ = app.emit("download:done", DonePayload {
        id: id.to_string(),
        file_path: file_path.to_string(),
        track_name,
        source,
        format: info.format,
        bitrate: info.bitrate,
    });
    crate::analyzer::analyze(app, id, file_path).await;
}

/// Pick the actionable line out of spotDL's output: the last "...Error: ..."
/// line, without the leading "<spotify url> - " spotDL prefixes it with.
fn spotdl_failure_message(output: &str) -> Option<String> {
    let line = output.lines().rev().map(str::trim).find(|l| l.contains("Error:"))?;
    let message = match line.split_once(" - ") {
        Some((prefix, rest)) if prefix.starts_with("https://open.spotify.com/") => rest,
        _ => line,
    };
    Some(message.trim().to_string())
}

fn parse_ytdlp_progress(line: &str) -> Option<u8> {
    // "[download]  42.3% of"
    let trimmed = line.trim();
    if !trimmed.starts_with("[download]") { return None; }
    let pct_str = trimmed.split_whitespace().nth(1)?.strip_suffix('%')?;
    let pct = pct_str.parse::<f64>().ok()?;
    if pct.is_finite() && (0.0..=100.0).contains(&pct) {
        Some(pct as u8)
    } else {
        None
    }
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
    fn spotdl_only_reports_audio_from_its_own_download() {
        let root = tempfile::tempdir().unwrap();
        let output_dir = root.path();
        let job_dir = output_dir.join("job");
        std::fs::create_dir(&job_dir).unwrap();
        std::fs::write(output_dir.join("older.mp3"), b"older").unwrap();

        assert!(move_spotdl_outputs(&job_dir, output_dir).is_err());
        std::fs::write(job_dir.join("new.mp3"), b"new").unwrap();
        let path = move_spotdl_outputs(&job_dir, output_dir).unwrap();
        assert_eq!(std::path::Path::new(&path), output_dir.join("new.mp3"));
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert_eq!(std::fs::read(output_dir.join("older.mp3")).unwrap(), b"older");
    }
    #[test]
    fn spotdl_failure_is_the_error_line_without_the_spotify_url_prefix() {
        let output = "Processing query: https://open.spotify.com/track/x\n\
            YouTube Music returned no usable results\n\
            https://open.spotify.com/track/x - LookupError: No results found for song: Lizzy Ashliegh - Remedy\n";
        assert_eq!(
            spotdl_failure_message(output).as_deref(),
            Some("LookupError: No results found for song: Lizzy Ashliegh - Remedy"),
        );
        assert_eq!(spotdl_failure_message("Downloaded \"A - B\": https://soundcloud.com/a"), None);
    }
    #[test]
    fn parse_progress_extracts_percent() {
        assert_eq!(parse_ytdlp_progress("[download]  42.3% of 5.00MiB"), Some(42));
        assert!(ytdlp_args("https://example.com", "output", "ffmpeg")
            .iter().any(|arg| arg == "--newline"));
        assert_eq!(parse_ytdlp_progress("[download] 100% of 5.00MiB"), Some(100));
        assert_eq!(parse_ytdlp_progress("[info] Writing thumbnail"), None);
        assert_eq!(parse_ytdlp_progress("[download] 0.0% of 5.00MiB"), Some(0));
        assert_eq!(parse_ytdlp_progress("[download] Unknown size"), None);
        assert_eq!(parse_ytdlp_progress("[download] 105% of 5.00MiB"), None);
    }
}
