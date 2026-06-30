use serde::Serialize;
use tauri::{AppHandle, Emitter};
use tokio::process::Command;

#[derive(Clone, Serialize)]
pub struct AnalysisDonePayload { pub id: String, pub bpm: u32, pub key: String }

pub async fn analyze(app: &AppHandle, id: &str, file_path: &str) {
    let fp = file_path.to_string();
    let ffmpeg = crate::binaries::ffmpeg_dir(app).ok()
        .map(|d| {
            let exe = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
            d.join(exe).to_string_lossy().into_owned()
        })
        .unwrap_or_else(|| "ffmpeg".to_string());

    let raw_bpm = {
        let fp = fp.clone();
        let ff = ffmpeg.clone();
        tokio::task::spawn_blocking(move || detect_bpm_inline(&fp, &ff))
            .await.unwrap_or(None).unwrap_or(0)
    };

    let cfg = crate::config::read(app).unwrap_or_default();
    let bpm = normalize_bpm(raw_bpm, cfg.bpm_range_min, cfg.bpm_range_max);

    // Use keyfinder-cli if present, otherwise run inline chromagram analysis.
    // Always emit the raw musical key — Camelot conversion is done in the frontend
    // so that switching the setting updates already-downloaded items instantly.
    let key = {
        let keyfinder = crate::binaries::keyfinder_cli(app).ok()
            .filter(|p| p.exists());
        if let Some(kf) = keyfinder {
            run_keyfinder_path(&kf, file_path).await.unwrap_or_default()
        } else {
            let fp = fp.clone();
            let ff = ffmpeg.clone();
            tokio::task::spawn_blocking(move || detect_key_inline(&fp, &ff))
                .await.unwrap_or(None).unwrap_or_default()
        }
    };

    if bpm > 0 || !key.is_empty() {
        let _ = app.emit("analysis:done", AnalysisDonePayload {
            id: id.to_string(), bpm, key,
        });
    }
}

fn normalize_bpm(mut bpm: u32, min: Option<u32>, max: Option<u32>) -> u32 {
    if let (Some(lo), Some(hi)) = (min, max) {
        if lo > 0 && lo < hi && bpm > 0 {
            while bpm < lo { bpm *= 2; }
            while bpm > hi { bpm /= 2; }
        }
    }
    bpm
}

fn to_camelot(key: &str) -> String {
    match key {
        "C" => "8B", "G" => "9B", "D" => "10B", "A" => "11B",
        "E" => "12B", "B" => "1B", "F#" => "2B", "C#" => "3B",
        "G#" => "4B", "D#" => "5B", "A#" => "6B", "F" => "7B",
        "Am" => "8A", "Em" => "9A", "Bm" => "10A", "F#m" => "11A",
        "C#m" => "12A", "G#m" => "1A", "D#m" => "2A", "A#m" => "3A",
        "Fm" => "4A", "Cm" => "5A", "Gm" => "6A", "Dm" => "7A",
        _ => key,
    }.to_string()
}

// ── BPM detection ─────────────────────────────────────────────────────────────

fn detect_bpm_inline(file_path: &str, ffmpeg_path: &str) -> Option<u32> {
    use aubio_rs::{OnsetMode, Tempo};
    use std::io::Read;
    use std::process::{Command, Stdio};

    let hop_size: usize = 512;
    let win_size: usize = 1024;
    let sample_rate: u32 = 44100;

    let mut child = {
        #[cfg(windows)] use std::os::windows::process::CommandExt;
        let mut cmd = Command::new(ffmpeg_path);
        cmd.args(["-i", file_path, "-f", "f32le", "-ar", "44100", "-ac", "1", "pipe:1"])
           .stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)] cmd.creation_flags(0x0800_0000);
        cmd.spawn().ok()?
    };

    let mut raw = Vec::new();
    child.stdout.take()?.read_to_end(&mut raw).ok()?;
    let _ = child.wait();

    let samples: Vec<f32> = raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    if samples.is_empty() { return None; }

    let mut tempo = Tempo::new(OnsetMode::SpecFlux, win_size, hop_size, sample_rate).ok()?;
    for chunk in samples.chunks(hop_size) {
        if chunk.len() < hop_size { break; }
        let _ = tempo.do_result(chunk);
    }
    let bpm = tempo.get_bpm();
    if bpm > 0.0 { Some(bpm.round() as u32) } else { None }
}

// ── key detection ─────────────────────────────────────────────────────────────

async fn run_keyfinder_path(bin: &std::path::Path, file_path: &str) -> Option<String> {
    let mut cmd = Command::new(bin);
    cmd.args([file_path]);
    #[cfg(windows)] cmd.creation_flags(0x0800_0000);
    let output = cmd.output().await.ok()?;
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Chromagram + Krumhansl-Schmuckler key detection — no external binary needed.
fn detect_key_inline(file_path: &str, ffmpeg_path: &str) -> Option<String> {
    use rustfft::{FftPlanner, num_complex::Complex};
    use std::io::Read;
    use std::process::{Command, Stdio};

    const SR: u32 = 11025;
    const WIN: usize = 8192;
    const HOP: usize = 4096;

    let mut child = {
        #[cfg(windows)] use std::os::windows::process::CommandExt;
        let mut cmd = Command::new(ffmpeg_path);
        cmd.args(["-i", file_path, "-f", "f32le", "-ar", &SR.to_string(), "-ac", "1", "pipe:1"])
           .stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)] cmd.creation_flags(0x0800_0000);
        cmd.spawn().ok()?
    };

    let mut raw = Vec::new();
    child.stdout.take()?.read_to_end(&mut raw).ok()?;
    let _ = child.wait();

    let samples: Vec<f32> = raw.chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();

    if samples.len() < WIN { return None; }

    // Pre-compute FFT bin → pitch class (0=C … 11=B), reference C4 = 261.63 Hz
    let bin_to_pc: Vec<Option<usize>> = (0..WIN / 2)
        .map(|k| {
            let f = k as f64 * SR as f64 / WIN as f64;
            if f < 27.5 || f > 4200.0 { return None; }
            let semitones = 12.0 * (f / 261.63_f64).log2();
            Some(semitones.round().rem_euclid(12.0) as usize)
        })
        .collect();

    let mut chroma = [0.0f64; 12];
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(WIN);

    for start in (0..samples.len().saturating_sub(WIN)).step_by(HOP) {
        let mut buf: Vec<Complex<f32>> = samples[start..start + WIN]
            .iter()
            .enumerate()
            .map(|(i, &s)| {
                // Hanning window
                let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (WIN - 1) as f32).cos();
                Complex::new(s * w, 0.0)
            })
            .collect();
        fft.process(&mut buf);
        for (k, opt) in bin_to_pc.iter().enumerate() {
            if let Some(pc) = opt {
                chroma[*pc] += buf[k].norm_sqr() as f64;
            }
        }
    }

    let max = chroma.iter().cloned().fold(0.0f64, f64::max);
    if max == 0.0 { return None; }

    // Krumhansl-Schmuckler key profiles
    const MAJOR: [f64; 12] = [6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88];
    const MINOR: [f64; 12] = [6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17];
    const NOTES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];

    let pearson = |profile: &[f64; 12], c: &[f64; 12]| -> f64 {
        let pm = profile.iter().sum::<f64>() / 12.0;
        let cm = c.iter().sum::<f64>() / 12.0;
        let num: f64 = (0..12).map(|i| (profile[i] - pm) * (c[i] - cm)).sum();
        let dp: f64 = (0..12).map(|i| (profile[i] - pm).powi(2)).sum::<f64>().sqrt();
        let dc: f64 = (0..12).map(|i| (c[i] - cm).powi(2)).sum::<f64>().sqrt();
        if dp == 0.0 || dc == 0.0 { 0.0 } else { num / (dp * dc) }
    };

    let mut best = (f64::NEG_INFINITY, String::from("C"));

    for root in 0..12usize {
        // Rotate chroma so `root` note is at index 0, matching the profile tonic
        let mut rot = [0.0f64; 12];
        for i in 0..12 { rot[i] = chroma[(i + root) % 12]; }

        let ms = pearson(&MAJOR, &rot);
        if ms > best.0 { best = (ms, NOTES[root].to_string()); }
        let ns = pearson(&MINOR, &rot);
        if ns > best.0 { best = (ns, format!("{}m", NOTES[root])); }
    }

    Some(best.1)
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_bpm_inline_returns_none_for_missing_file() {
        assert_eq!(detect_bpm_inline("/nonexistent/path/file.mp3", "ffmpeg"), None);
    }

    #[test]
    fn normalize_bpm_doubles_below_min() {
        assert_eq!(normalize_bpm(70, Some(120), Some(175)), 140);
        // 45 → ×2=90 → ×2=180 (done doubling) → ÷2=90 (done halving)
        assert_eq!(normalize_bpm(45, Some(120), Some(175)), 90);
    }

    #[test]
    fn normalize_bpm_halves_above_max() {
        assert_eq!(normalize_bpm(200, Some(120), Some(175)), 100);
    }

    #[test]
    fn normalize_bpm_in_range_unchanged() {
        assert_eq!(normalize_bpm(140, Some(120), Some(175)), 140);
    }

    #[test]
    fn normalize_bpm_no_range_unchanged() {
        assert_eq!(normalize_bpm(70, None, None), 70);
    }

    #[test]
    fn camelot_major_keys() {
        assert_eq!(to_camelot("C"), "8B");
        assert_eq!(to_camelot("A"), "11B");
        assert_eq!(to_camelot("F#"), "2B");
    }

    #[test]
    fn camelot_minor_keys() {
        assert_eq!(to_camelot("Am"), "8A");
        assert_eq!(to_camelot("Dm"), "7A");
        assert_eq!(to_camelot("F#m"), "11A");
    }

    #[test]
    fn camelot_unknown_key_passthrough() {
        assert_eq!(to_camelot("Xm"), "Xm");
    }
}
