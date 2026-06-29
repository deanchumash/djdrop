mod binaries;
mod analyzer;
mod config;
mod credentials;
mod downloader;
mod router;

use tauri::Manager;

// Win32 APIs for hit-test region and decoration management (Windows only)
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
        pub fn GetWindowLongPtrW(hwnd: isize, n_index: i32) -> isize;
        pub fn SetWindowLongPtrW(hwnd: isize, n_index: i32, dw_new_long: isize) -> isize;
        pub fn SetWindowPos(hwnd: isize, hwnd_insert_after: isize, x: i32, y: i32, cx: i32, cy: i32, u_flags: u32) -> i32;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        // pv_attribute is always a *const u32 (DWORD) for the attributes we use
        pub fn DwmSetWindowAttribute(hwnd: isize, dw_attribute: u32, pv_attribute: *const u32, cb_attribute: u32) -> i32;
    }
}

// Must match tauri.conf.json window size and App.css circle position
const WIN_W: i32 = 300;
const WIN_H: i32 = 440;
const CIRCLE: i32 = 72;

/// Strip title bar and borders via Win32 — belt-and-suspenders over decorations:false
/// in tauri.conf.json, which WebView2 can override during initialisation.
#[cfg(windows)]
fn strip_window_chrome(win: &tauri::WebviewWindow) {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = win.window_handle() else { return };
    let hwnd = match handle.as_raw() {
        RawWindowHandle::Win32(h) => h.hwnd.get(),
        _ => return,
    };
    const GWL_STYLE: i32 = -16;
    // WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU
    const CHROME_BITS: isize = 0x00C00000 | 0x00040000 | 0x00020000 | 0x00010000 | 0x00080000;
    // SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED
    const SWP_FLAGS: u32 = 0x0002 | 0x0001 | 0x0004 | 0x0020;
    // DWM attributes (Windows 11 22000+): remove compositor border and round corners
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_DONOTROUND: u32 = 1;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
    unsafe {
        // Strip Win32 style bits
        let style = win32::GetWindowLongPtrW(hwnd, GWL_STYLE);
        win32::SetWindowLongPtrW(hwnd, GWL_STYLE, style & !CHROME_BITS);
        win32::SetWindowPos(hwnd, 0, 0, 0, 0, 0, SWP_FLAGS);
        // Tell DWM compositor to draw no border and no rounded corners
        win32::DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_DONOTROUND, 4);
        win32::DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &DWMWA_COLOR_NONE, 4);
    }
}

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
                let _ = win.set_decorations(false);
                let _ = win.set_ignore_cursor_events(false);
                #[cfg(windows)]
                {
                    strip_window_chrome(&win);
                    apply_window_region(&win, false);
                }
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running djdrop");
}
