//! Per-app volume through the Windows audio session API (same thing the
//! Windows volume mixer uses). Works for Spotify and for the browser process.
use std::cell::Cell;
use windows::core::{Interface, GUID, PWSTR};
use windows::Win32::Foundation::{CloseHandle, FALSE};
use windows::Win32::Media::Audio::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::Threading::*;

thread_local! { static INIT: Cell<bool> = Cell::new(false); }

/// Make sure COM is initialised (multithreaded) on the calling thread.
pub fn com_init() {
    INIT.with(|i| {
        if !i.get() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            i.set(true);
        }
    });
}

fn exe_stem(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid).ok()?;
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
        let _ = CloseHandle(h);
        r.ok()?;
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let file = path.rsplit('\\').next()?.to_lowercase();
        Some(file.trim_end_matches(".exe").to_string())
    }
}

/// All audio sessions whose process name appears inside the media session's
/// app id (e.g. "Spotify.exe" -> spotify.exe, "...brave..." -> brave.exe).
fn volumes_for(app_id: &str) -> windows::core::Result<Vec<ISimpleAudioVolume>> {
    com_init();
    let id = app_id.to_lowercase();
    let mut out = Vec::new();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let mgr: IAudioSessionManager2 = dev.Activate(CLSCTX_ALL, None)?;
        let list = mgr.GetSessionEnumerator()?;
        for i in 0..list.GetCount()? {
            let ctl = list.GetSession(i)?;
            let ctl2: IAudioSessionControl2 = ctl.cast()?;
            let pid = ctl2.GetProcessId().unwrap_or(0);
            if pid == 0 {
                continue;
            }
            if let Some(stem) = exe_stem(pid) {
                if stem.len() >= 4 && id.contains(&stem) {
                    if let Ok(v) = ctl.cast::<ISimpleAudioVolume>() {
                        out.push(v);
                    }
                }
            }
        }
    }
    Ok(out)
}

pub fn get_volume(app_id: &str) -> Option<f32> {
    let vols = volumes_for(app_id).ok()?;
    vols.iter()
        .filter_map(|v| unsafe { v.GetMasterVolume().ok() })
        .fold(None, |m: Option<f32>, x| Some(m.map_or(x, |y| y.max(x))))
}

pub fn set_volume(app_id: &str, level: f32) -> bool {
    let level = level.clamp(0.0, 1.0);
    match volumes_for(app_id) {
        Ok(vols) if !vols.is_empty() => {
            for v in vols {
                unsafe {
                    let _ = v.SetMasterVolume(level, &GUID::zeroed());
                }
            }
            true
        }
        _ => false,
    }
}
