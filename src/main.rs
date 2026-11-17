//! Desktop Switcher: scroll the mouse wheel over the Windows taskbar to move
//! between virtual desktops.
//!
//! Started with no option it runs the small background helper (tray icon).
//! `--settings` opens the settings window, `--screenshot <file.png>` draws the
//! settings window, saves a picture of it and exits.

#![cfg_attr(windows, windows_subsystem = "windows")]

mod config;
#[cfg(windows)]
mod helper;
mod i18n;
mod icon;
mod startup;
mod ui;

use std::path::PathBuf;

/// UTF-16, zero-terminated, for Win32 calls.
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn helper_notify() {
    #[cfg(windows)]
    helper::notify_reload();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let screenshot = args
        .iter()
        .position(|a| a == "--screenshot")
        .and_then(|i| args.get(i + 1))
        .map(PathBuf::from);
    let welcome = args.iter().any(|a| a == "--welcome");

    #[cfg(windows)]
    if let Some(i) = args.iter().position(|a| a == "--hint-png") {
        if let Some(file) = args.get(i + 1) {
            let ctrl = args.iter().any(|a| a == "ctrl");
            let lang = if args.iter().any(|a| a == "fr") { "fr" } else { "en" };
            helper::render_hint_png(std::path::Path::new(file), ctrl, lang);
        }
        return;
    }

    if screenshot.is_some() || args.iter().any(|a| a == "--settings") {
        let _ = ui::run(ui::Options { welcome, screenshot });
        return;
    }

    #[cfg(windows)]
    helper::run();
    #[cfg(not(windows))]
    {
        let _ = ui::run(ui::Options { welcome, screenshot: None });
    }
}
