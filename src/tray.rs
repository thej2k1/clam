use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicU32, Ordering};

use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{autostart, power, state};

const WM_TRAY: u32 = WM_APP + 1;

// Set once in run() to the value the shell broadcasts when the taskbar
// (re)appears — e.g. after explorer.exe restarts. We re-add our icon then.
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

const ID_STATUS: usize = 100;
const ID_RESET: usize = 101;
const ID_AUTOSTART: usize = 102;
const ID_QUIT: usize = 103;

struct App {
    hwnd: HWND,
    active: bool,
    icon_on: HICON,
    icon_off: HICON,
}

// ---------------------------------------------------------------------------
// Icon generation: solid red circle (active) vs green hollow ring (inactive).
// Shape differs (filled vs outline) so state is unambiguous even for
// red-green colorblind users. Both are 32x32 with anti-aliased edges.
// ---------------------------------------------------------------------------

fn gen_solid_circle(r: u8, g: u8, b: u8, radius: f32) -> Vec<u8> {
    let s = 32usize;
    let mut px = vec![0u8; s * s * 4];
    let c = 15.5f32;
    for y in 0..s {
        for x in 0..s {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            let a = (radius + 0.7 - d).clamp(0.0, 1.0);
            if a > 0.0 {
                let i = (y * s + x) * 4;
                px[i] = r;
                px[i + 1] = g;
                px[i + 2] = b;
                px[i + 3] = (a * 255.0) as u8;
            }
        }
    }
    px
}

fn gen_ring(r: u8, g: u8, b: u8, r_out: f32, r_in: f32) -> Vec<u8> {
    let s = 32usize;
    let mut px = vec![0u8; s * s * 4];
    let c = 15.5f32;
    for y in 0..s {
        for x in 0..s {
            let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt();
            let outer = (r_out + 0.7 - d).clamp(0.0, 1.0);
            let inner = (d - r_in + 0.7).clamp(0.0, 1.0);
            let a = outer * inner;
            if a > 0.0 {
                let i = (y * s + x) * 4;
                px[i] = r;
                px[i + 1] = g;
                px[i + 2] = b;
                px[i + 3] = (a * 255.0) as u8;
            }
        }
    }
    px
}

unsafe fn rgba_to_hicon(rgba: &[u8]) -> HICON {
    let (w, h) = (32i32, 32i32);
    let hdc = GetDC(None);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: 0,
            ..Default::default()
        },
        bmiColors: [Default::default()],
    };

    let mut bits: *mut c_void = std::ptr::null_mut();
    let hbm = CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0)
        .expect("CreateDIBSection");

    let n = (w * h) as usize;
    let dst = std::slice::from_raw_parts_mut(bits as *mut u8, n * 4);
    for i in 0..n {
        let a = rgba[i * 4 + 3] as f32 / 255.0;
        dst[i * 4] = (rgba[i * 4 + 2] as f32 * a) as u8;
        dst[i * 4 + 1] = (rgba[i * 4 + 1] as f32 * a) as u8;
        dst[i * 4 + 2] = (rgba[i * 4] as f32 * a) as u8;
        dst[i * 4 + 3] = rgba[i * 4 + 3];
    }

    let mask = CreateBitmap(w, h, 1, 1, None);
    let ii = ICONINFO {
        fIcon: BOOL(1),
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: hbm,
    };
    let icon = CreateIconIndirect(&ii).expect("CreateIconIndirect");
    let _ = DeleteObject(hbm.into());
    let _ = DeleteObject(mask.into());
    ReleaseDC(None, hdc);
    icon
}

// ---------------------------------------------------------------------------
// Shell_NotifyIcon wrappers
// ---------------------------------------------------------------------------

fn wide(dst: &mut [u16], s: &str) {
    let last = dst.len() - 1;
    for (i, c) in s.encode_utf16().enumerate() {
        if i >= last {
            break;
        }
        dst[i] = c;
    }
    dst[last] = 0;
}

unsafe fn tray_add(hwnd: HWND, icon: HICON, tip: &str) {
    let mut n: NOTIFYICONDATAW = std::mem::zeroed();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n.uFlags = NIF_ICON | NIF_TIP | NIF_MESSAGE;
    n.uCallbackMessage = WM_TRAY;
    n.hIcon = icon;
    wide(&mut n.szTip, tip);
    let _ = Shell_NotifyIconW(NIM_ADD, &n);
}

unsafe fn tray_set(hwnd: HWND, icon: HICON, tip: &str) {
    let mut n: NOTIFYICONDATAW = std::mem::zeroed();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n.uFlags = NIF_ICON | NIF_TIP;
    n.hIcon = icon;
    wide(&mut n.szTip, tip);
    let _ = Shell_NotifyIconW(NIM_MODIFY, &n);
}

unsafe fn tray_del(hwnd: HWND) {
    let mut n: NOTIFYICONDATAW = std::mem::zeroed();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    let _ = Shell_NotifyIconW(NIM_DELETE, &n);
}

unsafe fn balloon(hwnd: HWND, title: &str, body: &str) {
    let mut n: NOTIFYICONDATAW = std::mem::zeroed();
    n.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    n.hWnd = hwnd;
    n.uID = 1;
    n.uFlags = NIF_INFO;
    wide(&mut n.szInfoTitle, title);
    wide(&mut n.szInfo, body);
    n.dwInfoFlags = NIIF_INFO;
    let _ = Shell_NotifyIconW(NIM_MODIFY, &n);
}

// ---------------------------------------------------------------------------
// Toggle, reset, quit
// ---------------------------------------------------------------------------

fn toggle(app: &mut App) {
    if !app.active {
        let current = match power::read_current_settings() {
            Ok(v) => v,
            Err(c) => {
                unsafe { balloon(app.hwnd, "Clam Error", &format!("Read failed ({c})")) };
                return;
            }
        };
        if let Err(e) = state::save_state(&current) {
            unsafe { balloon(app.hwnd, "Clam Error", &e) };
            return;
        }
        match power::write_stay_awake() {
            Ok(()) => {
                app.active = true;
                unsafe {
                    tray_set(
                        app.hwnd,
                        app.icon_on,
                        "Clam: ON \u{2014} staying awake with lid shut",
                    );
                    balloon(
                        app.hwnd,
                        "Clam",
                        "Stay-awake ON \u{2014} staying awake with lid shut",
                    );
                }
            }
            Err(c) => {
                state::clear_state();
                let msg = if power::is_access_denied(c) {
                    "Access denied. Try running as administrator.".into()
                } else {
                    format!("Write failed ({c})")
                };
                unsafe { balloon(app.hwnd, "Clam Error", &msg) };
            }
        }
    } else {
        let saved = state::load_state();
        let res = match &saved {
            Some(s) => power::write_settings(s),
            None => power::write_defaults(),
        };
        match res {
            Ok(()) => {
                state::clear_state();
                app.active = false;
                let note = if saved.is_some() {
                    "Stay-awake OFF \u{2014} normal sleep restored"
                } else {
                    "Stay-awake OFF \u{2014} reset to defaults (state file missing)"
                };
                unsafe {
                    tray_set(app.hwnd, app.icon_off, "Clam: OFF \u{2014} normal sleep");
                    balloon(app.hwnd, "Clam", note);
                }
            }
            Err(c) => {
                unsafe { balloon(app.hwnd, "Clam Error", &format!("Restore failed ({c})")) };
            }
        }
    }
}

fn reset(app: &mut App) {
    match power::write_defaults() {
        Ok(()) => {
            state::clear_state();
            app.active = false;
            unsafe {
                tray_set(app.hwnd, app.icon_off, "Clam: OFF \u{2014} normal sleep");
                balloon(app.hwnd, "Clam", "Reset to normal defaults");
            }
        }
        Err(c) => {
            unsafe { balloon(app.hwnd, "Clam Error", &format!("Reset failed ({c})")) };
        }
    }
}

fn quit(app: &mut App) {
    if app.active {
        match state::load_state() {
            Some(s) => {
                let _ = power::write_settings(&s);
            }
            None => {
                let _ = power::write_defaults();
            }
        }
        state::clear_state();
    }
    unsafe {
        tray_del(app.hwnd);
        PostQuitMessage(0);
    }
}

// ---------------------------------------------------------------------------
// Context menu
// ---------------------------------------------------------------------------

// Takes the raw pointer rather than `&mut App`: TrackPopupMenu runs its own
// modal message loop that can re-enter wnd_proc, so we must NOT hold any borrow
// of App across it. We copy the two fields we need first, then release the
// borrow before entering the modal call.
unsafe fn show_menu(p: *mut App) {
    let (active, hwnd) = {
        let app = &*p;
        (app.active, app.hwnd)
    };

    let hm = CreatePopupMenu().expect("CreatePopupMenu");

    let label: Vec<u16> = if active {
        "Status: STAY-AWAKE (active)"
    } else {
        "Status: Normal (inactive)"
    }
    .encode_utf16()
    .chain(std::iter::once(0))
    .collect();
    let _ = AppendMenuW(hm, MF_STRING | MF_GRAYED, ID_STATUS, PCWSTR(label.as_ptr()));

    let _ = AppendMenuW(hm, MF_SEPARATOR, 0, PCWSTR(std::ptr::null()));

    let _ = AppendMenuW(hm, MF_STRING, ID_RESET, w!("Reset to normal defaults"));

    let chk = if autostart::is_autostart_enabled() {
        MF_STRING | MF_CHECKED
    } else {
        MF_STRING | MF_UNCHECKED
    };
    let _ = AppendMenuW(hm, chk, ID_AUTOSTART, w!("Start with Windows"));

    let _ = AppendMenuW(hm, MF_SEPARATOR, 0, PCWSTR(std::ptr::null()));
    let _ = AppendMenuW(hm, MF_STRING, ID_QUIT, w!("Quit"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);
    let _ = SetForegroundWindow(hwnd);
    // Menu selection arrives later as a posted WM_COMMAND, so we need nothing
    // from App after the menu closes.
    let _ = TrackPopupMenu(hm, TPM_RIGHTBUTTON, pt.x, pt.y, None, hwnd, None);
    let _ = PostMessageW(Some(hwnd), 0, WPARAM(0), LPARAM(0));
    let _ = DestroyMenu(hm);
}

// ---------------------------------------------------------------------------
// Window procedure
// ---------------------------------------------------------------------------

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    if msg == WM_CREATE {
        let cs = &*(lp.0 as *const CREATESTRUCTW);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
        return LRESULT(0);
    }

    let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut App;
    if p.is_null() {
        return DefWindowProcW(hwnd, msg, wp, lp);
    }

    // Shell restarted (e.g. explorer.exe crashed): re-add our tray icon.
    let taskbar_created = TASKBAR_CREATED.load(Ordering::Relaxed);
    if taskbar_created != 0 && msg == taskbar_created {
        let app = &mut *p;
        if app.active {
            tray_add(
                app.hwnd,
                app.icon_on,
                "Clam: ON \u{2014} staying awake with lid shut",
            );
        } else {
            tray_add(app.hwnd, app.icon_off, "Clam: OFF \u{2014} normal sleep");
        }
        return LRESULT(0);
    }

    // Each handler forms its own short-lived `&mut App`. None of toggle/reset/
    // quit pump messages, so those borrows can't overlap. show_menu takes the
    // raw pointer because it enters a modal loop (see its doc comment).
    match msg {
        WM_TRAY => {
            match lp.0 as u32 {
                WM_LBUTTONUP => toggle(&mut *p),
                WM_RBUTTONUP => show_menu(p),
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wp.0 & 0xFFFF {
                ID_RESET => reset(&mut *p),
                ID_AUTOSTART => {
                    autostart::set_autostart(!autostart::is_autostart_enabled());
                }
                ID_QUIT => quit(&mut *p),
                _ => {}
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wp, lp),
    }
}

// ---------------------------------------------------------------------------
// Entry: create icons, register class, run message pump
// ---------------------------------------------------------------------------

pub fn run() {
    unsafe {
        let icon_on = rgba_to_hicon(&gen_solid_circle(220, 50, 50, 12.5));
        let icon_off = rgba_to_hicon(&gen_ring(50, 180, 50, 12.5, 8.5));

        let cls = w!("ClamClass");
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            lpszClassName: cls,
            ..Default::default()
        };
        RegisterClassExW(&wc);

        // Remember the shell's "taskbar (re)created" broadcast so wnd_proc can
        // re-add the icon after an explorer.exe restart.
        TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

        let app = Box::new(App {
            hwnd: HWND::default(),
            active: false,
            icon_on,
            icon_off,
        });
        let raw = Box::into_raw(app);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            cls,
            w!("LidStay"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            None,
            Some(raw as *const c_void),
        )
        .expect("CreateWindowExW");

        (*raw).hwnd = hwnd;
        tray_add(hwnd, icon_off, "Clam: OFF \u{2014} normal sleep");

        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            if ret.0 <= 0 {
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = DestroyIcon(icon_on);
        let _ = DestroyIcon(icon_off);
        drop(Box::from_raw(raw));
    }
}
