//! "Launch automatically with Windows": a value in the current user's Run key.
//! No administrator rights are needed, so it starts at logon without a prompt.

#[cfg(windows)]
mod imp {
    use crate::wide;
    use windows_sys::Win32::System::Registry::*;

    const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const VALUE: &str = "DesktopSwitcher";

    fn open(write: bool) -> Option<HKEY> {
        let mut key: HKEY = std::ptr::null_mut();
        let access = if write { KEY_SET_VALUE | KEY_QUERY_VALUE } else { KEY_QUERY_VALUE };
        let ok = unsafe {
            RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN_KEY).as_ptr(), 0, access, &mut key)
        };
        (ok == 0).then_some(key)
    }

    pub fn is_enabled() -> bool {
        let Some(key) = open(false) else { return false };
        let mut size = 0u32;
        let ok = unsafe {
            RegQueryValueExW(key, wide(VALUE).as_ptr(), std::ptr::null(), std::ptr::null_mut(), std::ptr::null_mut(), &mut size)
        };
        unsafe { RegCloseKey(key) };
        ok == 0
    }

    pub fn set(enabled: bool) -> bool {
        let Some(key) = open(true) else { return false };
        let ok = if enabled {
            let exe = std::env::current_exe().unwrap_or_default();
            let cmd = wide(&format!("\"{}\"", exe.display()));
            unsafe {
                RegSetValueExW(key, wide(VALUE).as_ptr(), 0, REG_SZ, cmd.as_ptr() as *const u8, (cmd.len() * 2) as u32)
            }
        } else {
            let r = unsafe { RegDeleteValueW(key, wide(VALUE).as_ptr()) };
            if r == 2 { 0 } else { r } // already absent is fine
        };
        unsafe { RegCloseKey(key) };
        ok == 0
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn is_enabled() -> bool {
        false
    }
    pub fn set(_enabled: bool) -> bool {
        false
    }
}

pub use imp::*;
