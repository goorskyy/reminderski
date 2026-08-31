//! The notification shown when a reminder falls due.
//!
//! Everything in it is drawn by hand: black title bar, white ground, hairline boxes, monospace
//! throughout, and one filled button for the action you actually want. Native controls were
//! tried first and could not be made to look like this, so the window paints itself and does
//! its own hit testing. There are only a handful of rectangles, which is why that is cheaper
//! than fighting the theming.
//!
//! The window is modeless. A notification that blocked the message loop would stop the capture
//! shortcut working for as long as it went unanswered, which is exactly how long a reminder
//! that arrives at a bad moment tends to sit there.

use std::cell::Cell;
use std::io;
use std::mem;
use std::ptr;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DT_CENTER, DT_LEFT,
    DT_NOPREFIX, DT_SINGLELINE, DT_TOP, DT_VCENTER, DT_WORDBREAK, DeleteDC, DeleteObject, EndPaint,
    GetMonitorInfoW, HFONT, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint, PAINTSTRUCT, SRCCOPY, SelectObject, SetBkMode, TRANSPARENT,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, TRACKMOUSEEVENT, TrackMouseEvent, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GWLP_USERDATA, GetClientRect, GetCursorPos,
    GetWindowLongPtrW, HTCAPTION, IDC_ARROW, IsWindow, KillTimer, LoadCursorW, MB_ICONASTERISK,
    RegisterClassW, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOZORDER, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, WM_CLOSE, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
    WM_NCLBUTTONDOWN, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::persona::{self, Mood};
use crate::ui::{
    BAR, BAR_INK, HOVER, HOVER_FILLED, INK, LABEL, PAPER, centred_block, contains, draw_text, fill,
    inset, mono_font, outline, point_of, scale, wide,
};

const CLASS_NAME: &str = "ReminderskiNotification";
const TITLE: &str = "REMINDERSKI !!";

/// Logical pixels at 96 DPI.
const WIDTH: i32 = 520;
const HEIGHT: i32 = 284;
const TITLEBAR: i32 = 34;
const PADDING: i32 = 18;
const GAP: i32 = 10;
const FACE: i32 = 152;
const BUTTON_HEIGHT: i32 = 44;
const DONE_WIDTH: i32 = 118;

/// Type sizes, also in logical pixels.
const TITLE_SIZE: i32 = 13;
const LABEL_SIZE: i32 = 11;
const MESSAGE_SIZE: i32 = 17;
const BUTTON_SIZE: i32 = 12;
const PERSONA_SIZE: i32 = 19;

/// Letter spacing for the uppercase runs, which is most of the window.
const TITLE_TRACKING: i32 = 3;
const LABEL_TRACKING: i32 = 2;

const ANIMATION_TIMER: usize = 1;

/// Sent once after TrackMouseEvent is asked for it. It lives with the common controls rather
/// than the window messages, which is the only reason it is written out here.
pub const WM_MOUSELEAVE: u32 = 0x02a3;

/// The buttons, in the order they are drawn from the left. Done is last and is the filled one.
const ACTIONS: [(&str, &str); 4] = [
    ("+30 MIN", "30m"),
    ("+2 HRS", "2h"),
    ("TOMORROW", "tomorrow"),
    ("DONE  \u{2713}", ""),
];
const DONE: usize = ACTIONS.len() - 1;

/// What the user did with the notification.
#[derive(Clone, Copy)]
pub enum Outcome {
    Done,
    /// Pushed back by this much. Closing or ignoring the window means the first snooze, so a
    /// reminder is never lost by being dismissed.
    Later(Duration),
}

pub struct Notification {
    window: HWND,
    /// Read and written by the window procedure, which reaches it through the window's user data.
    state: Box<State>,
    /// Which reminder this is about, as an index into the store. Reminders are only ever added
    /// to that list, never removed, so the index stays valid for the life of the notification.
    pub reminder: usize,
    /// Which step up from the bottom corner the window sits at.
    pub slot: usize,
}

struct State {
    text: Vec<u16>,
    eyebrow: Vec<u16>,
    snoozes: u32,
    opened: Instant,
    outcome: Cell<Outcome>,
    mood: Cell<Mood>,
    frame: Cell<usize>,
    hovered: Cell<Option<usize>>,
    pressed: Cell<Option<usize>>,
    tracking: Cell<bool>,
    dpi: u32,
    fonts: Fonts,
}

struct Fonts {
    title: HFONT,
    label: HFONT,
    message: HFONT,
    button: HFONT,
    persona: HFONT,
}

/// Where everything sits inside the client area, in device pixels.
struct Layout {
    titlebar: RECT,
    close: RECT,
    face: RECT,
    eyebrow: RECT,
    message: RECT,
    buttons: [RECT; ACTIONS.len()],
}

impl Notification {
    pub fn show(text: &str, snoozes: u32, reminder: usize, slot: usize) -> io::Result<Self> {
        register_class()?;

        let (left, top, width, height) = placement(slot, 96);
        let window = unsafe {
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                wide(CLASS_NAME).as_ptr(),
                wide(TITLE).as_ptr(),
                WS_POPUP,
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

        // The monitor's DPI is only knowable once the window is on it, and everything is drawn
        // in its pixels, so the window is resized to match before anything is painted.
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

        let state = Box::new(State::new(text, snoozes, dpi));
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, ptr::from_ref(&*state) as isize) };

        let notification = Self {
            window,
            state,
            reminder,
            slot,
        };
        notification.schedule_next_frame();

        unsafe {
            // Showing without activating leaves the user typing wherever they already were.
            ShowWindow(window, SW_SHOWNOACTIVATE);
            MessageBeep(MB_ICONASTERISK);
        }
        Ok(notification)
    }

    fn schedule_next_frame(&self) {
        self.state.schedule_next_frame(self.window);
    }

    pub fn window(&self) -> HWND {
        self.window
    }

    pub fn finished(&self) -> bool {
        unsafe { IsWindow(self.window) == 0 }
    }

    pub fn outcome(&self) -> Outcome {
        self.state.outcome.get()
    }
}

impl Drop for Notification {
    fn drop(&mut self) {
        if !self.finished() {
            unsafe { DestroyWindow(self.window) };
        }
        self.state.fonts.delete();
    }
}

impl State {
    fn new(text: &str, snoozes: u32, dpi: u32) -> Self {
        let eyebrow = if snoozes == 0 {
            "EXCUSE ME... BUT:".to_string()
        } else {
            format!("EXCUSE ME... BUT: (snoozed {snoozes}x)")
        };

        Self {
            text: wide(text),
            eyebrow: wide(&eyebrow),
            snoozes,
            opened: Instant::now(),
            // Ignoring the window is the same as pressing the first snooze.
            outcome: Cell::new(Outcome::Later(Duration::from_secs(30 * 60))),
            mood: Cell::new(persona::mood(snoozes, Duration::ZERO)),
            frame: Cell::new(0),
            hovered: Cell::new(None),
            pressed: Cell::new(None),
            tracking: Cell::new(false),
            dpi,
            fonts: Fonts::new(dpi),
        }
    }

    /// Advances the character and asks to be woken when its current frame runs out.
    fn schedule_next_frame(&self, window: HWND) {
        let wanted = persona::mood(self.snoozes, self.opened.elapsed());
        if wanted != self.mood.get() {
            self.mood.set(wanted);
            self.frame.set(0);
        }

        let frames = persona::frames(self.mood.get());
        let millis = frames[self.frame.get() % frames.len()].millis;
        unsafe { SetTimer(window, ANIMATION_TIMER, millis as u32, None) };
    }

    fn art(&self) -> Vec<u16> {
        let frames = persona::frames(self.mood.get());
        wide(frames[self.frame.get() % frames.len()].art)
    }
}

impl Fonts {
    fn new(dpi: u32) -> Self {
        Self {
            title: mono_font(dpi, TITLE_SIZE, true),
            label: mono_font(dpi, LABEL_SIZE, false),
            message: mono_font(dpi, MESSAGE_SIZE, false),
            button: mono_font(dpi, BUTTON_SIZE, false),
            persona: mono_font(dpi, PERSONA_SIZE, false),
        }
    }

    fn delete(&self) {
        for font in [
            self.title,
            self.label,
            self.message,
            self.button,
            self.persona,
        ] {
            if !font.is_null() {
                unsafe { DeleteObject(font as _) };
            }
        }
    }
}

impl Layout {
    fn of(window: HWND, dpi: u32) -> Self {
        let mut client = RECT::default();
        unsafe { GetClientRect(window, &mut client) };

        let titlebar_height = scale(TITLEBAR, dpi);
        let padding = scale(PADDING, dpi);
        let gap = scale(GAP, dpi);
        let face = scale(FACE, dpi);
        let button_height = scale(BUTTON_HEIGHT, dpi);
        let done_width = scale(DONE_WIDTH, dpi);
        let label_height = scale(LABEL_SIZE * 2, dpi);

        let titlebar = RECT {
            left: 0,
            top: 0,
            right: client.right,
            bottom: titlebar_height,
        };
        let close = RECT {
            left: client.right - scale(46, dpi),
            top: 0,
            right: client.right,
            bottom: titlebar_height,
        };

        let body_top = titlebar_height + padding;
        let face_rect = RECT {
            left: padding,
            top: body_top,
            right: padding + face,
            bottom: body_top + face,
        };
        let column_left = face_rect.right + scale(18, dpi);
        let eyebrow = RECT {
            left: column_left,
            top: body_top,
            right: client.right - padding,
            bottom: body_top + label_height,
        };
        let message = RECT {
            left: column_left,
            top: eyebrow.bottom + gap,
            right: client.right - padding,
            bottom: face_rect.bottom,
        };

        // Done keeps a fixed width; the three snoozes share what is left.
        let buttons_top = face_rect.bottom + scale(16, dpi);
        let available = client.right - padding * 2 - done_width - gap * (ACTIONS.len() as i32 - 1);
        let snooze_width = available / (ACTIONS.len() as i32 - 1);
        let mut buttons = [RECT::default(); ACTIONS.len()];
        let mut left = padding;
        for (index, button) in buttons.iter_mut().enumerate() {
            let width = if index == DONE {
                done_width
            } else {
                snooze_width
            };
            let left_edge = if index == DONE {
                client.right - padding - done_width
            } else {
                left
            };
            *button = RECT {
                left: left_edge,
                top: buttons_top,
                right: left_edge + width,
                bottom: buttons_top + button_height,
            };
            left += width + gap;
        }

        Self {
            titlebar,
            close,
            face: face_rect,
            eyebrow,
            message,
            buttons,
        }
    }

    fn hit(&self, point: POINT) -> Option<usize> {
        self.buttons
            .iter()
            .position(|button| contains(button, point))
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
            // Every pixel is painted in WM_PAINT, so there is no background to erase.
            hbrBackground: ptr::null_mut(),
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

/// Where the window belongs: stacked up from the bottom corner of the monitor being worked on.
fn placement(slot: usize, dpi: u32) -> (i32, i32, i32, i32) {
    let mut cursor = POINT::default();
    unsafe { GetCursorPos(&mut cursor) };
    let monitor = unsafe { MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST) };

    let mut info = MONITORINFO {
        cbSize: mem::size_of::<MONITORINFO>() as u32,
        ..unsafe { mem::zeroed() }
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
    let margin = scale(12, dpi);
    let step = (height + margin) * slot as i32;

    (
        work.right - margin - width,
        work.bottom - margin - height - step,
        width,
        height,
    )
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let stored = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as *const State;
    if stored.is_null() {
        // Messages sent while the window is still being created have nothing to draw yet.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    }
    let state = unsafe { &*stored };
    let layout = Layout::of(window, state.dpi);

    match message {
        WM_PAINT => {
            paint(window, state, &layout);
            0
        }
        WM_TIMER => {
            state.frame.set(state.frame.get() + 1);
            unsafe { InvalidateRect(window, &layout.face, 0) };
            state.schedule_next_frame(window);
            0
        }
        WM_MOUSEMOVE => {
            track_mouse(window, state);
            let hovered = layout.hit(point_of(lparam));
            if hovered != state.hovered.get() {
                state.hovered.set(hovered);
                unsafe { InvalidateRect(window, ptr::null(), 0) };
            }
            0
        }
        WM_MOUSELEAVE => {
            state.tracking.set(false);
            if state.hovered.get().is_some() {
                state.hovered.set(None);
                unsafe { InvalidateRect(window, ptr::null(), 0) };
            }
            0
        }
        WM_LBUTTONDOWN => {
            let point = point_of(lparam);
            if let Some(index) = layout.hit(point) {
                state.pressed.set(Some(index));
                unsafe { InvalidateRect(window, ptr::null(), 0) };
            } else if contains(&layout.close, point) {
                unsafe { DestroyWindow(window) };
            } else if contains(&layout.titlebar, point) {
                // Hand the drag to Windows, which is what makes it feel like a title bar.
                unsafe {
                    ReleaseCapture();
                    DefWindowProcW(window, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, 0);
                }
            }
            0
        }
        WM_LBUTTONUP => {
            let pressed = state.pressed.take();
            if let Some(index) = pressed
                && layout.hit(point_of(lparam)) == Some(index)
            {
                answer(window, state, index);
            } else {
                unsafe { InvalidateRect(window, ptr::null(), 0) };
            }
            0
        }
        WM_KEYDOWN => {
            match wparam as u16 {
                VK_RETURN => answer(window, state, DONE),
                // Escape leaves the standing outcome alone, which is the first snooze.
                VK_ESCAPE => {
                    unsafe { DestroyWindow(window) };
                }
                // 1, 2 and 3 pick the snoozes from the left.
                key @ (0x31..=0x33) => answer(window, state, key as usize - 0x31),
                _ => {}
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

/// Records what the user chose and closes the window.
fn answer(window: HWND, state: &State, index: usize) {
    let outcome = if index == DONE {
        Outcome::Done
    } else {
        // The snooze labels are written as time expressions so the parser stays the one place
        // that knows what "tomorrow" means.
        match crate::reminder::parse_when(ACTIONS[index].1) {
            Some(delay) => Outcome::Later(delay),
            None => state.outcome.get(),
        }
    };
    state.outcome.set(outcome);
    unsafe {
        KillTimer(window, ANIMATION_TIMER);
        DestroyWindow(window);
    }
}

/// Asks for one WM_MOUSELEAVE, so a button does not stay lit after the pointer goes.
fn track_mouse(window: HWND, state: &State) {
    const TME_LEAVE: u32 = 2;
    if state.tracking.get() {
        return;
    }
    let mut request = TRACKMOUSEEVENT {
        cbSize: mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: window,
        dwHoverTime: 0,
    };
    if unsafe { TrackMouseEvent(&mut request) } != 0 {
        state.tracking.set(true);
    }
}

/// Draws the whole window into a bitmap and blits it, so nothing flickers as the character moves.
fn paint(window: HWND, state: &State, layout: &Layout) {
    let mut info: PAINTSTRUCT = unsafe { mem::zeroed() };
    let screen = unsafe { BeginPaint(window, &mut info) };

    let mut client = RECT::default();
    unsafe { GetClientRect(window, &mut client) };
    let buffer = unsafe { CreateCompatibleDC(screen) };
    let bitmap = unsafe { CreateCompatibleBitmap(screen, client.right, client.bottom) };
    let previous = unsafe { SelectObject(buffer, bitmap as _) };
    unsafe { SetBkMode(buffer, TRANSPARENT as i32) };

    fill(buffer, &client, PAPER);
    outline(buffer, &client, INK);

    fill(buffer, &layout.titlebar, BAR);
    let title_text = inset(&layout.titlebar, scale(14, state.dpi), 0);
    draw_text(
        buffer,
        &title_text,
        &wide(TITLE),
        state.fonts.title,
        BAR_INK,
        TITLE_TRACKING,
        state.dpi,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_text(
        buffer,
        &layout.close,
        &wide("[X]"),
        state.fonts.title,
        BAR_INK,
        0,
        state.dpi,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    outline(buffer, &layout.face, INK);
    centred_block(
        buffer,
        &layout.face,
        &state.art(),
        state.fonts.persona,
        INK,
        state.dpi,
    );

    draw_text(
        buffer,
        &layout.eyebrow,
        &state.eyebrow,
        state.fonts.label,
        LABEL,
        LABEL_TRACKING,
        state.dpi,
        DT_LEFT | DT_TOP | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_text(
        buffer,
        &layout.message,
        &state.text,
        state.fonts.message,
        INK,
        0,
        state.dpi,
        DT_LEFT | DT_TOP | DT_WORDBREAK | DT_NOPREFIX,
    );

    for (index, button) in layout.buttons.iter().enumerate() {
        let filled = index == DONE;
        let lit = state.hovered.get() == Some(index) || state.pressed.get() == Some(index);
        let background = match (filled, lit) {
            (true, false) => Some(INK),
            (true, true) => Some(HOVER_FILLED),
            (false, true) => Some(HOVER),
            (false, false) => None,
        };
        if let Some(background) = background {
            fill(buffer, button, background);
        }
        if !filled {
            outline(buffer, button, INK);
        }
        draw_text(
            buffer,
            button,
            &wide(ACTIONS[index].0),
            state.fonts.button,
            if filled { PAPER } else { INK },
            LABEL_TRACKING,
            state.dpi,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }

    unsafe {
        BitBlt(
            screen,
            0,
            0,
            client.right,
            client.bottom,
            buffer,
            0,
            0,
            SRCCOPY,
        );
        SelectObject(buffer, previous);
        DeleteObject(bitmap as _);
        DeleteDC(buffer);
        EndPaint(window, &info);
    }
}
