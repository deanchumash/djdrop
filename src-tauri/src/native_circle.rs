/// Pure Win32 layered window for the circle — no WebView2.
/// Per-pixel alpha via UpdateLayeredWindow; raw-vtable IDropTarget for browser URL drops.
#![cfg(windows)]

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use uuid::Uuid;

use windows::Win32::{
    Foundation::{
        BOOL, HANDLE, HINSTANCE, HWND, LPARAM, LRESULT, PCWSTR, POINT, RECT, SIZE, WPARAM,
        COLORREF,
    },
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, DIB_RGB_COLORS, HDC, HBITMAP,
        HGDIOBJ, ULW_ALPHA, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
        GetDC, ReleaseDC, SelectObject, UpdateLayeredWindow,
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Memory::{GlobalLock, GlobalSize, GlobalUnlock, HGLOBAL},
    },
    UI::{
        HiDpi::GetDpiForWindow,
        Input::KeyboardAndMouse::{ReleaseCapture, SetCapture},
        WindowsAndMessaging::{
            CS_HREDRAW, CS_VREDRAW, CREATESTRUCTW, GWLP_USERDATA, MONITORINFO,
            MONITOR_DEFAULTTOPRIMARY, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOWNOACTIVATE,
            SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, WM_DESTROY, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
            WS_EX_TOPMOST, WS_POPUP, WNDCLASSEXW, CreateWindowExW, DefWindowProcW,
            DispatchMessageW, GetCursorPos, GetDesktopWindow, GetMessageW, GetMonitorInfoW,
            GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, MonitorFromWindow,
            PostQuitMessage, RegisterClassExW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
            TranslateMessage,
        },
    },
};

pub static CIRCLE_HWND: AtomicIsize = AtomicIsize::new(0);

const W: i32 = 72;
const H: i32 = 72;

const COL_BG: (u8, u8, u8) = (0x1a, 0x1a, 0x2e);
const COL_HOVER: (u8, u8, u8) = (0x2a, 0x2a, 0x4e);
const COL_RING: (u8, u8, u8) = (0x4a, 0xde, 0x80);

// ── raw COM types ─────────────────────────────────────────────────────────────

const DROPEFFECT_COPY: u32 = 1;
const TYMED_HGLOBAL: u32 = 1;
const DVASPECT_CONTENT: u32 = 1;
const CF_UNICODETEXT: u16 = 13;
const CF_TEXT: u16 = 1;
const S_OK: i32 = 0;
const COINIT_APARTMENTTHREADED: u32 = 0x2;

#[repr(C)]
#[derive(PartialEq)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

// {00000000-0000-0000-C000-000000000046}
const IID_IUNKNOWN: Guid = Guid {
    data1: 0,
    data2: 0,
    data3: 0,
    data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
};

// {00000122-0000-0000-C000-000000000046}
const IID_IDROPTARGET: Guid = Guid {
    data1: 0x0000_0122,
    data2: 0,
    data3: 0,
    data4: [0xC0, 0, 0, 0, 0, 0, 0, 0x46],
};

#[repr(C)]
struct FormatEtcRaw {
    cf_format: u16,
    ptd: *mut c_void,
    dw_aspect: u32,
    lindex: i32,
    tymed: u32,
}

// Mirrors Windows STGMEDIUM (tymed + union[ptr-sized] + pUnkForRelease)
#[repr(C)]
struct StgMediumRaw {
    tymed: u32,
    data: *mut c_void, // hGlobal when tymed == TYMED_HGLOBAL
    pUnkForRelease: *mut c_void,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct PointlRaw {
    x: i32,
    y: i32,
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *const c_void, coinit: u32) -> i32;
    fn CoUninitialize();
    fn RegisterDragDrop(hwnd: HWND, pdt: *mut DropTargetCom) -> i32;
    fn RevokeDragDrop(hwnd: HWND) -> i32;
    fn ReleaseStgMedium(p: *mut StgMediumRaw);
}

// ── IDropTarget COM object (raw vtable) ───────────────────────────────────────

type QiFn  = unsafe extern "system" fn(*mut DropTargetCom, *const Guid, *mut *mut c_void) -> i32;
type RefFn = unsafe extern "system" fn(*mut DropTargetCom) -> u32;
type DragEnterFn = unsafe extern "system" fn(*mut DropTargetCom, *mut c_void, u32, PointlRaw, *mut u32) -> i32;
type DragOverFn  = unsafe extern "system" fn(*mut DropTargetCom, u32, PointlRaw, *mut u32) -> i32;
type DragLeaveFn = unsafe extern "system" fn(*mut DropTargetCom) -> i32;
type DropFn      = unsafe extern "system" fn(*mut DropTargetCom, *mut c_void, u32, PointlRaw, *mut u32) -> i32;

#[repr(C)]
struct IDropTargetVtbl {
    query_interface: QiFn,
    add_ref: RefFn,
    release: RefFn,
    drag_enter: DragEnterFn,
    drag_over: DragOverFn,
    drag_leave: DragLeaveFn,
    drop: DropFn,
}

#[repr(C)]
struct DropTargetCom {
    vtbl: *const IDropTargetVtbl,
    ref_count: std::sync::atomic::AtomicU32,
    app: tauri::AppHandle,
}

static DROP_TARGET_VTBL: IDropTargetVtbl = IDropTargetVtbl {
    query_interface: dt_query_interface,
    add_ref: dt_add_ref,
    release: dt_release,
    drag_enter: dt_drag_enter,
    drag_over: dt_drag_over,
    drag_leave: dt_drag_leave,
    drop: dt_drop,
};

fn create_drop_target(app: tauri::AppHandle) -> *mut DropTargetCom {
    Box::into_raw(Box::new(DropTargetCom {
        vtbl: &DROP_TARGET_VTBL,
        ref_count: std::sync::atomic::AtomicU32::new(1),
        app,
    }))
}

unsafe extern "system" fn dt_query_interface(
    this: *mut DropTargetCom,
    riid: *const Guid,
    ppv: *mut *mut c_void,
) -> i32 {
    const E_POINTER: i32 = -2147467261;
    const E_NOINTERFACE: i32 = -2147467262;
    if ppv.is_null() { return E_POINTER; }
    if &*riid == &IID_IUNKNOWN || &*riid == &IID_IDROPTARGET {
        *ppv = this as *mut c_void;
        dt_add_ref(this);
        S_OK
    } else {
        *ppv = std::ptr::null_mut();
        E_NOINTERFACE
    }
}

unsafe extern "system" fn dt_add_ref(this: *mut DropTargetCom) -> u32 {
    (*this).ref_count.fetch_add(1, Ordering::Relaxed) + 1
}

unsafe extern "system" fn dt_release(this: *mut DropTargetCom) -> u32 {
    let prev = (*this).ref_count.fetch_sub(1, Ordering::Release);
    if prev == 1 {
        std::sync::atomic::fence(Ordering::Acquire);
        drop(Box::from_raw(this));
    }
    prev - 1
}

unsafe extern "system" fn dt_drag_enter(
    _this: *mut DropTargetCom, _pdata: *mut c_void, _key: u32,
    _pt: PointlRaw, pdweffect: *mut u32,
) -> i32 {
    if !pdweffect.is_null() { *pdweffect = DROPEFFECT_COPY; }
    S_OK
}

unsafe extern "system" fn dt_drag_over(
    _this: *mut DropTargetCom, _key: u32,
    _pt: PointlRaw, pdweffect: *mut u32,
) -> i32 {
    if !pdweffect.is_null() { *pdweffect = DROPEFFECT_COPY; }
    S_OK
}

unsafe extern "system" fn dt_drag_leave(_this: *mut DropTargetCom) -> i32 { S_OK }

unsafe extern "system" fn dt_drop(
    this: *mut DropTargetCom, pdata: *mut c_void, _key: u32,
    _pt: PointlRaw, pdweffect: *mut u32,
) -> i32 {
    if !pdweffect.is_null() { *pdweffect = DROPEFFECT_COPY; }
    if !pdata.is_null() {
        if let Some(text) = extract_text_from_data_obj(pdata) {
            let t = text.trim().to_string();
            if !t.is_empty() {
                let app = (*this).app.clone();
                let id = Uuid::new_v4().to_string();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::start_download_inner(app, id, t).await;
                });
            }
        }
    }
    S_OK
}

unsafe fn extract_text_from_data_obj(pdata: *mut c_void) -> Option<String> {
    for (cf, is_unicode) in [(CF_UNICODETEXT, true), (CF_TEXT, false)] {
        let fmt = FormatEtcRaw {
            cf_format: cf,
            ptd: std::ptr::null_mut(),
            dw_aspect: DVASPECT_CONTENT,
            lindex: -1,
            tymed: TYMED_HGLOBAL,
        };
        let mut med = StgMediumRaw {
            tymed: 0,
            data: std::ptr::null_mut(),
            pUnkForRelease: std::ptr::null_mut(),
        };

        // IDataObject vtable: QueryInterface(0), AddRef(1), Release(2), GetData(3)
        type GetDataFn = unsafe extern "system" fn(
            *mut c_void, *const FormatEtcRaw, *mut StgMediumRaw,
        ) -> i32;
        let vtbl = *(pdata as *mut *const *const c_void);
        let get_data: GetDataFn = std::mem::transmute(*vtbl.add(3));

        if get_data(pdata, &fmt, &mut med) == S_OK
            && med.tymed == TYMED_HGLOBAL
            && !med.data.is_null()
        {
            let hg = HGLOBAL(med.data);
            let ptr = GlobalLock(hg);
            if !ptr.is_null() {
                let size = GlobalSize(hg);
                let text = if is_unicode {
                    let n = size / 2;
                    let s = std::slice::from_raw_parts(ptr as *const u16, n);
                    String::from_utf16_lossy(s)
                } else {
                    let s = std::slice::from_raw_parts(ptr as *const u8, size);
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

// ── rendering ────────────────────────────────────────────────────────────────

struct CircleState {
    app: tauri::AppHandle,
    mouse_down: bool,
    dragging: bool,
    cursor_start: POINT,
    win_start: POINT,
}

fn set_pixel(pixels: &mut [u32], x: i32, y: i32, r: u8, g: u8, b: u8, a: u8) {
    if x < 0 || y < 0 || x >= W || y >= H { return; }
    let pa = a as u32;
    let pr = (r as u32 * pa / 255) as u8;
    let pg = (g as u32 * pa / 255) as u8;
    let pb = (b as u32 * pa / 255) as u8;
    // BGRA in memory, pre-multiplied alpha
    pixels[(y * W + x) as usize] =
        (pa << 24) | ((pr as u32) << 16) | ((pg as u32) << 8) | pb as u32;
}

unsafe fn redraw(hwnd: HWND, progress: f32, hover: bool) {
    let screen_dc = GetDC(HWND(std::ptr::null_mut()));
    let mem_dc = CreateCompatibleDC(screen_dc);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: W,
            biHeight: -H,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    let hbmp = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        .unwrap_or_default();
    if hbmp.is_invalid() {
        DeleteDC(mem_dc);
        ReleaseDC(HWND(std::ptr::null_mut()), screen_dc);
        return;
    }
    let old = SelectObject(mem_dc, hbmp);

    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (W * H) as usize);

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

    // Progress ring (green arc)
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
                } else {
                    0
                };
                if base_a > 128 {
                    let (rr, rg, rb) = COL_RING;
                    set_pixel(pixels, px, py, rr, rg, rb, base_a);
                }
            }
        }
    }

    let blend = BLENDFUNCTION {
        BlendOp: 0,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: 1,
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
                RevokeDragDrop(hwnd);
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
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED);

        let hmod = GetModuleHandleW(None).unwrap_or_default();
        let class_name: Vec<u16> = "djdrop_circle\0".encode_utf16().collect();
        let window_title: Vec<u16> = "djdrop\0".encode_utf16().collect();

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: HINSTANCE(hmod.0),
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        RegisterClassExW(&wc);

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
            HINSTANCE(hmod.0),
            Some(state_ptr as *const c_void),
        ).unwrap();

        std::mem::forget(state); // owned by WNDPROC via GWLP_USERDATA

        CIRCLE_HWND.store(hwnd.0 as isize, Ordering::Relaxed);

        // Register OLE drop target (RegisterDragDrop calls AddRef)
        let dt = create_drop_target(app);
        RegisterDragDrop(hwnd, dt);
        dt_release(dt); // release our ref; OLE holds its own

        redraw(hwnd, 0.0, false);
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let mut msg = MSG::default();
        loop {
            match GetMessageW(&mut msg, None, 0, 0).0 {
                0 | -1 => break,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }

        CIRCLE_HWND.store(0, Ordering::Relaxed);
        CoUninitialize();
    }
}

fn primary_monitor_circle_pos() -> (i32, i32) {
    unsafe {
        let hwnd_desktop = GetDesktopWindow();
        let dpi = GetDpiForWindow(hwnd_desktop);
        let scale = if dpi == 0 { 1.0f32 } else { dpi as f32 / 96.0 };
        let phys_w = (W as f32 * scale).round() as i32;
        let phys_h = (H as f32 * scale).round() as i32;

        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let hmon = MonitorFromWindow(hwnd_desktop, MONITOR_DEFAULTTOPRIMARY);
        if GetMonitorInfoW(hmon, &mut info).as_bool() {
            let wa = info.rcWork;
            return (wa.right - phys_w - 12, wa.bottom - phys_h - 12);
        }

        let sw = GetSystemMetrics(SM_CXSCREEN);
        let sh = GetSystemMetrics(SM_CYSCREEN);
        (sw - phys_w - 12, sh - phys_h - 60)
    }
}
