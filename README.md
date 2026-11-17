# Desktop Switcher

Switch Windows virtual desktops by scrolling the mouse wheel over the taskbar.

The app runs in the background with no window: a small icon appears in the notification area. Fast startup and a low memory footprint are at the heart of the project.

## Features

- Wheel over the taskbar (or the bottom 15 pixels of the screen): previous / next desktop.
- `Ctrl` + wheel: previous / next app; release `Ctrl` to confirm the selection.
- Middle click: opens Task View.
- Subtle hint bubble near the cursor, with a preview in the settings.
- Settings window: language (French / English), sensitivity, active zone, opacity, launch at Windows startup.

## Build

```bash
cargo build --release
```

The executable is at `target/release/DesktopSwitcher.exe`.

## Usage

- Run `DesktopSwitcher.exe`: the program starts in the background and the icon appears near the clock.
- Right-click the icon: **Settings** or **Exit**.
- The configuration file is stored in `%APPDATA%\DesktopSwitcherRs\config.json`.

## Requirements

- Windows 10 / 11 (x86_64).
- Rust, 2024 edition (to build from source).
