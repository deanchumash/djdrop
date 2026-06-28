mod binaries;
mod analyzer;
mod config;
mod credentials;
mod downloader;
mod router;

use tauri::Manager;

// Win32 APIs for hit-test region management (Windows only)
#[cfg(windows)]
mod win32 {
    #[link(name = "gdi32")]
    extern "system" {
        pub fn CreateEllipticRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> isize;
        pub fn CreateRectRgn(x1: i32, y1: i32, x2: i32, y2: i32) -> isize;
    }
    #[link(name = "user32")]
    extern "system" {
        pub fn SetWindowRgn(hwnd: isize, hrgn: isize, redraw: i32) -> i32;
    }
}

// Must match tauri.conf.json window size and App.css circle position
const WIN_W: i32 = 300;
const WIN_H: i32 = 440;
const CIRCLE: i32 = 72;

/// Restrict mouse hit-testing to the circle when no panels are open,
/// or to the full window when a panel is visible.
#[cfg(windows)]
fn apply_window_region(win: &tauri::WebviewWindow, panels_open: bool) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = win.window_handle() else { return };
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(h) => h.hwnd.get(),
        _ => return,
    };
    unsafe {
        let rgn = if panels_open {
            win32::CreateRectRgn(0, 0, WIN_W, WIN_H)
        } else {
            win32::CreateEllipticRgn(WIN_W - CIRCLE, WIN_H - CIRCLE, WIN_W, WIN_H)
        };
        win32::SetWindowRgn(hwnd, rgn, 1);
    }
}

#[tauri::command]
async fn get_config(app: tauri::AppHandle) -> Result<config::Config, String> {
    config::read(&app).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_config(app: tauri::AppHandle, config: config::Config) -> Result<(), String> {
    config::write(&app, &config).map_err(|e| e.to_string())
}

#[tauri::command]
async fn start_download(app: tauri::AppHandle, id: String, input: String) -> Result<(), String> {
    let cfg = config::read(&app).map_err(|e| e.to_string())?;
    let source = router::route(&input);
    let output_dir = cfg.output_dir.clone();
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    tokio::spawn(downloader::run_download(app, id, source, input, output_dir));
    Ok(())
}

#[tauri::command]
async fn save_credential(pool: String, username: String, password: String) -> Result<(), String> {
    credentials::store_in_keychain(&pool, &username, &password)
}

#[tauri::command]
async fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

/// Called from React whenever any panel opens or closes.
#[tauri::command]
fn set_panels_open(app: tauri::AppHandle, open: bool) {
    #[cfg(windows)]
    if let Some(win) = app.get_webview_window("main") {
        apply_window_region(&win, open);
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_config, save_config, start_download, save_credential, quit_app, set_panels_open,
        ])
        .setup(|app| {
            if let Some(win) = app.get_webview_window("main") {
                let _ = win.set_ignore_cursor_events(false);
                #[cfg(windows)]
                apply_window_region(&win, false);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running djdrop");
}
