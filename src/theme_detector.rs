use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
use cocoa::foundation::{NSAutoreleasePool, NSString};
#[cfg(target_os = "macos")]
use cocoa::base::{id, nil};
#[cfg(target_os = "macos")]
use objc::{msg_send, sel, sel_impl, class};

#[cfg(target_os = "windows")]
use winapi::um::winreg::*;
#[cfg(target_os = "windows")]
use winapi::um::winnt::KEY_READ;
#[cfg(target_os = "windows")]
use winapi::shared::winerror::ERROR_SUCCESS;
#[cfg(target_os = "windows")]
use winapi::shared::minwindef::DWORD;

#[cfg(target_os = "windows")]
fn to_wide_str(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

pub struct ThemeDetector {
    is_dark: Arc<AtomicBool>,
}

impl ThemeDetector {
    pub fn new() -> Self {
        let is_dark = Self::detect_system_theme();
        Self {
            is_dark: Arc::new(AtomicBool::new(is_dark)),
        }
    }

    #[cfg(target_os = "macos")]
    pub fn detect_system_theme() -> bool {
        unsafe {
            let _pool = NSAutoreleasePool::new(nil);

            let app: id = msg_send![class!(NSApplication), sharedApplication];
            
            if app == nil {
                return false;
            }

            let appearance: id = msg_send![app, effectiveAppearance];
            
            if appearance == nil {
                return false;
            }

            let dark_aqua = NSString::alloc(nil).init_str("NSAppearanceNameDarkAqua");
            let aqua = NSString::alloc(nil).init_str("NSAppearanceNameAqua");
            
            let best_match: id = msg_send![appearance, 
                bestMatchFromAppearancesWithNames:vec![dark_aqua, aqua]
            ];
            
            if best_match == nil {
                return false;
            }

            let name: id = msg_send![best_match, UTF8String];
            
            if name == nil {
                return false;
            }

            let name_str = std::ffi::CStr::from_ptr(name as *const i8)
                .to_string_lossy()
                .into_owned();

            name_str.contains("dark")
        }
    }

    #[cfg(target_os = "windows")]
    pub fn detect_system_theme() -> bool {
        unsafe {
            let mut hkey = std::ptr::null_mut();
            let subkey = to_wide_str("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");
            
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                subkey.as_ptr(),
                0,
                KEY_READ,
                &mut hkey
            ) != ERROR_SUCCESS as i32 {
                return false;
            }
            
            let mut value: DWORD = 0;
            let mut value_size = std::mem::size_of::<DWORD>() as DWORD;
            
            let apps_use_light_theme = to_wide_str("AppsUseLightTheme");
            let result = RegQueryValueExW(
                hkey,
                apps_use_light_theme.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::mem::transmute(&mut value),
                &mut value_size
            );
            
            RegCloseKey(hkey);
            
            if result != ERROR_SUCCESS as i32 {
                return false;
            }
            
            // 如果 AppsUseLightTheme 为 0，则为深色模式
            value == 0
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    pub fn detect_system_theme() -> bool {
        false
    }

    pub fn is_dark_mode(&self) -> bool {
        // 每次调用都重新检测主题
        Self::detect_system_theme()
    }
}

impl Default for ThemeDetector {
    fn default() -> Self {
        Self::new()
    }
}
