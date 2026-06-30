mod binaries;
mod analyzer;
mod config;
mod credentials;
mod downloader;
mod router;
#[cfg(windows)]
mod native_circle;

use serde::Serialize;
use tauri::{Emitter, Manager};

const PANELS_W: f64 = 280.0;
const PANELS_H: f64 = 400.0;

// ── download ──────────────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
struct QueuedPayload { id: String, input: String }

/// Shared by the Tauri command and the native circle's IDropTarget.
pub async fn start_download_inner(
    app: tauri::AppHandle,
    id: String,
    input: String,
) -> Result<(), String> {
    let cfg = config::read(&app).map_err(|e| e.to_string())?;
    let source = router::route(&input);
    let output_dir = cfg.output_dir.clone();
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    // Notify the frontend so it can show the item in the queue even when the
    // drop came from the native circle (not from a frontend invoke).
    let _ = app.emit("download:queued", QueuedPayload { id: id.clone(), input: input.clone() });
    tokio::spawn(downloader::run_download(app, id, source, input, output_dir));
    Ok(())
}

#[tauri::command]
async fn start_download(app: tauri::AppHandle, id: String, input: String) -> Result<(), String> {
    start_download_inner(app, id, input).await
}

// ── panels toggle ─────────────────────────────────────────────────────────────

/// Called from the native circle on click. `circle_hwnd` is the Win32 HWND of
/// the native circle window (used to position the panels above/right of it).
pub fn do_toggle_panels(app: tauri::AppHandle, circle_hwnd: isize) {
    let Some(panels) = app.get_webview_window("panels") else { return };

    if panels.is_visible().unwrap_or(false) {
        let _ = panels.hide();
        return;
    }

    // Position panels above/right of the native circle window
    #[cfg(windows)]
    {
        use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetWindowRect};
        let mut wr = RECT::default();
        unsafe {
            let hwnd = windows::Win32::Foundation::HWND(circle_hwnd as *mut _);
            let _ = GetWindowRect(hwnd, &mut wr);
        }
        let scale = panels.scale_factor().unwrap_or(1.0);
        let pw = (PANELS_W * scale).round() as i32;
        let ph = (PANELS_H * scale).round() as i32;
        let x = (wr.left + (wr.right - wr.left) - pw).max(0);
        let y = (wr.top - ph).max(0);
        let _ = panels.set_position(tauri::PhysicalPosition::new(x, y));
    }

    let _ = panels.show();
    let _ = panels.set_focus();
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
async fn get_config(app: tauri::AppHandle) -> Result<config::Config, String> {
    config::read(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(app: tauri::AppHandle, config: config::Config) -> Result<(), String> {
    config::write(&app, &config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_credential(pool: String, username: String, password: String) -> Result<(), String> {
    credentials::store_in_keychain(&pool, &username, &password)
}

#[tauri::command]
async fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

// ── app setup ─────────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_config, save_config, start_download, save_credential, quit_app,
        ])
        .setup(|app| {
            // Spawn the native Win32 circle on a background thread.
            // It owns its own message loop and communicates via AppHandle.
            let app_h = app.handle().clone();
            std::thread::spawn(move || {
                #[cfg(windows)]
                native_circle::run(app_h);
            });

            // Panels window: opaque, decoration-free popup, starts hidden.
            tauri::WebviewWindowBuilder::new(
                app,
                "panels",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("djdrop")
            .inner_size(PANELS_W, PANELS_H)
            .decorations(false)
            .always_on_top(true)
            .resizable(false)
            .skip_taskbar(true)
            .visible(false)
            .build()?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running djdrop");
}
