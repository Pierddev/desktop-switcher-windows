//! Settings stored as JSON in %APPDATA%\DesktopSwitcherRs\config.json.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    /// "low", "medium" or "high".
    pub sensitivity: String,
    pub visual_feedback: bool,
    /// 0.10 ..= 0.80
    pub indicator_opacity: f64,
    pub cooldown_seconds: f64,
    /// "en" or "fr".
    pub language: String,
    pub first_run: bool,
    pub auto_startup: bool,
    /// "taskbar" (whole taskbar + bottom edge) or "edge" (bottom 15 px only).
    pub zone: String,
    /// Middle click in the zone opens Task View.
    pub middle_click: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            sensitivity: "medium".into(),
            visual_feedback: true,
            indicator_opacity: 0.3,
            cooldown_seconds: 0.1,
            language: "en".into(),
            first_run: true,
            auto_startup: false,
            zone: "taskbar".into(),
            middle_click: true,
        }
    }
}

impl Config {
    /// Wheel steps needed before a switch happens.
    pub fn steps_needed(&self) -> i32 {
        match self.sensitivity.as_str() {
            "low" => 4,
            "high" => 1,
            _ => 2,
        }
    }

    pub fn alpha(&self) -> u8 {
        (self.indicator_opacity.clamp(0.1, 1.0) * 255.0) as u8
    }

    pub fn whole_taskbar(&self) -> bool {
        self.zone != "edge"
    }

    pub fn load() -> Self {
        let Some(path) = path() else { return Self::default() };
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let path = path().ok_or_else(|| std::io::Error::other("no APPDATA folder"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }
}

pub fn path() -> Option<PathBuf> {
    let base = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(base).join("DesktopSwitcherRs").join("config.json"))
}
