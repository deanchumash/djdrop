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
