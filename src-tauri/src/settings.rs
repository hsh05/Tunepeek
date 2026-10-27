use serde::{Deserialize, Serialize};
use std::fs;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub hotkey: String,
    /// top-left | top-center | top-right | bottom-left | bottom-center | bottom-right | center
    pub corner: String,
    pub margin: i32,
    /// "cursor" or "primary"
    pub monitor: String,
    /// 0 = never auto-hide
    pub auto_hide_secs: u32,
    pub autostart: bool,
    pub lyrics: bool,
    pub accent: bool,
    /// comma separated, matched against the media session's app id
    pub sources: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            hotkey: "Ctrl+Alt+M".into(),
            corner: "bottom-right".into(),
            margin: 16,
            monitor: "cursor".into(),
            auto_hide_secs: 5,
            autostart: false,
            lyrics: true,
            accent: true,
            sources: "spotify".into(),
        }
    }
}

fn path(app: &AppHandle) -> Option<std::path::PathBuf> {
    app.path().app_config_dir().ok().map(|p| p.join("settings.json"))
}

pub fn exists(app: &AppHandle) -> bool {
    path(app).map(|p| p.exists()).unwrap_or(false)
}

pub fn load(app: &AppHandle) -> Settings {
    path(app)
        .and_then(|p| fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(app: &AppHandle, s: &Settings) -> Result<(), String> {
    let p = path(app).ok_or("no config dir")?;
    if let Some(dir) = p.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    fs::write(p, serde_json::to_string_pretty(s).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}
