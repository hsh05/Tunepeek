#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod lyrics;
mod media;
mod mouse;
mod settings;

use media::{MediaState, Tracker};
use settings::Settings;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewWindow};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, ShortcutState};

const FLYOUT_W: f64 = 380.0;

struct AppState {
    settings: Mutex<Settings>,
    media: Mutex<Tracker>,
    visible: AtomicBool,
}

fn sources(s: &Settings) -> Vec<String> {
    s.sources
        .split(',')
        .map(|x| x.trim().to_lowercase())
        .filter(|x| !x.is_empty())
        .collect()
}

// ---------------------------------------------------------------- flyout ---

fn apply_blur(w: &WebviewWindow) {
    let dark = !matches!(w.theme(), Ok(tauri::Theme::Light));
    let tint = if dark { (20, 20, 24, 90) } else { (245, 245, 248, 110) };
    let _ = window_vibrancy::apply_acrylic(w, Some(tint));
}

fn place(app: &AppHandle, w: &WebviewWindow, s: &Settings, height: f64) -> Option<(i32, i32, i32, i32)> {
    let monitors = app.available_monitors().unwrap_or_default();
    let under_cursor = app.cursor_position().ok().and_then(|c| {
        monitors
            .iter()
            .find(|m| {
                let (p, z) = (m.position(), m.size());
                c.x >= p.x as f64
                    && c.x < p.x as f64 + z.width as f64
                    && c.y >= p.y as f64
                    && c.y < p.y as f64 + z.height as f64
            })
            .cloned()
    });
    let mon = if s.monitor == "primary" { None } else { under_cursor }
        .or_else(|| app.primary_monitor().ok().flatten());
    let m = mon?;

    let sf = m.scale_factor();
    let wa = m.work_area();
    let (ax, ay) = (wa.position.x, wa.position.y);
    let (aw, ah) = (wa.size.width as i32, wa.size.height as i32);
    let (ww, wh) = ((FLYOUT_W * sf).round() as i32, (height * sf).round() as i32);
    let mg = (s.margin as f64 * sf).round() as i32;

    let x = if s.corner.contains("left") {
        ax + mg
    } else if s.corner.contains("right") {
        ax + aw - ww - mg
    } else {
        ax + (aw - ww) / 2
    };
    let y = if s.corner.contains("top") {
        ay + mg
    } else if s.corner.contains("bottom") {
        ay + ah - wh - mg
    } else {
        ay + (ah - wh) / 2
    };

    // Move first, then size in physical pixels, then move again: avoids the
    // DPI-change resize when hopping between monitors with different scaling.
    let _ = w.set_position(PhysicalPosition::new(x, y));
    let _ = w.set_size(PhysicalSize::new(ww as u32, wh as u32));
    let _ = w.set_position(PhysicalPosition::new(x, y));
    Some((x, y, ww, wh))
}

fn show_flyout(app: &AppHandle) {
    let Some(w) = app.get_webview_window("flyout") else { return };
    let st = app.state::<AppState>();
    let s = st.settings.lock().unwrap().clone();
    let height = if s.lyrics { 172.0 } else { 140.0 };
    if let Some((x, y, fw, fh)) = place(app, &w, &s, height) {
        mouse::set_rect(x, y, fw, fh);
    }
    apply_blur(&w);
    let _ = w.set_always_on_top(true);
    let _ = w.show();
    st.visible.store(true, Ordering::SeqCst);
    mouse::OPEN.store(true, Ordering::SeqCst);
    // Escape only exists as a global hotkey while the flyout is visible,
    // because the flyout never takes keyboard focus.
    let _ = app.global_shortcut().register("Escape");
    let _ = app.emit("flyout-shown", ());
}

fn hide_flyout_inner(app: &AppHandle) {
    let st = app.state::<AppState>();
    if !st.visible.swap(false, Ordering::SeqCst) {
        return;
    }
    mouse::OPEN.store(false, Ordering::SeqCst);
    let _ = app.global_shortcut().unregister("Escape");
    let _ = app.emit("flyout-hiding", ());
    let a = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(170));
        if !a.state::<AppState>().visible.load(Ordering::SeqCst) {
            if let Some(w) = a.get_webview_window("flyout") {
                let _ = w.hide();
            }
        }
    });
}

fn toggle_flyout_inner(app: &AppHandle) {
    if app.state::<AppState>().visible.load(Ordering::SeqCst) {
        hide_flyout_inner(app);
    } else {
        show_flyout(app);
    }
}

fn open_settings_inner(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

// -------------------------------------------------------------- commands ---
// Anything touching WinRT/COM is async so it runs off the main (STA) thread.

#[tauri::command]
async fn get_state(state: State<'_, AppState>) -> Result<Option<MediaState>, String> {
    let src = sources(&state.settings.lock().unwrap());
    let mut t = state.media.lock().unwrap();
    Ok(media::snapshot(&mut t, &src))
}

#[tauri::command]
async fn get_art(state: State<'_, AppState>) -> Result<Option<String>, String> {
    Ok(state.media.lock().unwrap().art())
}

#[tauri::command]
async fn play_pause(id: String) -> bool {
    media::play_pause(&id)
}
#[tauri::command]
async fn next(id: String) -> bool {
    media::next(&id)
}
#[tauri::command]
async fn prev(id: String) -> bool {
    media::prev(&id)
}
#[tauri::command]
async fn seek(id: String, ms: i64) -> bool {
    media::seek(&id, ms)
}
#[tauri::command]
async fn set_volume(id: String, level: f32) -> bool {
    audio::set_volume(&id, level)
}

#[tauri::command]
async fn fetch_lyrics(artist: String, title: String, album: String, duration_s: u32) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || lyrics::fetch(&artist, &title, &album, duration_s))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn hide_flyout(app: AppHandle) {
    hide_flyout_inner(&app);
}
#[tauri::command]
fn toggle_flyout(app: AppHandle) {
    toggle_flyout_inner(&app);
}
/// Called while the settings page records a new shortcut, so the current one
/// doesn't swallow the keypress (or mouse click) before the page sees it.
#[tauri::command]
fn pause_hotkey(app: AppHandle, state: State<'_, AppState>) {
    let hk = state.settings.lock().unwrap().hotkey.clone();
    mouse::unbind(&app, &hk);
}
#[tauri::command]
fn resume_hotkey(app: AppHandle, state: State<'_, AppState>) {
    let hk = state.settings.lock().unwrap().hotkey.clone();
    let _ = mouse::bind(&app, &hk);
}
#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
async fn save_settings(app: AppHandle, state: State<'_, AppState>, s: Settings) -> Result<(), String> {
    let old = state.settings.lock().unwrap().clone();

    // Always unbind + rebind: the settings page may have paused the hotkey
    // while recording, so "unchanged" doesn't mean "still registered".
    mouse::unbind(&app, &old.hotkey);
    if let Err(e) = mouse::bind(&app, &s.hotkey) {
        let _ = mouse::bind(&app, &old.hotkey);
        return Err(format!("Can't use that hotkey: {e}"));
    }
    if s.autostart != old.autostart {
        let al = app.autolaunch();
        let r = if s.autostart { al.enable() } else { al.disable() };
        r.map_err(|e| e.to_string())?;
    }
    settings::save(&app, &s)?;
    *state.settings.lock().unwrap() = s;
    let _ = app.emit("settings-changed", ());
    Ok(())
}

// ------------------------------------------------------------------ main ---

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| open_settings_inner(app)))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, sc, ev| {
                    if ev.state() != ShortcutState::Pressed {
                        return;
                    }
                    if sc.matches(Modifiers::empty(), Code::Escape) {
                        hide_flyout_inner(app);
                    } else {
                        toggle_flyout_inner(app);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_state, get_art, play_pause, next, prev, seek, set_volume, fetch_lyrics,
            hide_flyout, toggle_flyout, get_settings, save_settings, pause_hotkey, resume_hotkey
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let first_run = !settings::exists(&handle);
            let s = settings::load(&handle);
            if first_run {
                let _ = settings::save(&handle, &s); // write defaults once
            }
            let hotkey = s.hotkey.clone();
            app.manage(AppState {
                settings: Mutex::new(s),
                media: Mutex::new(Tracker::default()),
                visible: AtomicBool::new(false),
            });

            mouse::start(&handle);
            if let Err(e) = mouse::bind(&handle, &hotkey) {
                eprintln!("could not register hotkey {hotkey}: {e}");
                open_settings_inner(&handle); // let the user pick another one
            } else if first_run {
                open_settings_inner(&handle); // welcome: show settings on first launch
            }

            // Settings window: closing it just hides it.
            if let Some(sw) = app.get_webview_window("settings") {
                let sw2 = sw.clone();
                sw.on_window_event(move |e| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = e {
                        api.prevent_close();
                        let _ = sw2.hide();
                    }
                });
            }

            // Tray icon.
            let m_show = MenuItem::with_id(app, "show", "Show flyout", true, None::<&str>)?;
            let m_set = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
            let m_quit = MenuItem::with_id(app, "quit", "Quit Tunepeek", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&m_show, &m_set, &m_quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("Tunepeek")
                .menu(&menu)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "show" => show_flyout(app),
                    "settings" => open_settings_inner(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;

            // Poll the media session. Slow while hidden (just keeps track of
            // which app was active last), faster while the flyout is open.
            std::thread::spawn(move || loop {
                let st = handle.state::<AppState>();
                let vis = st.visible.load(Ordering::SeqCst);
                let src = sources(&st.settings.lock().unwrap());
                let snap = {
                    let mut t = st.media.lock().unwrap();
                    media::snapshot(&mut t, &src)
                };
                if vis {
                    let _ = handle.emit("media-state", &snap);
                }
                std::thread::sleep(Duration::from_millis(if vis { 500 } else { 1500 }));
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tunepeek");
}
