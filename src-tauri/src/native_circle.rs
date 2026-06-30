/// Pure Win32 layered window for the circle — no WebView2.
/// Per-pixel alpha via UpdateLayeredWindow; OLE IDropTarget for browser URL drops.
#![cfg(windows)]

use std::sync::atomic::{AtomicIsize, Ordering};
use uuid::Uuid;
use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{
            Com::*,
            DataExchange::*,
            Memory::*,
            Ole::*,
        },
        UI::{
            HiDpi::GetDpiForWindow,
            WindowsAndMessaging::*,
        },
    },
};

pub static CIRCLE_HWND: AtomicIsize = AtomicIsize::new(0);

const W: i32 = 72;
const H: i32 = 72;

// Navy fill:  #1a1a2e
const COL_BG: (u8, u8, u8) = (0x1a, 0x1a, 0x2e);
// Hover fill: #2a2a4e
const COL_HOVER: (u8, u8, u8) = (0x2a, 0x2a, 0x4e);
// Green ring: #4ade80
const COL_RING: (u8, u8, u8) = (0x4a, 0xde, 0x80);

struct CircleState {
    app: tauri::AppHandle,
    mouse_down: bool,
    dragging: bool,
    cursor_start: POINT,
    win_start: POINT,
}

// ── rendering ────────────────────────────────────────────────────────────────

fn set_pixel(pixels: &mut [u32], x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if x < 0 || y < 0 || x >= W || y >= H { return; }
    let pa = a as u32;
    let pr = (r as u32 * pa / 255) as u8;
    let pg = (g as u32 * pa / 255) as u8;
    let pb = (b as u32 * pa / 255) as u8;
    // BGRA in memory (little-endian), pre-multiplied alpha
    pixels[(y * W + x) as usize] = (pa << 24) | ((pr as u32) << 16) | ((pg as u32) << 8) | pb as u32;
}

unsafe fn redraw(hwnd: HWND, progress: f32, hover: bool) {
    let screen_dc = GetDC(HWND(std::ptr::null_mut()));
    let mem_dc = CreateCompatibleDC(screen_dc);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: W,
            biHeight: -H, // top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let hbmp = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap();
    let old = SelectObject(mem_dc, hbmp);

    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (W * H) as usize);

    // Fill circle with anti-aliased edge
    let (br, bg, bb) = if hover { COL_HOVER } else { COL_BG };
    let cx = W as f32 / 2.0;
    let cy = H as f32 / 2.0;
    let radius = cx - 1.5;

    for py in 0..H {
        for px in 0..W {
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            let a: u8 = if d <= radius - 1.0 {
                255
            } else if d <= radius + 1.0 {
                ((radius + 1.0 - d) * 127.5) as u8
            } else {
                0
            };
            set_pixel(pixels, px, py, br, bg, bb, a);
        }
    }

    // Down-arrow triangle (white)
    let acx = W / 2;
    let acy = H / 2;
    for dy in 0i32..10 {
        let half = 8 - dy * 8 / 10;
        for dx in -half..=half {
            let px = acx + dx;
            let py = acy - 3 + dy;
            let base_a = ((pixels[(py * W + px) as usize] >> 24) & 0xFF) as u8;
            if base_a > 0 {
                set_pixel(pixels, px, py, 255, 255, 255, base_a);
            }
        }
    }

    // Progress ring (green arc, 3px wide, from top clockwise)
    if progress > 0.0 {
        let rr = cx - 4.0;
        let steps = 720u32;
        for i in 0..steps {
            if i as f32 / steps as f32 > progress { break; }
            let angle = std::f32::consts::FRAC_PI_2 * -1.0
                + (i as f32 / steps as f32) * 2.0 * std::f32::consts::PI;
            for dr in -1i32..=1 {
                let px = (cx + (rr + dr as f32) * angle.cos()) as i32;
                let py = (cy + (rr + dr as f32) * angle.sin()) as i32;
                let base_a = if px >= 0 && py >= 0 && px < W && py < H {
                    ((pixels[(py * W + px) as usize] >> 24) & 0xFF) as u8
                } else { 0 };
                if base_a > 128 {
                    let (rr, rg, rb) = COL_RING;
                    set_pixel(pixels, px, py, rr, rg, rb, base_a);
                }
            }
        }
    }

    let blend = BLENDFUNCTION {
        BlendOp: 0,   // AC_SRC_OVER
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1, // AC_SRC_ALPHA
    };
    let size = SIZE { cx: W, cy: H };
    let src_pt = POINT { x: 0, y: 0 };
    let _ = UpdateLayeredWindow(
        hwnd, screen_dc, None, Some(&size),
        mem_dc, Some(&src_pt), COLORREF(0), Some(&blend), ULW_ALPHA,
    );

    SelectObject(mem_dc, old);
    let _ = DeleteObject(hbmp);
    let _ = DeleteDC(mem_dc);
    ReleaseDC(HWND(std::ptr::null_mut()), screen_dc);
}

// ── IDropTarget ───────────────────────────────────────────────────────────────

#[implement(IDropTarget)]
struct DropTarget {
    app: tauri::AppHandle,
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        _pdataobj: Option<&IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe { if !pdweffect.is_null() { *pdweffect = DROPEFFECT_COPY; } }
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe { if !pdweffect.is_null() { *pdweffect = DROPEFFECT_COPY; } }
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> { Ok(()) }

    fn Drop(
        &self,
        pdataobj: Option<&IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        _pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> Result<()> {
        unsafe { if !pdweffect.is_null() { *pdweffect = DROPEFFECT_COPY; } }
        if let Some(obj) = pdataobj {
            if let Some(text) = unsafe { extract_text(obj) } {
                let t = text.trim().to_string();
                if !t.is_empty() {
                    let app = self.app.clone();
                    let id = Uuid::new_v4().to_string();
                    tauri::async_runtime::spawn(async move {
                        let _ = crate::start_download_inner(app, id, t).await;
                    });
                }
            }
        }
        Ok(())
    }
}

unsafe fn extract_text(obj: &IDataObject) -> Option<String> {
    for cf in [CF_UNICODETEXT, CF_TEXT] {
        let fmt = FORMATETC {
            cfFormat: cf.0 as u16,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0,
        };
        let mut med = STGMEDIUM::default();
        if obj.GetData(&fmt, &mut med).is_ok() {
            let hg = med.Anonymous.hGlobal;
            let ptr = GlobalLock(hg);
            if !ptr.is_null() {
                let text = if cf == CF_UNICODETEXT {
                    let n = GlobalSize(hg) / 2;
                    let s = std::slice::from_raw_parts(ptr as *const u16, n);
                    String::from_utf16_lossy(s)
                } else {
                    let n = GlobalSize(hg);
                    let s = std::slice::from_raw_parts(ptr as *const u8, n);
                    String::from_utf8_lossy(s).into_owned()
                };
                let _ = GlobalUnlock(hg);
                ReleaseStgMedium(&mut med);
                let clean = text.trim_end_matches('\0').trim().to_string();
                if !clean.is_empty() { return Some(clean); }
            }
            ReleaseStgMedium(&mut med);
        }
    }
    None
}

// ── WNDPROC ───────────────────────────────────────────────────────────────────

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN => {
            let state = state_of(hwnd);
            if state.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let s = &mut *state;
            let _ = SetCapture(hwnd);
            let mut cur = POINT::default();
            let _ = GetCursorPos(&mut cur);
            let mut wr = RECT::default();
            let _ = GetWindowRect(hwnd, &mut wr);
            s.mouse_down = true;
            s.dragging = false;
            s.cursor_start = cur;
            s.win_start = POINT { x: wr.left, y: wr.top };
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let state = state_of(hwnd);
            if state.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let s = &mut *state;
            if s.mouse_down {
                let mut cur = POINT::default();
                let _ = GetCursorPos(&mut cur);
                let dx = cur.x - s.cursor_start.x;
                let dy = cur.y - s.cursor_start.y;
                if !s.dragging && dx * dx + dy * dy > 25 {
                    s.dragging = true;
                }
                if s.dragging {
                    let _ = SetWindowPos(
                        hwnd, HWND(std::ptr::null_mut()),
                        s.win_start.x + dx, s.win_start.y + dy, 0, 0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let state = state_of(hwnd);
            if state.is_null() { return DefWindowProcW(hwnd, msg, wparam, lparam); }
            let s = &mut *state;
            let _ = ReleaseCapture();
            if s.mouse_down && !s.dragging {
                crate::do_toggle_panels(s.app.clone(), hwnd.0 as isize);
            }
            s.mouse_down = false;
            s.dragging = false;
            LRESULT(0)
        }
        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CircleState;
            if !ptr.is_null() {
                let _ = RevokeDragDrop(hwnd);
                drop(Box::from_raw(ptr));
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            }
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn state_of(hwnd: HWND) -> *mut CircleState {
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut CircleState }
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run(app: tauri::AppHandle) {
    unsafe {
        // Init COM on this thread (needed for OLE drag-and-drop)
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let hmod = GetModuleHandleW(None).unwrap_or_default();
        let class_name: Vec<u16> = "djdrop_circle\0".encode_utf16().collect();
        let window_title: Vec<u16> = "djdrop\0".encode_utf16().collect();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: HINSTANCE(hmod.0 as *mut _),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);

        // Compute bottom-right position on primary monitor
        let (x, y) = primary_monitor_circle_pos();

        let mut state = Box::new(CircleState {
            app: app.clone(),
            mouse_down: false,
            dragging: false,
            cursor_start: POINT::default(),
            win_start: POINT::default(),
        });
        let state_ptr = Box::as_mut(&mut state) as *mut CircleState;

        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(window_title.as_ptr()),
            WS_POPUP,
            x, y, W, H,
            None, None,
            HINSTANCE(hmod.0 as *mut _),
            Some(state_ptr as *const _),
        ).unwrap();

        // state is now owned by the WNDPROC via GWLP_USERDATA; don't drop the Box here
        std::mem::forget(state);

        CIRCLE_HWND.store(hwnd.0 as isize, Ordering::Relaxed);

        // Register OLE drop target
        let drop_target: IDropTarget = DropTarget { app }.into();
        let _ = RegisterDragDrop(hwnd, &drop_target);

        // Initial draw
        redraw(hwnd, 0.0, false);

        // Show
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        // Message loop
        let mut msg = MSG::default();
        loop {
            match GetMessageW(&mut msg, None, 0, 0).0 {
                0 => break,
                -1 => break,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        CIRCLE_HWND.store(0, Ordering::Relaxed);
        let _ = CoUninitialize();
    }
}

fn primary_monitor_circle_pos() -> (i32, i32) {
    // Try to get primary monitor working area
    unsafe {
        let hwnd_desktop = GetDesktopWindow();
        let dpi = GetDpiForWindow(hwnd_desktop);
        let scale = if dpi == 0 { 1.0f32 } else { dpi as f32 / 96.0 };
        let phys_w = (W as f32 * scale).round() as i32;
        let phys_h = (H as f32 * scale).round() as i32;

        let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let hmon = MonitorFromWindow(hwnd_desktop, MONITOR_DEFAULTTOPRIMARY);
        if GetMonitorInfoW(hmon, &mut info).as_bool() {
            let wa = info.rcWork; // work area excludes taskbar
            return (wa.right - phys_w - 12, wa.bottom - phys_h - 12);
        }

        // Fallback
        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        (sw - phys_w - 12, sh - phys_h - 60)
    }
}
