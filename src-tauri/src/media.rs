use tauri::AppHandle;
use tokio::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Facts about a finished audio file, measured with ffprobe rather than assumed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaInfo {
    /// Upper-case file extension, e.g. "MP3" or "FLAC".
    pub format: String,
    /// Average audio bitrate in kbps, when ffprobe reports one.
    pub bitrate: Option<u32>,
    /// Source URL spotDL writes into the comment tag.
    pub source_url: Option<String>,
}

/// Map a provider URL to the source id the frontend renders an icon for.
pub fn source_from_url(url: &str) -> Option<&'static str> {
    let host = url::Url::parse(url.trim()).ok()?.host_str()?.to_lowercase();
    if host.contains("youtube.com") || host.contains("youtu.be") { return Some("youtube"); }
    if host.contains("soundcloud.com") { return Some("soundcloud"); }
    if host.contains("bandcamp.com") { return Some("bandcamp"); }
    None
}

pub fn parse_probe(json: &str, file_path: &str) -> MediaInfo {
    let format = std::path::Path::new(file_path).extension()
        .map(|e| e.to_string_lossy().to_uppercase())
        .unwrap_or_default();
    let value: serde_json::Value = serde_json::from_str(json).unwrap_or_default();

    let kbps = |v: &serde_json::Value| -> Option<u32> {
        let bps = v.get("bit_rate")?.as_str()?.parse::<f64>().ok()?;
        if bps > 0.0 { Some((bps / 1000.0).round() as u32) } else { None }
    };
    let bitrate = value.get("streams").and_then(|s| s.get(0)).and_then(kbps)
        .or_else(|| value.get("format").and_then(kbps));
    let source_url = value.get("format")
        .and_then(|f| f.get("tags"))
        .and_then(|t| t.as_object())
        .and_then(|tags| tags.iter().find(|(k, _)| k.eq_ignore_ascii_case("comment")))
        .and_then(|(_, v)| v.as_str())
        .map(str::to_string);

    MediaInfo { format, bitrate, source_url }
}

pub async fn probe(app: &AppHandle, file_path: &str) -> MediaInfo {
    let fallback = parse_probe("", file_path);
    let Ok(dir) = crate::binaries::ffmpeg_dir(app) else { return fallback };
    let ffprobe = dir.join(if cfg!(windows) { "ffprobe.exe" } else { "ffprobe" });
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v", "error", "-select_streams", "a:0",
        "-show_entries", "stream=bit_rate:format=bit_rate:format_tags=comment",
        "-of", "json", file_path,
    ]);
    #[cfg(windows)] cmd.creation_flags(CREATE_NO_WINDOW);
    match cmd.output().await {
        Ok(out) if out.status.success() => parse_probe(&String::from_utf8_lossy(&out.stdout), file_path),
        _ => fallback,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_stream_bitrate_and_spotdl_source_tag() {
        let json = r#"{"streams":[{"bit_rate":"320000"}],
            "format":{"bit_rate":"326289","tags":{"comment":"https://merczone.bandcamp.com/track/fun"}}}"#;
        let info = parse_probe(json, r"C:\music\Mercury - FUN.mp3");
        assert_eq!(info.format, "MP3");
        assert_eq!(info.bitrate, Some(320));
        assert_eq!(source_from_url(info.source_url.as_deref().unwrap()), Some("bandcamp"));
    }

    #[test]
    fn falls_back_to_container_bitrate_and_survives_missing_probe_data() {
        let info = parse_probe(r#"{"streams":[{}],"format":{"bit_rate":"245760"}}"#, "a.flac");
        assert_eq!((info.format.as_str(), info.bitrate, info.source_url), ("FLAC", Some(246), None));
        assert_eq!(parse_probe("not json", "b.wav"), MediaInfo { format: "WAV".into(), bitrate: None, source_url: None });
    }

    #[test]
    fn maps_known_provider_hosts_only() {
        assert_eq!(source_from_url("https://music.youtube.com/watch?v=1"), Some("youtube"));
        assert_eq!(source_from_url("https://youtu.be/1"), Some("youtube"));
        assert_eq!(source_from_url("https://soundcloud.com/a/b"), Some("soundcloud"));
        assert_eq!(source_from_url("https://example.com/a"), None);
        assert_eq!(source_from_url("not a url"), None);
    }
}
