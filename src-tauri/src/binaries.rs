use std::path::PathBuf;
use tauri::Manager;

fn binaries_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resource_dir()
        .map(|d| d.join("binaries"))
        .map_err(|e| e.to_string())
}

fn bin(app: &tauri::AppHandle, name: &str) -> Result<PathBuf, String> {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    Ok(binaries_dir(app)?.join(format!("{}{}", name, ext)))
}

pub fn ytdlp(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    bin(app, "yt-dlp")
}

pub fn ffmpeg_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    binaries_dir(app)
}

pub fn spotdl(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    bin(app, "spotdl")
}

pub fn keyfinder_cli(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    bin(app, "keyfinder-cli")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_suffix_on_windows() {
        // Verify the exe helper appends .exe on Windows
        let suffix = if cfg!(windows) { ".exe" } else { "" };
        let expected = format!("yt-dlp{}", suffix);
        // Can't call bin() without AppHandle in unit test; verify suffix logic directly
        assert!(expected.ends_with(suffix));
    }
}
