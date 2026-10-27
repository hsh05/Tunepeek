//! Hotkey binding. Keyboard shortcuts go through the global-shortcut plugin;
//! mouse side buttons (Mouse4 = back, Mouse5 = forward) use a low-level mouse
//! hook, since the plugin can't see mouse buttons. A hotkey string is either
//! "Ctrl+Alt+M" or "Mouse4" / "Ctrl+Mouse5" (modifiers optional for mouse).
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::OnceLock;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};
use windows::Win32::UI::WindowsAndMessaging::*;

static WANT_BTN: AtomicU32 = AtomicU32::new(0); // 0 = none, 1 = XBUTTON1, 2 = XBUTTON2
static WANT_MODS: AtomicU32 = AtomicU32::new(0); // bit 1 ctrl, 2 alt, 4 shift, 8 win
static SWALLOWED: AtomicBool = AtomicBool::new(false);
static APP: OnceLock<AppHandle> = OnceLock::new();

// Flyout state mirrored here so the hook callback never has to call into Tauri.
pub static OPEN: AtomicBool = AtomicBool::new(false);
static RECT: [AtomicI32; 4] = [AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0), AtomicI32::new(0)];

/// Screen rectangle (physical pixels) currently covered by the flyout.
pub fn set_rect(x: i32, y: i32, w: i32, h: i32) {
    for (slot, v) in RECT.iter().zip([x, y, w, h]) {
        slot.store(v, Ordering::SeqCst);
    }
}

fn outside_flyout(px: i32, py: i32) -> bool {
    let [x, y, w, h] = [0, 1, 2, 3].map(|i| RECT[i].load(Ordering::SeqCst));
    px < x || px >= x + w || py < y || py >= y + h
}

fn parse_mouse(s: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    let btn = match parts.last()?.to_lowercase().as_str() {
        "mouse4" | "xbutton1" => 1,
        "mouse5" | "xbutton2" => 2,
        _ => return None,
    };
    let mut mods = 0;
    for p in &parts[..parts.len() - 1] {
        match p.to_lowercase().as_str() {
            "ctrl" | "control" => mods |= 1,
            "alt" => mods |= 2,
            "shift" => mods |= 4,
            "super" | "win" | "meta" => mods |= 8,
            _ => {}
        }
    }
    Some((btn, mods))
}

pub fn bind(app: &AppHandle, hk: &str) -> Result<(), String> {
    if let Some((btn, mods)) = parse_mouse(hk) {
        WANT_MODS.store(mods, Ordering::SeqCst);
        WANT_BTN.store(btn, Ordering::SeqCst);
        return Ok(());
    }
    let gs = app.global_shortcut();
    if gs.is_registered(hk) {
        return Ok(());
    }
    gs.register(hk).map_err(|e| e.to_string())
}

pub fn unbind(app: &AppHandle, hk: &str) {
    if parse_mouse(hk).is_some() {
        WANT_BTN.store(0, Ordering::SeqCst);
        return;
    }
    let gs = app.global_shortcut();
    if gs.is_registered(hk) {
        let _ = gs.unregister(hk);
    }
}

fn down(vk: i32) -> bool {
    unsafe { (GetAsyncKeyState(vk) as u16) & 0x8000 != 0 }
}

fn mods_now() -> u32 {
    let mut m = 0;
    if down(VK_CONTROL.0 as i32) { m |= 1; }
    if down(VK_MENU.0 as i32) { m |= 2; }
    if down(VK_SHIFT.0 as i32) { m |= 4; }
    if down(VK_LWIN.0 as i32) || down(VK_RWIN.0 as i32) { m |= 8; }
    m
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let msg = wparam.0 as u32;
        // Click anywhere outside the open flyout closes it (click is passed through).
        if matches!(msg, WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN) && OPEN.load(Ordering::SeqCst) {
            let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            if outside_flyout(info.pt.x, info.pt.y) {
                if let Some(app) = APP.get() {
                    let a = app.clone();
                    std::thread::spawn(move || crate::hide_flyout_inner(&a));
                }
            }
        }
        if msg == WM_XBUTTONDOWN || msg == WM_XBUTTONUP {
            let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            let btn = (info.mouseData >> 16) & 0xFFFF;
            let want = WANT_BTN.load(Ordering::SeqCst);
            if msg == WM_XBUTTONDOWN {
                if want != 0 && btn == want && mods_now() == WANT_MODS.load(Ordering::SeqCst) {
                    SWALLOWED.store(true, Ordering::SeqCst);
                    if let Some(app) = APP.get() {
                        let a = app.clone();
                        // Keep the hook callback instant; do the work elsewhere.
                        std::thread::spawn(move || crate::toggle_flyout_inner(&a));
                    }
                    return LRESULT(1); // swallow so the browser doesn't navigate back
                }
            } else if SWALLOWED.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

/// Installs the mouse hook on its own thread (hooks need a message loop).
pub fn start(app: &AppHandle) {
    let _ = APP.set(app.clone());
    std::thread::spawn(|| unsafe {
        let hmod = GetModuleHandleW(None).map(|h| windows::Win32::Foundation::HINSTANCE(h.0)).unwrap_or_default();
        if SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), hmod, 0).is_err() {
            eprintln!("could not install mouse hook; mouse-button hotkeys won't work");
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
