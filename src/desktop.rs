//! Native shell integration. No background polling/render timers.
use std::path::{Path, PathBuf};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        Storage::FileSystem::*,
        System::{Com::CoTaskMemFree, LibraryLoader::GetModuleHandleW},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, Result, w},
};
pub const TRAY_MESSAGE: u32 = 0x8005;

pub fn app_icon(small: bool) -> Result<HICON> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        // Shared module resources live for the process; callers must not destroy them.
        let icon = LoadImageW(
            Some(HINSTANCE(instance.0)),
            PCWSTR(std::ptr::without_provenance(1)),
            IMAGE_ICON,
            GetSystemMetrics(if small { SM_CXSMICON } else { SM_CXICON }),
            GetSystemMetrics(if small { SM_CYSMICON } else { SM_CYICON }),
            LR_SHARED,
        )?;
        Ok(HICON(icon.0))
    }
}

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
                hIcon: app_icon(true)?,
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
/// Diagnostic payload creation in the receiving process: HGLOBAL is process-local.
/// Used only with explicit native-validation options, never to inject another process.
pub fn diagnostic_drop(files: &[PathBuf]) -> std::io::Result<HDROP> {
    use windows::Win32::System::Memory::*;
    let mut text = Vec::<u16>::new();
    for path in files {
        text.extend(path.to_string_lossy().encode_utf16());
        text.push(0);
    }
    text.push(0);
    if text.len() > 32768 {
        return Err(std::io::Error::other("Drop payload exceeds test limit"));
    }
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, 20 + text.len() * 2)
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let pointer = GlobalLock(handle);
        if pointer.is_null() {
            let _ = GlobalFree(Some(handle));
            return Err(std::io::Error::other("Could not lock diagnostic drop"));
        }
        let pointer = pointer as *mut u8;
        std::ptr::write_unaligned(pointer as *mut u32, 20);
        std::ptr::write_unaligned(pointer.add(16) as *mut u32, 1);
        std::ptr::copy_nonoverlapping(text.as_ptr() as *const u8, pointer.add(20), text.len() * 2);
        let _ = GlobalUnlock(handle);
        Ok(HDROP(handle.0))
    }
}
/// Consume exactly one Unicode file drop, always releasing the shell allocation.
pub fn dropped_markdown(drop: HDROP) -> std::io::Result<PathBuf> {
    unsafe {
        let count = DragQueryFileW(drop, u32::MAX, None);
        let result = if count != 1 {
            Err(std::io::Error::other(
                "Drop one Markdown (.md) file at a time",
            ))
        } else {
            let length = DragQueryFileW(drop, 0, None);
            if length == 0 || length > 32767 {
                Err(std::io::Error::other(
                    "Dropped filename is unavailable or too long",
                ))
            } else {
                let mut text = vec![0u16; length as usize + 1];
                let copied = DragQueryFileW(drop, 0, Some(&mut text));
                String::from_utf16(&text[..copied as usize])
                    .map(PathBuf::from)
                    .map_err(|_| std::io::Error::other("Dropped filename is not valid Unicode"))
            }
        };
        DragFinish(drop);
        result
    }
}

pub fn fit_window_to_work_area(hwnd: HWND) {
    unsafe {
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return;
        }
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }
        let fitted = crate::placement::Placement {
            x: r.left,
            y: r.top,
            width: r.right - r.left,
            height: r.bottom - r.top,
        }
        .fit(
            info.rcWork.left,
            info.rcWork.top,
            info.rcWork.right,
            info.rcWork.bottom,
        );
        if fitted.x != r.left
            || fitted.y != r.top
            || fitted.width != r.right - r.left
            || fitted.height != r.bottom - r.top
        {
            let _ = SetWindowPos(
                hwnd,
                None,
                fitted.x,
                fitted.y,
                fitted.width,
                fitted.height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
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
pub fn choose_markdown(hwnd: HWND) -> Option<PathBuf> {
    use windows::Win32::UI::Controls::Dialogs::*;
    unsafe {
        let mut buffer = vec![0u16; 32768];
        let filter: Vec<u16> = "Markdown (*.md)\0*.md\0\0".encode_utf16().collect();
        let mut dialog = OPENFILENAMEW {
            lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: hwnd,
            lpstrFilter: PCWSTR(filter.as_ptr()),
            lpstrFile: windows::core::PWSTR(buffer.as_mut_ptr()),
            nMaxFile: buffer.len() as u32,
            lpstrTitle: w!("Open Markdown sticker"),
            Flags: OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
            ..Default::default()
        };
        if !GetOpenFileNameW(&mut dialog).as_bool() {
            return None;
        }
        let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
        Some(PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
    }
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
    fn shell_drop_decodes_unicode_and_rejects_multiple_files() {
        let path = PathBuf::from("C:/fixture/한글 스티커.md");
        let payload = diagnostic_drop(std::slice::from_ref(&path)).unwrap();
        assert_eq!(dropped_markdown(payload).unwrap(), path);
        let payload = diagnostic_drop(&[path.clone(), path]).unwrap();
        assert!(dropped_markdown(payload).is_err());
    }
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
