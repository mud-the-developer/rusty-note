//! Small native text prompt for note names. No process/shell execution.
use windows::{
    Win32::{
        Foundation::*,
        UI::{
            Input::KeyboardAndMouse::{EnableWindow, SetFocus},
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};
struct State {
    edit: HWND,
    result: Option<String>,
    done: bool,
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
        if ptr.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        match msg {
            WM_COMMAND => {
                match w.0 & 0xffff {
                    1 => {
                        let state = &mut *ptr;
                        let mut text =
                            vec![0u16; GetWindowTextLengthW(state.edit).max(0) as usize + 1];
                        let count = GetWindowTextW(state.edit, &mut text).max(0) as usize;
                        state.result = Some(String::from_utf16_lossy(&text[..count]));
                        state.done = true;
                    }
                    2 => (*ptr).done = true,
                    _ => {}
                }
                LRESULT(0)
            }
            WM_CLOSE => {
                (*ptr).done = true;
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}
pub fn ask(parent: HWND, title: &str, value: &str) -> Option<String> {
    unsafe {
        let class = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hCursor: LoadCursorW(None, IDC_ARROW).ok()?,
            hbrBackground: windows::Win32::Graphics::Gdi::HBRUSH(6usize as *mut _),
            lpszClassName: w!("RustyNote.Prompt"),
            ..Default::default()
        };
        let _ = RegisterClassW(&class);
        let mut bounds = RECT::default();
        let _ = GetWindowRect(parent, &mut bounds);
        let caption: Vec<u16> = title.encode_utf16().chain([0]).collect();
        let text: Vec<u16> = value.encode_utf16().chain([0]).collect();
        let hwnd = CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            w!("RustyNote.Prompt"),
            PCWSTR(caption.as_ptr()),
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            bounds.left + 20,
            bounds.top + 90,
            330,
            145,
            Some(parent),
            None,
            None,
            None,
        )
        .ok()?;
        let edit = CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
            15,
            15,
            290,
            28,
            Some(hwnd),
            Some(HMENU(10usize as *mut _)),
            None,
            None,
        )
        .ok()?;
        for (id, label, x) in [(1, w!("OK"), 150), (2, w!("Cancel"), 230)] {
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("BUTTON"),
                label,
                WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                x,
                65,
                75,
                28,
                Some(hwnd),
                Some(HMENU(id as *mut _)),
                None,
                None,
            );
        }
        let mut state = Box::new(State {
            edit,
            result: None,
            done: false,
        });
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *state as *mut State) as isize);
        let _ = EnableWindow(parent, false);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetFocus(Some(edit));
        SendMessageW(edit, 0x00b1, Some(WPARAM(0)), Some(LPARAM(-1)));
        let mut msg = MSG::default();
        loop {
            if state.done {
                break;
            }
            let result = GetMessageW(&mut msg, None, 0, 0);
            if result.0 <= 0 {
                if result.0 == 0 {
                    PostQuitMessage(msg.wParam.0 as i32);
                }
                break;
            }
            if msg.message == WM_KEYDOWN && (msg.wParam.0 == 0x0d || msg.wParam.0 == 0x1b) {
                SendMessageW(
                    hwnd,
                    WM_COMMAND,
                    Some(WPARAM(if msg.wParam.0 == 0x0d { 1 } else { 2 })),
                    None,
                );
                continue;
            }
            if !IsDialogMessageW(hwnd, &msg).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
        let _ = DestroyWindow(hwnd);
        let _ = EnableWindow(parent, true);
        let _ = SetForegroundWindow(parent);
        state.result.take()
    }
}
