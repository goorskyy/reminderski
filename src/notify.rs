//! The notification shown when a reminder falls due.
//!
//! A small window in the corner of the work area rather than a Windows toast: the persona is
//! ASCII art that needs a fixed-pitch font, and a toast from a plain Win32 executable would mean
//! WinRT bindings and a registered Start menu shortcut for a window we can draw ourselves.
//!
//! The window is modeless. A notification that blocked the message loop would stop the capture
//! shortcut working for as long as it went unanswered, which is exactly how long a reminder that
//! arrives at a bad moment tends to sit there.

use std::cell::Cell;
use std::io;
use std::ptr;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    DeleteObject, GetMonitorInfoW, HFONT, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BS_DEFPUSHBUTTON, CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect,
    GetCursorPos, GetWindowLongPtrW, IDC_ARROW, IDCANCEL, IDOK, IsWindow, LoadCursorW,
    MB_ICONASTERISK, RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WM_CLOSE, WM_COMMAND, WM_SETFONT, WNDCLASSW,
    WS_BORDER, WS_CHILD, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP, WS_TABSTOP, WS_VISIBLE,
};

use crate::ui::{monospace_font, scale, ui_font, wide};

const CLASS_NAME: &str = "ReminderskiNotification";

/// Not 1 or 2, which the dialog handling reserves for OK and Cancel.
const ID_DONE: i32 = 100;
const ID_SNOOZE: i32 = 101;

/// Logical pixels at 96 DPI.
const WIDTH: i32 = 360;
const HEIGHT: i32 = 160;
const MARGIN: i32 = 12;
const GAP: i32 = 8;
const PERSONA_WIDTH: i32 = 76;
const BUTTON_WIDTH: i32 = 96;
const BUTTON_HEIGHT: i32 = 26;

/// The dialog face colour, which is what the buttons and labels paint themselves against.
const COLOR_DIALOG_BRUSH: usize = 16; // COLOR_BTNFACE + 1

const PERSONA: &str = " ,---.\r\n( o.o )\r\n > ^ <";

const SNOOZE_LABEL: &str = "Snooze 10m";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Snoozed,
}

pub struct Notification {
    window: HWND,
    fonts: [HFONT; 2],
    /// Written by the window procedure, which reaches it through the window's user data.
    outcome: Box<Cell<Outcome>>,
    /// Which reminder this is about, as an index into the store. Reminders are only ever added
    /// to that list, never removed, so the index stays valid for the life of the notification.
    pub reminder: usize,
    /// Which step up from the bottom corner the window sits at.
    pub slot: usize,
}

impl Notification {
    pub fn show(text: &str, reminder: usize, slot: usize) -> io::Result<Self> {
        register_class()?;

        let (left, top, width, height) = placement(slot, 96);
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                wide(CLASS_NAME).as_ptr(),
                wide("Reminderski").as_ptr(),
                WS_POPUP | WS_BORDER,
                left,
                top,
                width,
                height,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }

        // Ignoring a notification must never lose the reminder, so anything other than pressing
        // Done counts as putting it off.
        let outcome = Box::new(Cell::new(Outcome::Snoozed));
        unsafe {
            SetWindowLongPtrW(
                window,
                GWLP_USERDATA,
                ptr::from_ref(outcome.as_ref()) as isize,
            )
        };

        // The monitor's DPI is only knowable once the window is on it, and the controls are laid
        // out in its pixels, so the window has to be resized to match before they are created.
        let dpi = unsafe { GetDpiForWindow(window) };
        let (left, top, width, height) = placement(slot, dpi);
        unsafe {
            SetWindowPos(
                window,
                ptr::null_mut(),
                left,
                top,
                width,
                height,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };

        let fonts = [ui_font(dpi), monospace_font(dpi)];
        let notification = Self {
            window,
            fonts,
            outcome,
            reminder,
            slot,
        };
        notification.fill(text, dpi);

        unsafe {
            // Showing without activating leaves the user typing wherever they already were.
            ShowWindow(window, SW_SHOWNOACTIVATE);
            MessageBeep(MB_ICONASTERISK);
        }
        Ok(notification)
    }

    /// Lays the four controls out against the window's own client area.
    fn fill(&self, text: &str, dpi: u32) {
        let margin = scale(MARGIN, dpi);
        let gap = scale(GAP, dpi);
        let persona_width = scale(PERSONA_WIDTH, dpi);
        let button_width = scale(BUTTON_WIDTH, dpi);
        let button_height = scale(BUTTON_HEIGHT, dpi);

        let mut client = RECT::default();
        unsafe { GetClientRect(self.window, &mut client) };
        let text_left = margin + persona_width + gap;
        let text_width = client.right - margin - text_left;
        let text_height = client.bottom - margin * 2 - gap - button_height;
        let buttons_top = client.bottom - margin - button_height;

        let persona = self.child(
            "STATIC",
            PERSONA,
            0,
            0,
            margin,
            margin,
            persona_width,
            text_height,
        );
        let body = self.child(
            "STATIC",
            text,
            0,
            0,
            text_left,
            margin,
            text_width,
            text_height,
        );
        let snooze = self.child(
            "BUTTON",
            SNOOZE_LABEL,
            WS_TABSTOP,
            ID_SNOOZE,
            client.right - margin - button_width,
            buttons_top,
            button_width,
            button_height,
        );
        let done = self.child(
            "BUTTON",
            "Done",
            WS_TABSTOP | BS_DEFPUSHBUTTON as u32,
            ID_DONE,
            client.right - margin - button_width * 2 - gap,
            buttons_top,
            button_width,
            button_height,
        );

        set_font(persona, self.fonts[1]);
        for control in [body, done, snooze] {
            set_font(control, self.fonts[0]);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn child(
        &self,
        class: &str,
        text: &str,
        styles: u32,
        id: i32,
        left: i32,
        top: i32,
        width: i32,
        height: i32,
    ) -> HWND {
        unsafe {
            CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | styles,
                left,
                top,
                width,
                height,
                self.window,
                ptr::without_provenance_mut(id as usize),
                ptr::null_mut(),
                ptr::null(),
            )
        }
    }

    pub fn window(&self) -> HWND {
        self.window
    }

    pub fn finished(&self) -> bool {
        unsafe { IsWindow(self.window) == 0 }
    }

    pub fn outcome(&self) -> Outcome {
        self.outcome.get()
    }
}

impl Drop for Notification {
    fn drop(&mut self) {
        if !self.finished() {
            unsafe { DestroyWindow(self.window) };
        }
        for font in self.fonts {
            if !font.is_null() {
                unsafe { DeleteObject(font as _) };
            }
        }
    }
}

fn set_font(control: HWND, font: HFONT) {
    if !control.is_null() && !font.is_null() {
        unsafe { SendMessageW(control, WM_SETFONT, font as WPARAM, 1) };
    }
}

/// Where the window belongs: stacked up from the bottom corner of the monitor being worked on.
fn placement(slot: usize, dpi: u32) -> (i32, i32, i32, i32) {
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) };
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };

    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    // Falling back to a fixed corner is better than refusing to show the reminder at all.
    let work = if unsafe { GetMonitorInfoW(monitor, &mut info) } != 0 {
        info.rcWork
    } else {
        RECT {
            left: 0,
            top: 0,
            right: 1024,
            bottom: 768,
        }
    };

    let width = scale(WIDTH, dpi);
    let height = scale(HEIGHT, dpi);
    let margin = scale(MARGIN, dpi);
    let step = (height + margin) * slot as i32;

    (
        work.right - margin - width,
        work.bottom - margin - height - step,
        width,
        height,
    )
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
            hbrBackground: COLOR_DIALOG_BRUSH as _,
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
        WM_COMMAND => {
            // Escape arrives here as Cancel, by way of the dialog message handling in the
            // application's message loop.
            let chosen = match (wparam & 0xffff) as i32 {
                ID_DONE | IDOK => Some(Outcome::Done),
                ID_SNOOZE | IDCANCEL => Some(Outcome::Snoozed),
                _ => None,
            };
            if let Some(chosen) = chosen {
                unsafe {
                    record(window, chosen);
                    DestroyWindow(window);
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe { DestroyWindow(window) };
            0
        }
        // WM_DESTROY deliberately does nothing: this window is one of several, and posting a
        // quit message would end the application.
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

unsafe fn record(window: HWND, chosen: Outcome) {
    let stored = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const Cell<Outcome>;
    if !stored.is_null() {
        unsafe { (*stored).set(chosen) };
    }
}
