//! Owned, bounded local-image fit/zoom window. No source writes or render timer.
use crate::{desktop, images::Images, theme::Palette};
use std::{cell::RefCell, path::PathBuf};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::{
            Direct2D::{Common::*, *},
            Dxgi::Common::*,
            Gdi::*,
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
    },
    core::{Result, w},
};
struct View {
    hwnd: HWND,
    factory: ID2D1Factory,
    target: Option<ID2D1HwndRenderTarget>,
    images: Images,
    directory: PathBuf,
    source: String,
    light: bool,
    zoom: f32,
    pan: (f32, f32),
    drag: Option<(i16, i16)>,
}
impl View {
    unsafe fn render(&mut self) -> Result<()> {
        unsafe {
            let mut rect = RECT::default();
            GetClientRect(self.hwnd, &mut rect)?;
            let width = (rect.right.max(1)) as f32;
            let height = (rect.bottom.max(1)) as f32;
            if self.target.is_none() {
                self.target = Some(self.factory.CreateHwndRenderTarget(
                    &D2D1_RENDER_TARGET_PROPERTIES {
                        pixelFormat: D2D1_PIXEL_FORMAT {
                            format: DXGI_FORMAT_UNKNOWN,
                            alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                        },
                        dpiX: 96.0,
                        dpiY: 96.0,
                        ..Default::default()
                    },
                    &D2D1_HWND_RENDER_TARGET_PROPERTIES {
                        hwnd: self.hwnd,
                        pixelSize: D2D_SIZE_U {
                            width: width as u32,
                            height: height as u32,
                        },
                        presentOptions: D2D1_PRESENT_OPTIONS_NONE,
                    },
                )?);
            }
            let target = self.target.as_ref().unwrap();
            let bitmap = self
                .images
                .load(target, &self.directory, &self.source)
                .ok_or_else(|| {
                    windows::core::Error::new(
                        E_FAIL,
                        "Unsupported/unavailable local image or image exceeds limits",
                    )
                })?;
            let size = bitmap.GetSize();
            let fit = (width / size.width).min(height / size.height);
            let scale = fit * self.zoom;
            let (w, h) = (size.width * scale, size.height * scale);
            self.pan.0 = self.pan.0.clamp(-w / 2.0, w / 2.0);
            self.pan.1 = self.pan.1.clamp(-h / 2.0, h / 2.0);
            let (x, y) = (
                (width - w) / 2.0 + self.pan.0,
                (height - h) / 2.0 + self.pan.1,
            );
            let rgb = Palette::new(self.light).background;
            target.BeginDraw();
            target.Clear(Some(&D2D1_COLOR_F {
                r: ((rgb >> 16) & 255) as f32 / 255.0,
                g: ((rgb >> 8) & 255) as f32 / 255.0,
                b: (rgb & 255) as f32 / 255.0,
                a: 1.0,
            }));
            target.DrawBitmap(
                &bitmap,
                Some(&D2D_RECT_F {
                    left: x,
                    top: y,
                    right: x + w,
                    bottom: y + h,
                }),
                1.0,
                D2D1_BITMAP_INTERPOLATION_MODE_LINEAR,
                None,
            );
            let result = target.EndDraw(None, None);
            if result.is_err() {
                self.images.clear();
                self.target = None;
            }
            result
        }
    }
}
unsafe extern "system" fn procedure(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut RefCell<View>;
        if pointer.is_null() {
            return DefWindowProcW(hwnd, message, w, l);
        }
        if message == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            drop(Box::from_raw(pointer));
            return DefWindowProcW(hwnd, message, w, l);
        }
        let Ok(mut view) = (*pointer).try_borrow_mut() else {
            return LRESULT(0);
        };
        match message {
            WM_ERASEBKGND => LRESULT(1),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut paint);
                if let Err(error) = view.render() {
                    eprintln!("Image view: {error}");
                }
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            WM_SIZE => {
                view.images.clear();
                view.target = None;
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let bounds = *(l.0 as *const RECT);
                view.images.clear();
                view.target = None;
                drop(view);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    bounds.left,
                    bounds.top,
                    bounds.right - bounds.left,
                    bounds.bottom - bounds.top,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
                LRESULT(0)
            }
            WM_MOUSEWHEEL => {
                let delta = ((w.0 >> 16) & 0xffff) as i16;
                view.zoom = (view.zoom * if delta > 0 { 1.2 } else { 1.0 / 1.2 }).clamp(0.25, 16.0);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                view.drag = Some(((l.0 & 0xffff) as i16, ((l.0 >> 16) & 0xffff) as i16));
                let _ = SetCapture(hwnd);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if let Some((old_x, old_y)) = view.drag {
                    let (x, y) = ((l.0 & 0xffff) as i16, ((l.0 >> 16) & 0xffff) as i16);
                    view.pan.0 += (i32::from(x) - i32::from(old_x)) as f32;
                    view.pan.1 += (i32::from(y) - i32::from(old_y)) as f32;
                    view.drag = Some((x, y));
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                view.drag = None;
                drop(view);
                let _ = ReleaseCapture();
                LRESULT(0)
            }
            WM_CAPTURECHANGED | WM_CANCELMODE => {
                view.drag = None;
                LRESULT(0)
            }
            WM_KEYDOWN => {
                if w.0 == 0x1b {
                    drop(view);
                    let _ = DestroyWindow(hwnd);
                    return LRESULT(0);
                }
                match w.0 {
                    0x24 | 0x30 | 0x20 => {
                        view.zoom = 1.0;
                        view.pan = (0.0, 0.0);
                    }
                    0x25 => view.pan.0 -= 32.0,
                    0x27 => view.pan.0 += 32.0,
                    0x26 => view.pan.1 -= 32.0,
                    0x28 => view.pan.1 += 32.0,
                    _ => {}
                }
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_CLOSE => {
                drop(view);
                let _ = DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => LRESULT(0),
            _ => {
                drop(view);
                DefWindowProcW(hwnd, message, w, l)
            }
        }
    }
}
pub fn is_view(hwnd: HWND) -> bool {
    unsafe {
        let mut name = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut name);
        n > 0 && String::from_utf16_lossy(&name[..n as usize]) == "RustyNote.Image"
    }
}
pub fn open(
    owner: HWND,
    factory: ID2D1Factory,
    directory: PathBuf,
    source: String,
    light: bool,
) -> Result<HWND> {
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(procedure),
            hInstance: HINSTANCE(instance.0),
            hCursor: LoadCursorW(None, IDC_SIZEALL)?,
            lpszClassName: w!("RustyNote.Image"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
            return Err(windows::core::Error::from_hresult(
                windows::core::HRESULT::from_win32(GetLastError().0),
            ));
        }
        let images = Images::new()?;
        let mut bounds = RECT::default();
        GetWindowRect(owner, &mut bounds)?;
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RustyNote.Image"),
            w!("Image — wheel: zoom · drag/arrows: pan · Home: fit · Esc: close"),
            WS_OVERLAPPEDWINDOW,
            bounds.left + 24,
            bounds.top + 24,
            720,
            560,
            Some(owner),
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )?;
        let state = Box::new(RefCell::new(View {
            hwnd,
            factory,
            target: None,
            images,
            directory,
            source,
            light,
            zoom: 1.0,
            pan: (0.0, 0.0),
            drag: None,
        }));
        let pointer = Box::into_raw(state);
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, pointer as isize);
        desktop::fit_window_to_work_area(hwnd);
        let result = {
            let mut view = (*pointer).borrow_mut();
            view.render()
        };
        if let Err(error) = result {
            let _ = DestroyWindow(hwnd);
            return Err(error);
        }
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        Ok(hwnd)
    }
}
