//! The reminder input form: a plain Win32 window holding the reminder text and the time it is
//! due, as two native edit controls.
//!
//! Native controls, the user's own UI font and per-monitor DPI are what make this feel like a
//! Windows application rather than a drawn imitation.

use std::io;
use std::mem;
use std::ptr;
use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    DeleteObject, GetMonitorInfoW, HFONT, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetFocus, SetFocus, VK_CONTROL, VK_ESCAPE, VK_RETURN, VK_TAB,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    ES_AUTOHSCROLL, ES_AUTOVSCROLL, ES_MULTILINE, ES_WANTRETURN, GetClientRect, GetCursorPos,
    GetDlgItem, GetMessageW, GetWindowTextLengthW, GetWindowTextW, IDC_ARROW, LoadCursorW,
    MB_ICONERROR, MSG, MoveWindow, PostQuitMessage, RegisterClassW, SW_SHOW, SendMessageW,
    SetForegroundWindow, SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, WM_DESTROY,
    WM_KEYDOWN, WM_SETFONT, WM_SIZE, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_OVERLAPPED, WS_SYSMENU,
    WS_VISIBLE, WS_VSCROLL,
};

use crate::reminder;
use crate::ui::{scale, ui_font, wide};

const CLASS_NAME: &str = "ReminderskiForm";
const WINDOW_TITLE: &str = "Reminderski";

/// Child window identifiers, so the two edit controls can be found again from the window
/// procedure without stashing pointers on the window.
const ID_TEXT: i32 = 1;
const ID_WHEN: i32 = 2;

/// Logical pixels at 96 DPI; scaled to the monitor the form opens on.
const WINDOW_WIDTH: i32 = 560;
const WINDOW_HEIGHT: i32 = 240;
const MARGIN: i32 = 8;
const GAP: i32 = 8;
const WHEN_HEIGHT: i32 = 24;

/// The standard window background, so the form follows the system theme.
const COLOR_WINDOW_BRUSH: usize = 6; // COLOR_WINDOW + 1

/// What the user accepted: the reminder itself, and how long from now it is due.
pub struct Entry {
    pub text: String,
    pub delay: Duration,
}

struct Form {
    window: HWND,
    text: HWND,
    when: HWND,
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
/// Returns the entry on Enter, or `None` on Escape or closing the window.
pub fn show(initial: &str) -> io::Result<Option<Entry>> {
    let form = Form::create()?;
    unsafe { SetWindowTextW(form.text, wide(initial).as_ptr()) };

    // With a capture in hand the only thing left to type is the time, so start there. Without
    // one the reminder still has to be written.
    let focus = if initial.is_empty() {
        form.text
    } else {
        form.when
    };

    // A hotkey press grants this process the right to take the foreground, so this succeeds even
    // though the user was working in another application.
    unsafe {
        ShowWindow(form.window, SW_SHOW);
        SetForegroundWindow(form.window);
        SetFocus(focus);
    }

    Ok(run_modal_loop(&form))
}

/// Pumps messages until the form closes, returning the entry if the user accepted.
///
/// The keys are handled here rather than in the window procedure because the edit controls
/// consume them themselves, and intercepting them before dispatch avoids subclassing both
/// controls for three keystrokes.
fn run_modal_loop(form: &Form) -> Option<Entry> {
    let mut entered = None;
    let mut message: MSG = unsafe { mem::zeroed() };
    loop {
        if unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } <= 0 {
            return entered;
        }
        if message.message == WM_KEYDOWN {
            match message.wParam as u16 {
                VK_ESCAPE => {
                    unsafe { DestroyWindow(form.window) };
                    continue;
                }
                VK_TAB => {
                    let target = if focused(form.when) {
                        form.text
                    } else {
                        form.when
                    };
                    unsafe { SetFocus(target) };
                    continue;
                }
                // Enter accepts from the time field, where it has nothing else to do.
                // Ctrl+Enter accepts from either, so the reminder text never traps the user.
                VK_RETURN if focused(form.when) || control_is_down() => {
                    match read_entry(form) {
                        // Read before destroying: the handles are worthless afterwards.
                        Some(entry) => {
                            entered = Some(entry);
                            unsafe { DestroyWindow(form.window) };
                        }
                        None => unsafe {
                            MessageBeep(MB_ICONERROR);
                            SetFocus(form.when);
                        },
                    }
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

/// The entry, or `None` when there is nothing to remind about or the time cannot be read.
fn read_entry(form: &Form) -> Option<Entry> {
    let text = read_text(form.text);
    if text.trim().is_empty() {
        return None;
    }
    let delay = reminder::parse_when(&read_text(form.when))?;
    Some(Entry { text, delay })
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

        // The edit styles are declared as i32 while the window styles are u32.
        let text = create_edit(
            window,
            ID_TEXT,
            WS_VSCROLL | (ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32,
        );
        let when = create_edit(window, ID_WHEN, ES_AUTOHSCROLL as u32);
        if text.is_null() || when.is_null() {
            let error = io::Error::last_os_error();
            unsafe { DestroyWindow(window) };
            return Err(error);
        }

        // Only now that there is a window is its monitor, and therefore its DPI, known.
        let dpi = unsafe { GetDpiForWindow(window) };
        let font = ui_font(dpi);
        if !font.is_null() {
            unsafe {
                SendMessageW(text, WM_SETFONT, font as WPARAM, 1);
                SendMessageW(when, WM_SETFONT, font as WPARAM, 1);
            }
        }
        center_on_active_monitor(window, dpi);

        Ok(Self {
            window,
            text,
            when,
            font,
        })
    }
}

fn create_edit(parent: HWND, id: i32, styles: u32) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            ptr::null(),
            WS_CHILD | WS_VISIBLE | styles,
            0,
            0,
            0,
            0,
            parent,
            ptr::without_provenance_mut(id as usize),
            ptr::null_mut(),
            ptr::null(),
        )
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
            unsafe { layout(window) };
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

/// Stacks the reminder text above the one-line time field, both filling the window's width.
unsafe fn layout(window: HWND) {
    let text = unsafe { GetDlgItem(window, ID_TEXT) };
    let when = unsafe { GetDlgItem(window, ID_WHEN) };
    if text.is_null() || when.is_null() {
        return;
    }

    let dpi = unsafe { GetDpiForWindow(window) };
    let margin = scale(MARGIN, dpi);
    let gap = scale(GAP, dpi);
    let when_height = scale(WHEN_HEIGHT, dpi);

    let mut client = RECT::default();
    unsafe { GetClientRect(window, &mut client) };
    let width = client.right - margin * 2;
    let text_height = client.bottom - margin * 2 - gap - when_height;
    if width <= 0 || text_height <= 0 {
        return;
    }

    unsafe {
        MoveWindow(text, margin, margin, width, text_height, 1);
        MoveWindow(
            when,
            margin,
            margin + text_height + gap,
            width,
            when_height,
            1,
        );
    }
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

fn focused(control: HWND) -> bool {
    unsafe { GetFocus() == control }
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
