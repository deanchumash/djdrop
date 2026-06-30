mod binaries;
mod analyzer;
mod config;
mod credentials;
mod downloader;
mod router;

use tauri::Manager;

#[cfg(windows)]
mod win32 {
    #[link(name = "user32")]
    extern "system" {
        pub fn SetWindowRgn(hwnd: isize, hrgn: isize, redraw: i32) -> i32;
        pub fn GetWindowLongPtrW(hwnd: isize, n_index: i32) -> isize;
        pub fn SetWindowLongPtrW(hwnd: isize, n_index: i32, dw_new_long: isize) -> isize;
        pub fn SetWindowPos(hwnd: isize, hwnd_insert_after: isize, x: i32, y: i32, cx: i32, cy: i32, u_flags: u32) -> i32;
        pub fn GetDpiForWindow(hwnd: isize) -> u32;
    }
    #[link(name = "dwmapi")]
    extern "system" {
        pub fn DwmSetWindowAttribute(hwnd: isize, dw_attribute: u32, pv_attribute: *const u32, cb_attribute: u32) -> i32;
    }
}

// Logical size of the circle in CSS pixels — must match App.css .circle width/height.
const CIRCLE: i32 = 72;

// Logical size of the panels popup window.
const PANELS_W: f64 = 280.0;
const PANELS_H: f64 = 400.0;

#[cfg(windows)]
fn hwnd_of(win: &tauri::WebviewWindow) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match win.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
        _ => None,
    }
}

#[cfg(windows)]
fn apply_dwm_borderless(hwnd: isize) {
    const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
    const DWMWCP_DONOTROUND: u32 = 1;
    const DWMWA_BORDER_COLOR: u32 = 34;
    const DWMWA_COLOR_NONE: u32 = 0xFFFFFFFE;
    unsafe {
        win32::DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_DONOTROUND, 4);
        win32::DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &DWMWA_COLOR_NONE, 4);
    }
}

/// Strip any Win32 chrome that WebView2 may have added and enforce the exact
/// desired physical size. For the 72×72 circle window this is a no-op when
/// decorations:false is working, but keeps us safe if it isn't.
#[cfg(windows)]
fn strip_window_chrome(win: &tauri::WebviewWindow) {
    let Some(hwnd) = hwnd_of(win) else { return };
    const GWL_STYLE: i32 = -16;
    const CHROME_BITS: isize = 0x00C00000 | 0x00040000 | 0x00020000 | 0x00010000 | 0x00080000;
    const SWP_FLAGS: u32 = 0x0002 | 0x0004 | 0x0020; // NOMOVE | NOZORDER | FRAMECHANGED
    unsafe {
        let dpi = win32::GetDpiForWindow(hwnd);
        let scale = if dpi == 0 { 1.0_f32 } else { dpi as f32 / 96.0 };
        let phys_w = (CIRCLE as f32 * scale).round() as i32;
        let phys_h = (CIRCLE as f32 * scale).round() as i32;
        let style = win32::GetWindowLongPtrW(hwnd, GWL_STYLE);
        win32::SetWindowLongPtrW(hwnd, GWL_STYLE, style & !CHROME_BITS);
        win32::SetWindowPos(hwnd, 0, 0, 0, phys_w, phys_h, SWP_FLAGS);
    }
    apply_dwm_borderless(hwnd);
}

/// Remove any Win32 region so the full 72×72 window is visible and apply DWM
/// borderless styling. Visual circle shape comes from CSS border-radius on the
/// transparent window — no SetWindowRgn needed (and it was clipping the circle).
#[cfg(windows)]
fn apply_circle_region(win: &tauri::WebviewWindow) {
    let Some(hwnd) = hwnd_of(win) else { return };
    unsafe {
        // NULL region = remove any existing region restriction
        win32::SetWindowRgn(hwnd, 0, 1);
    }
    apply_dwm_borderless(hwnd);
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

/// Toggle the panels popup. If hidden, position it flush above/right of the
/// circle window and show it. If visible, hide it.
#[tauri::command]
fn toggle_panels(app: tauri::AppHandle) {
    let Some(panels) = app.get_webview_window("panels") else { return };
    let Some(circle) = app.get_webview_window("circle") else { return };

    if panels.is_visible().unwrap_or(false) {
        let _ = panels.hide();
        return;
    }

    // Position panels: right-aligned with the circle, sitting just above it.
    if let (Ok(pos), Ok(size)) = (circle.outer_position(), circle.outer_size()) {
        let scale = circle.scale_factor().unwrap_or(1.0);
        let pw = (PANELS_W * scale).round() as i32;
        let ph = (PANELS_H * scale).round() as i32;
        let x = (pos.x + size.width as i32 - pw).max(0);
        let y = (pos.y - ph).max(0);
        let _ = panels.set_position(tauri::PhysicalPosition::new(x, y));
    }

    let _ = panels.show();
    let _ = panels.set_focus();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_config, save_config, start_download, save_credential, quit_app, toggle_panels,
        ])
        .setup(|app| {
            // Circle window: small transparent circle at bottom-right of primary monitor.
            if let Some(circle) = app.get_webview_window("circle") {
                let _ = circle.set_decorations(false);
                // Position at bottom-right corner, above the taskbar.
                if let Some(monitor) = circle.primary_monitor().ok().flatten() {
                    let mpos = monitor.position();
                    let msize = monitor.size();
                    let scale = circle.scale_factor().unwrap_or(1.0);
                    let pw = (CIRCLE as f64 * scale).round() as i32;
                    let ph = (CIRCLE as f64 * scale).round() as i32;
                    let margin = (12.0 * scale).round() as i32;
                    let taskbar = (48.0 * scale).round() as i32;
                    let x = mpos.x + msize.width as i32 - pw - margin;
                    let y = mpos.y + msize.height as i32 - ph - taskbar;
                    let _ = circle.set_position(tauri::PhysicalPosition::new(x, y));
                }
                #[cfg(windows)]
                {
                    strip_window_chrome(&circle);
                    apply_circle_region(&circle);
                }
            }

            // Panels window: created programmatically so it starts hidden.
            // Not transparent — avoids all the SetWindowRgn/DWM issues.
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
