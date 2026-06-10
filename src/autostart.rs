use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::WIN32_ERROR;
use windows::Win32::System::Registry::*;

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const VALUE_NAME: PCWSTR = w!("Clam");

pub fn is_autostart_enabled() -> bool {
    unsafe {
        let mut hkey = HKEY::default();
        let err = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, Some(0), KEY_READ, &mut hkey);
        if err != WIN32_ERROR(0) {
            return false;
        }
        let exists = RegQueryValueExW(hkey, VALUE_NAME, None, None, None, None) == WIN32_ERROR(0);
        let _ = RegCloseKey(hkey);
        exists
    }
}

pub fn set_autostart(enabled: bool) {
    unsafe {
        let mut hkey = HKEY::default();
        let err = RegOpenKeyExW(HKEY_CURRENT_USER, RUN_KEY, Some(0), KEY_SET_VALUE, &mut hkey);
        if err != WIN32_ERROR(0) {
            return;
        }

        if enabled {
            let path = std::env::current_exe()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
            let bytes: &[u8] = std::slice::from_raw_parts(
                wide.as_ptr() as *const u8,
                wide.len() * 2,
            );
            let _ = RegSetValueExW(hkey, VALUE_NAME, None, REG_SZ, Some(bytes));
        } else {
            let _ = RegDeleteValueW(hkey, VALUE_NAME);
        }

        let _ = RegCloseKey(hkey);
    }
}
