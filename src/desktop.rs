//! Native shell integration. No background polling/render timers.
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::*,
        Storage::FileSystem::*,
        System::Com::CoTaskMemFree,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, Result, w},
};
pub const TRAY_MESSAGE: u32 = 0x8005;
pub struct Tray {
    data: NOTIFYICONDATAW,
}
impl Tray {
    pub fn new(hwnd: HWND) -> Result<Self> {
        unsafe {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                uCallbackMessage: TRAY_MESSAGE,
                hIcon: LoadIconW(None, IDI_APPLICATION)?,
                ..Default::default()
            };
            for (dst, src) in data
                .szTip
                .iter_mut()
                .zip("Rusty Note — double-click to restore".encode_utf16())
            {
                *dst = src;
            }
            if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                return Err(windows::core::Error::new(
                    E_FAIL,
                    "Could not register tray icon",
                ));
            }
            Ok(Self { data })
        }
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
        }
    }
}
pub struct Watch {
    handle: HANDLE,
}
impl Watch {
    pub fn new(path: &Path) -> Result<Self> {
        let wide: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain([0])
            .collect();
        unsafe {
            Ok(Self {
                handle: FindFirstChangeNotificationW(
                    PCWSTR(wide.as_ptr()),
                    false,
                    FILE_NOTIFY_CHANGE_FILE_NAME
                        | FILE_NOTIFY_CHANGE_LAST_WRITE
                        | FILE_NOTIFY_CHANGE_SIZE,
                )?,
            })
        }
    }
    pub fn handle(&self) -> HANDLE {
        self.handle
    }
    pub fn rearm(&self) {
        unsafe {
            let _ = FindNextChangeNotification(self.handle);
        }
    }
}
impl Drop for Watch {
    fn drop(&mut self) {
        unsafe {
            let _ = FindCloseChangeNotification(self.handle);
        }
    }
}
pub fn open_folder(hwnd: HWND, path: &Path) {
    let wide: Vec<u16> = path
        .as_os_str()
        .to_string_lossy()
        .encode_utf16()
        .chain([0])
        .collect();
    unsafe {
        ShellExecuteW(
            Some(hwnd),
            w!("open"),
            PCWSTR(wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}
pub fn open_link(hwnd: HWND, url: &str) {
    if !safe_link(url) {
        return;
    }
    let wide: Vec<u16> = url.encode_utf16().chain([0]).collect();
    unsafe {
        ShellExecuteW(
            Some(hwnd),
            w!("open"),
            PCWSTR(wide.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
}
pub fn safe_link(url: &str) -> bool {
    let value = url.to_ascii_lowercase();
    !value.contains(['\0', '\r', '\n'])
        && (value.starts_with("https://")
            || value.starts_with("http://")
            || value.starts_with("mailto:"))
}
pub fn choose_folder(hwnd: HWND) -> Option<PathBuf> {
    unsafe {
        let browse = BROWSEINFOW {
            hwndOwner: hwnd,
            lpszTitle: w!("Choose your Markdown notes folder"),
            ulFlags: BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE,
            ..Default::default()
        };
        let id = SHBrowseForFolderW(&browse);
        if id.is_null() {
            return None;
        }
        let mut buffer = [0u16; 260];
        let valid = SHGetPathFromIDListW(id, &mut buffer).as_bool();
        CoTaskMemFree(Some(id as *const _));
        valid.then(|| {
            PathBuf::from(String::from_utf16_lossy(
                &buffer[..buffer.iter().position(|c| *c == 0).unwrap_or(260)],
            ))
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explicit_web_and_mail_schemes_are_opened() {
        assert!(safe_link("https://example.com"));
        assert!(safe_link("mailto:hello@example.com"));
        for url in [
            "javascript:alert(1)",
            "file:///C:/private",
            "shell:startup",
            "https://example.com\nfile:bad",
        ] {
            assert!(!safe_link(url));
        }
    }
}
