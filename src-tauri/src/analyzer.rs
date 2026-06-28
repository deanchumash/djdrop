use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Clone, Serialize)]
pub struct AnalysisDonePayload { pub id: String, pub bpm: u32, pub key: String }

pub async fn analyze(app: &AppHandle, id: &str, file_path: &str) {
    let fp = file_path.to_string();
    // Resolve bundled ffmpeg path to pass into the blocking thread
    let ffmpeg = crate::binaries::ffmpeg_dir(app).ok()
        .map(|d| {
            let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
            d.join(exe).to_string_lossy().into_owned()
        })
        .unwrap_or_else(|| "ffmpeg".to_string());

    let bpm = tokio::task::spawn_blocking(move || detect_bpm_inline(&fp, &ffmpeg))
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

fn detect_bpm_inline(file_path: &str, ffmpeg_path: &str) -> Option<u32> {
    use aubio_rs::{OnsetMode, Tempo};
    use std::io::Read;
    use std::process::{Command, Stdio};

    let hop_size: usize = 512;
    let win_size: usize = 1024;
    let sample_rate: u32 = 44100;

    // Decode audio to raw f32le mono PCM via bundled ffmpeg
    let mut child = Command::new(ffmpeg_path)
        .args(["-i", file_path, "-f", "f32le", "-ar", "44100", "-ac", "1", "pipe:1"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let mut raw = Vec::new();
    child.stdout.take()?.read_to_end(&mut raw).ok()?;
    let _ = child.wait();

    let samples: Vec<f32> = raw
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    if samples.is_empty() {
        return None;
    }

    let mut tempo = Tempo::new(OnsetMode::SpecFlux, win_size, hop_size, sample_rate).ok()?;

    for chunk in samples.chunks(hop_size) {
        if chunk.len() < hop_size {
            break;
        }
        let _ = tempo.do_result(chunk);
    }

    let bpm = tempo.get_bpm();
    if bpm > 0.0 { Some(bpm.round() as u32) } else { None }
}

async fn run_keyfinder(app: &AppHandle, file_path: &str) -> Option<String> {
    let keyfinder_bin = crate::binaries::keyfinder_cli(app).ok()?;
    if !keyfinder_bin.exists() {
        return None;
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
        assert_eq!(detect_bpm_inline("/nonexistent/path/file.mp3", "ffmpeg"), None);
    }

    #[test]
    fn parses_keyfinder_output() {
        assert_eq!(parse_key("Am\n"), Some("Am".to_string()));
        assert_eq!(parse_key("F#\n"), Some("F#".to_string()));
        assert_eq!(parse_key(""), None);
    }
}
