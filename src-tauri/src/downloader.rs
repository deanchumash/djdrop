use crate::router::DownloadSource;
use serde::Serialize;
use std::process::Stdio;
use tauri::{AppHandle, Emitter};
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
    let stderr = child.stderr.take().unwrap();

    let app_clone = app.clone();
    let id_clone = id.to_string();

    // Read stderr for progress lines concurrently with stdout
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

    // Read stdout for the filepath (--print after_move:filepath outputs one line)
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
        id: id.to_string(), file_path: file_path.clone(), track_name, source: "spotify".into()
    });
    crate::analyzer::analyze(app, id, &file_path).await;
    Ok(())
}

async fn run_qobuz(app: &AppHandle, id: &str, url: &str, output_dir: &str, username: &str, password: &str) -> Result<(), String> {
    let args = qobuz_args(url, output_dir);
    let status = Command::new("qobuz-dlp")
        .args(&args)
        .env("QOBUZ_EMAIL", username)
        .env("QOBUZ_PASSWORD", password)
        .status()
        .await
        .map_err(|e| format!("qobuz-dlp not found: {e}"))?;

    if !status.success() { return Err("qobuz-dlp exited with error".into()); }

    let file_path = newest_file_in(output_dir, &["flac", "mp3"])?;
    let track_name = std::path::Path::new(&file_path)
        .file_stem().map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let _ = app.emit("download:done", DonePayload {
        id: id.to_string(), file_path: file_path.clone(), track_name, source: "qobuz".into()
    });
    crate::analyzer::analyze(app, id, &file_path).await;
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
