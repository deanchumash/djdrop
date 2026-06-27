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
