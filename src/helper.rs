//! The background helper: a tiny Win32 program with no visible window.
//! It watches the mouse wheel over the taskbar / bottom screen edge, sends the
//! Windows shortcuts that switch desktops, shows the hint bubble and the tray icon.

// Win32 callbacks operate on raw pointers throughout their bodies.
#![allow(unsafe_op_in_unsafe_fn)]

use crate::config::Config;
use crate::{i18n, icon, startup, wide};
use std::cell::{Cell, RefCell};
use std::process::Child;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::HiDpi::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
use windows_sys::Win32::UI::Shell::*;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const MAIN_CLASS: &str = "DesktopSwitcherRs.Main";
const HINT_CLASS: &str = "DesktopSwitcherRs.Indicator";

pub const WM_APP_TRAY: u32 = WM_APP + 1;
pub const WM_APP_ACTION: u32 = WM_APP + 2;
pub const WM_APP_RELOAD: u32 = WM_APP + 3;
pub const WM_APP_OPEN_SETTINGS: u32 = WM_APP + 4;

/// Bottom strip of the monitor that always counts as the zone.
const EDGE_PX: i32 = 15;
/// Cursor poll period.
const TIMER_MS: u32 = 50;
const TIMER_ID: usize = 1;

const MENU_SETTINGS: usize = 1001;
const MENU_EXIT: usize = 1002;

#[derive(Clone, Copy)]
#[repr(usize)]
enum Action {
    DesktopLeft = 1,
    DesktopRight = 2,
    AppPrev = 3,
    AppNext = 4,
    Enter = 5,
    TaskView = 6,
}

struct State {
    cfg: Config,
    main: HWND,
    ctrl_down: bool,
    app_switching: bool,
    wheel_count: i32,
    last_action: Option<Instant>,
    mbutton_in_zone: bool,
    taskbars: Vec<RECT>,
    hint: Hint,
    settings_child: Option<Child>,
    taskbar_created_msg: u32,
}

struct Hint {
    hwnd: HWND,
    title_font: HFONT,
    sub_font: HFONT,
    /// Pixels per 96-dpi pixel.
    scale: f32,
    visible: bool,
    ctrl_mode: Option<bool>,
    lang: String,
    w: i32,
    h: i32,
    pos: (i32, i32),
    alpha: u8,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    static MOUSE_HOOK: Cell<HHOOK> = const { Cell::new(std::ptr::null_mut()) };
    static KEY_HOOK: Cell<HHOOK> = const { Cell::new(std::ptr::null_mut()) };
}

fn with<R>(f: impl FnOnce(&mut State) -> R) -> Option<R> {
    STATE.with(|s| s.try_borrow_mut().ok().and_then(|mut s| s.as_mut().map(f)))
}

pub fn run() {
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // One copy only: a second launch opens the settings of the running one.
        CreateMutexW(std::ptr::null(), 0, wide("Local\\DesktopSwitcherRs.Instance").as_ptr());
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let other = FindWindowW(wide(MAIN_CLASS).as_ptr(), std::ptr::null());
            if !other.is_null() {
                AllowSetForegroundWindow(ASFW_ANY);
                PostMessageW(other, WM_APP_OPEN_SETTINGS, 0, 0);
            }
            return;
        }

        let hinst = GetModuleHandleW(std::ptr::null());
        let class = wide(MAIN_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(main_proc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        let main = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            wide("Desktop Switcher").as_ptr(),
            WS_POPUP,
            0, 0, 0, 0,
            std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null(),
        );
        if main.is_null() {
            return;
        }

        let mut cfg = Config::load();
        let first_run = cfg.first_run || crate::config::path().map_or(true, |p| !p.exists());
        if first_run {
            // Follow the Windows display language at first start.
            let lang = windows_sys::Win32::Globalization::GetUserDefaultUILanguage();
            cfg.language = if lang & 0x3ff == 0x0c { "fr".into() } else { "en".into() };
            cfg.first_run = false;
            cfg.auto_startup = startup::is_enabled();
            let _ = cfg.save();
        }

        let hint = create_hint(hinst, &cfg);
        let taskbar_created_msg = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                cfg,
                main,
                ctrl_down: false,
                app_switching: false,
                wheel_count: 0,
                last_action: None,
                mbutton_in_zone: false,
                taskbars: Vec::new(),
                hint,
                settings_child: None,
                taskbar_created_msg,
            })
        });

        refresh_taskbars();
        add_tray_icon(main, false);

        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), hinst, 0);
        let keys = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook), hinst, 0);
        MOUSE_HOOK.with(|h| h.set(mouse));
        KEY_HOOK.with(|h| h.set(keys));
        if mouse.is_null() || keys.is_null() {
            MessageBoxW(
                main,
                wide("Failed to install mouse hooks.").as_ptr(),
                wide("Desktop Switcher").as_ptr(),
                MB_ICONERROR,
            );
        }
        SetTimer(main, TIMER_ID, TIMER_MS, None);

        if first_run {
            open_settings(true);
        }

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        UnhookWindowsHookEx(MOUSE_HOOK.with(|h| h.get()));
        UnhookWindowsHookEx(KEY_HOOK.with(|h| h.get()));
        remove_tray_icon(main);
    }
}

// ---------------------------------------------------------------- the zone

fn bottom_edge(pt: POINT) -> bool {
    unsafe {
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let bottom = if GetMonitorInfoW(mon, &mut mi) != 0 {
            mi.rcMonitor.bottom
        } else {
            GetSystemMetrics(SM_CYSCREEN)
        };
        pt.y >= bottom - EDGE_PX
    }
}

fn in_zone(s: &State, pt: POINT) -> bool {
    if bottom_edge(pt) {
        return true;
    }
    s.cfg.whole_taskbar()
        && s.taskbars
            .iter()
            .any(|r| pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom)
}

fn refresh_taskbars() {
    let mut rects = Vec::new();
    unsafe {
        let mut push = |hwnd: HWND| {
            let mut r: RECT = std::mem::zeroed();
            if IsWindowVisible(hwnd) != 0 && GetWindowRect(hwnd, &mut r) != 0 {
                rects.push(r);
            }
        };
        let main = FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null());
        if !main.is_null() {
            push(main);
        }
        let class = wide("Shell_SecondaryTrayWnd");
        let mut prev: HWND = std::ptr::null_mut();
        for _ in 0..16 {
            prev = FindWindowExW(std::ptr::null_mut(), prev, class.as_ptr(), std::ptr::null());
            if prev.is_null() {
                break;
            }
            push(prev);
        }
    }
    with(|s| s.taskbars = rects);
}

// ---------------------------------------------------------------- hooks

unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let mut swallow = false;
    if code >= 0 {
        let info = &*(lparam as *const MSLLHOOKSTRUCT);
        let msg = wparam as u32;
        if info.flags & LLMHF_INJECTED == 0 {
            swallow = with(|s| on_mouse(s, msg, info)).unwrap_or(false);
        }
    }
    if swallow {
        return 1;
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

/// Returns true when the event must not reach other programs.
fn on_mouse(s: &mut State, msg: u32, info: &MSLLHOOKSTRUCT) -> bool {
    match msg {
        WM_MOUSEWHEEL => {
            if !in_zone(s, info.pt) {
                s.wheel_count = 0;
                return false;
            }
            let cooldown = Duration::from_secs_f64((s.cfg.cooldown_seconds * 1000.0).max(0.0) / 1000.0);
            if s.last_action.is_some_and(|t| t.elapsed() < cooldown) {
                return false;
            }
            let delta = (info.mouseData >> 16) as u16 as i16;
            if delta == 0 {
                return false;
            }
            s.wheel_count += if delta > 0 { 1 } else { -1 };
            if s.wheel_count.abs() >= s.cfg.steps_needed() {
                let up = s.wheel_count > 0;
                let action = if s.ctrl_down {
                    s.app_switching = true;
                    if up { Action::AppPrev } else { Action::AppNext }
                } else if up {
                    Action::DesktopLeft
                } else {
                    Action::DesktopRight
                };
                post_action(s.main, action);
                s.last_action = Some(Instant::now());
                s.wheel_count = 0;
            }
            false
        }
        WM_MBUTTONDOWN => {
            s.mbutton_in_zone = s.cfg.middle_click && in_zone(s, info.pt);
            s.mbutton_in_zone
        }
        WM_MBUTTONUP => {
            if s.mbutton_in_zone {
                s.mbutton_in_zone = false;
                post_action(s.main, Action::TaskView);
                s.last_action = Some(Instant::now());
                return true;
            }
            false
        }
        _ => false,
    }
}

unsafe extern "system" fn key_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let info = &*(lparam as *const KBDLLHOOKSTRUCT);
        let is_ctrl = matches!(info.vkCode as u16, VK_LCONTROL | VK_RCONTROL | VK_CONTROL);
        if is_ctrl && info.flags & LLKHF_INJECTED == 0 {
            match wparam as u32 {
                WM_KEYDOWN | WM_SYSKEYDOWN => {
                    with(|s| s.ctrl_down = true);
                }
                WM_KEYUP | WM_SYSKEYUP => {
                    with(|s| {
                        s.ctrl_down = false;
                        if s.app_switching {
                            s.app_switching = false;
                            post_action(s.main, Action::Enter);
                        }
                    });
                }
                _ => {}
            }
        }
    }
    CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam)
}

fn post_action(main: HWND, action: Action) {
    unsafe { PostMessageW(main, WM_APP_ACTION, action as usize, 0) };
}

// ---------------------------------------------------------------- keyboard output

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let extended = matches!(vk, VK_LEFT | VK_RIGHT | VK_LWIN);
    let mut flags = if up { KEYEVENTF_KEYUP } else { 0 };
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: unsafe { MapVirtualKeyW(vk as u32, MAPVK_VK_TO_VSC) } as u16,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// Press the modifiers in order, tap the key, release the modifiers in reverse.
fn chord(mods: &[VIRTUAL_KEY], vk: VIRTUAL_KEY) {
    let mut inputs: Vec<INPUT> = mods.iter().map(|&m| key(m, false)).collect();
    inputs.push(key(vk, false));
    inputs.push(key(vk, true));
    inputs.extend(mods.iter().rev().map(|&m| key(m, true)));
    unsafe { SendInput(inputs.len() as u32, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32) };
}

fn perform(action: usize) {
    match action {
        a if a == Action::DesktopLeft as usize => chord(&[VK_LWIN, VK_CONTROL], VK_LEFT),
        a if a == Action::DesktopRight as usize => chord(&[VK_LWIN, VK_CONTROL], VK_RIGHT),
        a if a == Action::AppPrev as usize => chord(&[VK_MENU, VK_SHIFT], VK_TAB),
        a if a == Action::AppNext as usize => chord(&[VK_MENU], VK_TAB),
        a if a == Action::Enter as usize => chord(&[], VK_RETURN),
        a if a == Action::TaskView as usize => chord(&[VK_LWIN], VK_TAB),
        _ => {}
    }
}

// ---------------------------------------------------------------- main window

unsafe extern "system" fn main_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TIMER if wparam == TIMER_ID => {
            tick();
            0
        }
        WM_APP_ACTION => {
            perform(wparam);
            0
        }
        WM_APP_RELOAD => {
            let cfg = Config::load();
            with(|s| {
                s.cfg = cfg;
                s.hint.ctrl_mode = None; // redraw with new language / opacity
            });
            update_tray_tip(hwnd);
            0
        }
        WM_APP_OPEN_SETTINGS => {
            open_settings(false);
            0
        }
        WM_APP_TRAY => {
            match (lparam as u32) & 0xffff {
                WM_LBUTTONDBLCLK | WM_LBUTTONUP => open_settings(false),
                WM_RBUTTONUP | WM_CONTEXTMENU => show_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => {
            let created = with(|s| s.taskbar_created_msg).unwrap_or(0);
            if created != 0 && msg == created {
                add_tray_icon(hwnd, false);
                return 0;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
    }
}

fn tick() {
    refresh_taskbars();
    let mut pt = POINT { x: 0, y: 0 };
    if unsafe { GetCursorPos(&mut pt) } == 0 {
        return;
    }
    with(|s| {
        if let Some(child) = s.settings_child.as_mut() {
            if !matches!(child.try_wait(), Ok(None)) {
                s.settings_child = None;
            }
        }
        let show = in_zone(s, pt);
        if !show {
            s.wheel_count = 0;
        }
        if show && s.cfg.visual_feedback {
            show_hint(s, pt);
        } else {
            hide_hint(&mut s.hint);
        }
    });
}

fn open_settings(welcome: bool) {
    with(|s| {
        if let Some(child) = s.settings_child.as_mut() {
            if matches!(child.try_wait(), Ok(None)) {
                unsafe { AllowSetForegroundWindow(child.id()) };
                focus_settings_window();
                return;
            }
        }
        if let Ok(exe) = std::env::current_exe() {
            let mut cmd = std::process::Command::new(exe);
            cmd.arg("--settings");
            if welcome {
                cmd.arg("--welcome");
            }
            if let Ok(child) = cmd.spawn() {
                unsafe { AllowSetForegroundWindow(child.id()) };
                s.settings_child = Some(child);
            }
        }
    });
}

fn focus_settings_window() {
    for title in [i18n::EN.settings_title, i18n::FR.settings_title] {
        unsafe {
            let w = FindWindowW(std::ptr::null(), wide(title).as_ptr());
            if !w.is_null() {
                ShowWindow(w, SW_RESTORE);
                SetForegroundWindow(w);
                return;
            }
        }
    }
}

fn show_menu(hwnd: HWND) {
    let lang = with(|s| s.cfg.language.clone()).unwrap_or_default();
    let t = i18n::texts(&lang);
    unsafe {
        let menu = CreatePopupMenu();
        AppendMenuW(menu, MF_STRING, MENU_SETTINGS, wide(t.settings).as_ptr());
        AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
        AppendMenuW(menu, MF_STRING, MENU_EXIT, wide(t.exit).as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY | TPM_BOTTOMALIGN,
            pt.x, pt.y, 0, hwnd, std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        match cmd as usize {
            MENU_SETTINGS => open_settings(false),
            MENU_EXIT => {
                DestroyWindow(hwnd);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- tray icon

fn make_icon(size: i32) -> HICON {
    let px = icon::rgba(size as u32);
    let mut bgra = px.clone();
    for p in bgra.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
    let mask_row = ((size + 15) / 16 * 2) as usize;
    let mask = vec![0u8; mask_row * size as usize];
    unsafe {
        CreateIcon(
            GetModuleHandleW(std::ptr::null()),
            size, size, 1, 32,
            mask.as_ptr(), bgra.as_ptr(),
        )
    }
}

fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = 1;
    nid
}

fn set_tip(nid: &mut NOTIFYICONDATAW) {
    let tip = wide("Desktop Switcher");
    let n = tip.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&tip[..n]);
}

fn add_tray_icon(hwnd: HWND, _retry: bool) {
    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_APP_TRAY;
    nid.hIcon = make_icon(unsafe { GetSystemMetrics(SM_CXSMICON) }.max(16));
    set_tip(&mut nid);
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
}

fn update_tray_tip(hwnd: HWND) {
    let mut nid = tray_data(hwnd);
    nid.uFlags = NIF_TIP;
    set_tip(&mut nid);
    unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
}

fn remove_tray_icon(hwnd: HWND) {
    let nid = tray_data(hwnd);
    unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

// ---------------------------------------------------------------- hint bubble

fn system_scale() -> f32 {
    unsafe { GetDpiForSystem() as f32 / 96.0 }
}

fn make_font(points: f32, weight: i32) -> HFONT {
    unsafe {
        let dpi = GetDpiForSystem() as f32;
        CreateFontW(
            -((points * dpi / 72.0).round() as i32), 0, 0, 0, weight, 0, 0, 0,
            DEFAULT_CHARSET as u32, 0, 0, CLEARTYPE_QUALITY as u32, 0,
            wide("Segoe UI").as_ptr(),
        )
    }
}

fn create_hint(hinst: HINSTANCE, cfg: &Config) -> Hint {
    unsafe {
        let class = wide(HINT_CLASS);
        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(hint_proc),
            hInstance: hinst,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class.as_ptr(),
            std::ptr::null(),
            WS_POPUP,
            0, 0, 160, 40,
            std::ptr::null_mut(), std::ptr::null_mut(), hinst, std::ptr::null(),
        );
        SetLayeredWindowAttributes(hwnd, 0, cfg.alpha(), LWA_ALPHA);
        Hint {
            hwnd,
            title_font: make_font(10.5, 600),
            sub_font: make_font(9.0, 400),
            scale: system_scale(),
            visible: false,
            ctrl_mode: None,
            lang: String::new(),
            w: 160,
            h: 40,
            pos: (i32::MIN, i32::MIN),
            alpha: cfg.alpha(),
        }
    }
}

/// Title and second line of the hint bubble.
fn hint_text(lang: &str, ctrl: bool) -> (&'static str, &'static str) {
    let t = i18n::texts(lang);
    if ctrl { (t.hint_ctrl_title, t.hint_ctrl_sub) } else { (t.hint_title, t.hint_sub) }
}

// Bubble layout, in 96-dpi pixels: a dark card with a coloured badge on the left.
const PAD_X: f32 = 12.0;
const PAD_Y: f32 = 10.0;
const BADGE: f32 = 30.0;
const GAP: f32 = 10.0;
const RADIUS: f32 = 14.0;

fn text_size(dc: HDC, font: HFONT, text: &str) -> (i32, i32) {
    unsafe {
        let w = wide(text);
        let old = SelectObject(dc, font);
        let mut r: RECT = std::mem::zeroed();
        DrawTextW(dc, w.as_ptr(), -1, &mut r, DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX);
        SelectObject(dc, old);
        (r.right - r.left, r.bottom - r.top)
    }
}

fn show_hint(s: &mut State, pt: POINT) {
    let ctrl = s.ctrl_down;
    let hint = &mut s.hint;
    if hint.hwnd.is_null() {
        return;
    }
    unsafe {
        if hint.ctrl_mode != Some(ctrl) || hint.lang != s.cfg.language {
            hint.ctrl_mode = Some(ctrl);
            hint.lang = s.cfg.language.clone();
            let (title, sub) = hint_text(&hint.lang, ctrl);
            let dc = GetDC(hint.hwnd);
            let (tw, th) = text_size(dc, hint.title_font, title);
            let (sw, sh) = text_size(dc, hint.sub_font, sub);
            ReleaseDC(hint.hwnd, dc);
            let k = hint.scale;
            let px = |v: f32| (v * k).round() as i32;
            hint.w = px(PAD_X * 2.0 + BADGE + GAP + 4.0) + tw.max(sw);
            hint.h = px(PAD_Y * 2.0) + px(BADGE).max(th + sh + px(1.0));
            let corner = px(RADIUS) * 2;
            let rgn = CreateRoundRectRgn(0, 0, hint.w + 1, hint.h + 1, corner, corner);
            SetWindowPos(hint.hwnd, HWND_TOPMOST, 0, 0, hint.w, hint.h, SWP_NOMOVE | SWP_NOACTIVATE);
            SetWindowRgn(hint.hwnd, rgn, 1);
            hint.pos = (i32::MIN, i32::MIN);
            InvalidateRect(hint.hwnd, std::ptr::null(), 1);
        }
        let alpha = s.cfg.alpha();
        if alpha != hint.alpha {
            hint.alpha = alpha;
            SetLayeredWindowAttributes(hint.hwnd, 0, alpha, LWA_ALPHA);
        }

        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        let area = if GetMonitorInfoW(mon, &mut mi) != 0 {
            mi.rcMonitor
        } else {
            RECT { left: 0, top: 0, right: GetSystemMetrics(SM_CXSCREEN), bottom: GetSystemMetrics(SM_CYSCREEN) }
        };
        // Centred on the cursor, kept 8 px inside the screen, 16 px above the cursor.
        let mut x = pt.x - hint.w / 2;
        x = x.min(area.right - hint.w - 8).max(area.left + 8);
        let mut y = pt.y - hint.h - 16;
        if y < area.top + 8 {
            y = pt.y + 16;
        }
        if hint.pos != (x, y) || !hint.visible {
            hint.pos = (x, y);
            SetWindowPos(hint.hwnd, HWND_TOPMOST, x, y, hint.w, hint.h, SWP_NOACTIVATE);
        }
        if !hint.visible {
            ShowWindow(hint.hwnd, SW_SHOWNOACTIVATE);
            hint.visible = true;
        }
    }
}

fn hide_hint(hint: &mut Hint) {
    if hint.visible {
        unsafe { ShowWindow(hint.hwnd, SW_HIDE) };
        hint.visible = false;
    }
}

unsafe extern "system" fn hint_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let info = with(|s| {
                (s.hint.title_font, s.hint.sub_font, s.hint.scale, s.hint.ctrl_mode.unwrap_or(false), s.hint.lang.clone())
            });
            let mut ps: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut ps);
            if let Some((title_font, sub_font, k, ctrl, lang)) = info {
                let mut r: RECT = std::mem::zeroed();
                GetClientRect(hwnd, &mut r);
                let mem = CreateCompatibleDC(dc);
                let bmp = CreateCompatibleBitmap(dc, r.right, r.bottom);
                let old_bmp = SelectObject(mem, bmp);
                paint_hint(mem, &r, k, ctrl, &lang, title_font, sub_font);
                BitBlt(dc, 0, 0, r.right, r.bottom, mem, 0, 0, SRCCOPY);
                SelectObject(mem, old_bmp);
                DeleteObject(bmp);
                DeleteDC(mem);
            }
            EndPaint(hwnd, &ps);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Tell the running helper to read the settings again.
pub fn notify_reload() {
    unsafe {
        let w = FindWindowW(wide(MAIN_CLASS).as_ptr(), std::ptr::null());
        if !w.is_null() {
            PostMessageW(w, WM_APP_RELOAD, 0, 0);
        }
    }
}

/// Is the helper running?
pub fn is_running() -> bool {
    unsafe { !FindWindowW(wide(MAIN_CLASS).as_ptr(), std::ptr::null()).is_null() }
}

/// COLORREF from #RRGGBB.
const fn rgb(hex: u32) -> COLORREF {
    ((hex & 0xff) << 16) | (hex & 0xff00) | ((hex >> 16) & 0xff)
}

fn fill_round(dc: HDC, l: i32, t: i32, r: i32, b: i32, radius: i32, fill: COLORREF, border: Option<COLORREF>) {
    unsafe {
        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(PS_SOLID, 1, border.unwrap_or(fill));
        let ob = SelectObject(dc, brush);
        let op = SelectObject(dc, pen);
        RoundRect(dc, l, t, r, b, radius, radius);
        SelectObject(dc, ob);
        SelectObject(dc, op);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}

/// Draw the shared app icon into a square on the given HDC.
fn paint_icon(dc: HDC, x: i32, y: i32, size: i32) {
    let mut bgra = icon::rgba(size as u32);
    for p in bgra.chunks_exact_mut(4) {
        p.swap(0, 2);
    }
    unsafe {
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        info.bmiHeader.biWidth = size;
        info.bmiHeader.biHeight = -size; // top-down rows
        info.bmiHeader.biPlanes = 1;
        info.bmiHeader.biBitCount = 32;
        info.bmiHeader.biCompression = BI_RGB;

        let mem = CreateCompatibleDC(dc);
        let bmp = CreateCompatibleBitmap(dc, size, size);
        let old = SelectObject(mem, bmp);
        SetDIBits(
            mem,
            bmp,
            0,
            size as u32,
            bgra.as_ptr() as *const std::ffi::c_void,
            &info,
            DIB_RGB_COLORS,
        );
        BitBlt(dc, x, y, size, size, mem, 0, 0, SRCCOPY);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
    }
}

/// Dark card, coloured badge with a small drawing, a title and a second line.
fn paint_hint(dc: HDC, r: &RECT, k: f32, ctrl: bool, lang: &str, title_font: HFONT, sub_font: HFONT) {
    let px = |v: f32| (v * k).round() as i32;
    let card = rgb(0x1c1e24);
    // Blue for desktops, violet while Ctrl is held (apps).
    let accent = if ctrl { rgb(0x8b5cf6) } else { rgb(0x2f6feb) };
    let white = rgb(0xffffff);
    unsafe {
        let brush = CreateSolidBrush(card);
        FillRect(dc, r, brush);
        DeleteObject(brush);
        fill_round(dc, 0, 0, r.right, r.bottom, px(RADIUS) * 2, card, Some(rgb(0x3a3d46)));

        // badge: the shared app icon, or four tiles while Ctrl is held
        let bx = px(PAD_X);
        let bs = px(BADGE);
        let by = (r.bottom - bs) / 2;
        if ctrl {
            fill_round(dc, bx, by, bx + bs, by + bs, px(16.0), accent, None);
            let cell = px(6.0);
            let gap = px(3.0);
            let ox = bx + (bs - cell * 2 - gap) / 2;
            let oy = by + (bs - cell * 2 - gap) / 2;
            for (i, j) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let x = ox + i * (cell + gap);
                let y = oy + j * (cell + gap);
                fill_round(dc, x, y, x + cell, y + cell, px(3.0), white, None);
            }
        } else {
            paint_icon(dc, bx, by, bs);
        }

        // text
        let (title, sub) = hint_text(lang, ctrl);
        let tx = bx + bs + px(GAP);
        let (_, th) = text_size(dc, title_font, title);
        let (_, sh) = text_size(dc, sub_font, sub);
        let ty = (r.bottom - (th + px(1.0) + sh)) / 2;
        SetBkMode(dc, TRANSPARENT as i32);
        let draw = |font: HFONT, colour: COLORREF, text: &str, y: i32, h: i32| {
            let w = wide(text);
            let old = SelectObject(dc, font);
            SetTextColor(dc, colour);
            let mut tr = RECT { left: tx, top: y, right: r.right - px(PAD_X), bottom: y + h };
            DrawTextW(dc, w.as_ptr(), -1, &mut tr, DT_SINGLELINE | DT_NOPREFIX | DT_LEFT);
            SelectObject(dc, old);
        };
        draw(title_font, white, title, ty, th);
        draw(sub_font, rgb(0xaeb4c0), sub, ty + th + px(1.0), sh);
    }
}

/// Draw the hint bubble offscreen and save it as a PNG (`--hint-png <file> [ctrl] [fr]`),
/// to check its look without starting the helper.
pub fn render_hint_png(path: &std::path::Path, ctrl: bool, lang: &str) {
    unsafe {
        let title_font = make_font(10.5, 600);
        let sub_font = make_font(9.0, 400);
        let k = system_scale();
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let (title, sub) = hint_text(lang, ctrl);
        let (tw, th) = text_size(mem, title_font, title);
        let (sw, sh) = text_size(mem, sub_font, sub);
        let px = |v: f32| (v * k).round() as i32;
        let w = px(PAD_X * 2.0 + BADGE + GAP + 4.0) + tw.max(sw);
        let h = px(PAD_Y * 2.0) + px(BADGE).max(th + sh + px(1.0));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, bmp);
        let r = RECT { left: 0, top: 0, right: w, bottom: h };
        paint_hint(mem, &r, k, ctrl, lang, title_font, sub_font);
        let mut img = image::RgbaImage::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let c = GetPixel(mem, x, y);
                img.put_pixel(x as u32, y as u32, image::Rgba([(c & 0xff) as u8, ((c >> 8) & 0xff) as u8, ((c >> 16) & 0xff) as u8, 255]));
            }
        }
        let _ = img.save(path);
        SelectObject(mem, old);
        DeleteObject(bmp);
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
}
