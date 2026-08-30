//! The reminder input form: a plain Win32 window holding one multiline edit control.
//!
//! Native controls, the user's own UI font and per-monitor DPI are what make this feel like a
//! Windows application rather than a drawn imitation.

use std::io;
use std::iter::once;
use std::mem;
use std::ptr;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetMonitorInfoW, HFONT, MONITOR_DEFAULTTONEAREST,
    MONITORINFO, MonitorFromPoint,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, SystemParametersInfoForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SetFocus, VK_CONTROL, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    ES_AUTOVSCROLL, ES_MULTILINE, ES_WANTRETURN, GW_CHILD, GetClientRect, GetCursorPos,
    GetMessageW, GetWindow, GetWindowTextLengthW, GetWindowTextW, IDC_ARROW, LoadCursorW, MSG,
    MoveWindow, NONCLIENTMETRICSW, PostQuitMessage, RegisterClassW, SPI_GETNONCLIENTMETRICS,
    SW_SHOW, SendMessageW, SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow,
    TranslateMessage, WM_DESTROY, WM_KEYDOWN, WM_SETFONT, WM_SIZE, WNDCLASSW, WS_CAPTION, WS_CHILD,
    WS_OVERLAPPED, WS_SYSMENU, WS_VISIBLE, WS_VSCROLL,
};

const CLASS_NAME: &str = "ReminderskiForm";
const WINDOW_TITLE: &str = "Reminderski";

/// Logical pixels at 96 DPI; scaled to the monitor the form opens on.
const WINDOW_WIDTH: i32 = 560;
const WINDOW_HEIGHT: i32 = 220;
const MARGIN: i32 = 8;

/// The standard window background, so the form follows the system theme.
const COLOR_WINDOW_BRUSH: usize = 6; // COLOR_WINDOW + 1

struct Form {
    window: HWND,
    edit: HWND,
    font: HFONT,
}

impl Drop for Form {
    fn drop(&mut self) {
        if !self.font.is_null() {
            unsafe { DeleteObject(self.font as _) };
        }
    }
}

/// Shows the form pre-filled with `initial`, and blocks until the user accepts or cancels.
///
/// Returns the entered text on Ctrl+Enter, or `None` on Escape or closing the window.
pub fn show(initial: &str) -> io::Result<Option<String>> {
    let form = Form::create()?;
    unsafe { SetWindowTextW(form.edit, wide(initial).as_ptr()) };

    // A hotkey press grants this process the right to take the foreground, so this succeeds even
    // though the user was working in another application.
    unsafe {
        ShowWindow(form.window, SW_SHOW);
        SetForegroundWindow(form.window);
        SetFocus(form.edit);
    }

    Ok(run_modal_loop(form.window, form.edit))
}

/// Pumps messages until the form closes, returning the entered text if the user accepted.
///
/// Escape and Ctrl+Enter are handled here rather than in the window procedure because the edit
/// control consumes those keys itself, and intercepting them before dispatch avoids subclassing
/// it for two keystrokes.
fn run_modal_loop(window: HWND, edit: HWND) -> Option<String> {
    let mut entered = None;
    let mut message: MSG = unsafe { mem::zeroed() };
    loop {
        if unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } <= 0 {
            return entered;
        }
        if message.message == WM_KEYDOWN {
            match message.wParam as u16 {
                VK_ESCAPE => {
                    unsafe { DestroyWindow(window) };
                    continue;
                }
                VK_RETURN if control_is_down() => {
                    // Read before destroying: the handle is worthless afterwards.
                    entered = Some(read_text(edit));
                    unsafe { DestroyWindow(window) };
                    continue;
                }
                _ => {}
            }
        }
        // The capture hotkey stays registered while the form is open. WM_HOTKEY arrives here as
        // a thread message with no target window, so dispatching it does nothing, which is what
        // stops a second form from opening on top of this one.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

impl Form {
    fn create() -> io::Result<Self> {
        register_class()?;

        let window = unsafe {
            CreateWindowExW(
                0,
                wide(CLASS_NAME).as_ptr(),
                wide(WINDOW_TITLE).as_ptr(),
                WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }

        let edit = unsafe {
            CreateWindowExW(
                0,
                wide("EDIT").as_ptr(),
                ptr::null(),
                // The edit styles are declared as i32 while the window styles are u32.
                WS_CHILD
                    | WS_VISIBLE
                    | WS_VSCROLL
                    | (ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32,
                0,
                0,
                0,
                0,
                window,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if edit.is_null() {
            let error = io::Error::last_os_error();
            unsafe { DestroyWindow(window) };
            return Err(error);
        }

        // Only now that there is a window is its monitor, and therefore its DPI, known.
        let dpi = unsafe { GetDpiForWindow(window) };
        let font = create_ui_font(dpi);
        if !font.is_null() {
            unsafe { SendMessageW(edit, WM_SETFONT, font as WPARAM, 1) };
        }
        center_on_active_monitor(window, dpi);

        Ok(Self { window, edit, font })
    }
}

fn register_class() -> io::Result<()> {
    static REGISTERED: OnceLock<Result<(), i32>> = OnceLock::new();

    let outcome = REGISTERED.get_or_init(|| {
        // Bound to a local so the buffer outlives the RegisterClassW call below.
        let class_name = wide(CLASS_NAME);
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: unsafe { GetModuleHandleW(ptr::null()) },
            hIcon: ptr::null_mut(),
            hCursor: unsafe { LoadCursorW(ptr::null_mut(), IDC_ARROW) },
            hbrBackground: COLOR_WINDOW_BRUSH as _,
            lpszMenuName: ptr::null(),
            lpszClassName: class_name.as_ptr(),
        };
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(io::Error::last_os_error().raw_os_error().unwrap_or(0));
        }
        Ok(())
    });

    outcome.map_err(io::Error::from_raw_os_error)
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_SIZE => {
            // The edit control is the window's only child, so there is nothing to disambiguate.
            let edit = unsafe { GetWindow(window, GW_CHILD) };
            if !edit.is_null() {
                let margin = scale(MARGIN, unsafe { GetDpiForWindow(window) });
                let mut client = RECT::default();
                unsafe {
                    GetClientRect(window, &mut client);
                    MoveWindow(
                        edit,
                        margin,
                        margin,
                        client.right - margin * 2,
                        client.bottom - margin * 2,
                        1,
                    );
                }
            }
            0
        }
        WM_DESTROY => {
            // Ends run_modal_loop. The resulting WM_QUIT is consumed by that loop's GetMessageW,
            // so the application's main loop never sees it.
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Creates the font Windows uses for UI text, at the given DPI. Null when it cannot be read,
/// in which case the control keeps the default font rather than failing the capture.
fn create_ui_font(dpi: u32) -> HFONT {
    let mut metrics = NONCLIENTMETRICSW {
        cbSize: mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..unsafe { mem::zeroed() }
    };
    let read = unsafe {
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS,
            metrics.cbSize,
            ptr::from_mut(&mut metrics).cast(),
            0,
            dpi,
        )
    };
    if read == 0 {
        return ptr::null_mut();
    }
    unsafe { CreateFontIndirectW(&metrics.lfMessageFont) }
}

/// Centres the form on whichever monitor the mouse is on, which is where the user is working.
fn center_on_active_monitor(window: HWND, dpi: u32) {
    let width = scale(WINDOW_WIDTH, dpi);
    let height = scale(WINDOW_HEIGHT, dpi);

    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) };
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };

    let mut info = MONITORINFO {
        cbSize: mem::size_of::<MONITORINFO>() as u32,
        ..unsafe { mem::zeroed() }
    };
    let work = if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
        info.rcWork
    } else {
        RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        }
    };

    let left = work.left + (work.right - work.left - width) / 2;
    let top = work.top + (work.bottom - work.top - height) / 2;
    unsafe { SetWindowPos(window, ptr::null_mut(), left, top, width, height, 0) };
}

fn scale(logical: i32, dpi: u32) -> i32 {
    logical * dpi as i32 / 96
}

fn control_is_down() -> bool {
    unsafe { GetAsyncKeyState(VK_CONTROL as i32) as u16 & 0x8000 != 0 }
}

fn read_text(edit: HWND) -> String {
    let length = unsafe { GetWindowTextLengthW(edit) };
    if length <= 0 {
        return String::new();
    }
    let mut buffer = vec![0u16; length as usize + 1];
    let copied = unsafe { GetWindowTextW(edit, buffer.as_mut_ptr(), buffer.len() as i32) };
    String::from_utf16_lossy(&buffer[..copied as usize])
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}
