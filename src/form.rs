//! The reminder input form.
//!
//! Drawn by hand like the notification: black title bar, white ground, uppercase grey labels,
//! hairline boxes and one filled button. The two places you type are real edit controls with
//! their borders taken off and our own frame drawn around them, because a text field has a
//! caret, a selection, an undo stack and an input method behind it, and none of that is worth
//! rewriting to change how a rectangle looks.

use std::cell::Cell;
use std::io;
use std::mem;
use std::ptr;
use std::sync::OnceLock;
use std::time::Duration;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, CreateSolidBrush, DT_CENTER,
    DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_TOP, DT_VCENTER, DeleteDC, DeleteObject, EndPaint,
    GetMonitorInfoW, HBRUSH, HFONT, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO,
    MonitorFromPoint, PAINTSTRUCT, SRCCOPY, SelectObject, SetBkColor, SetBkMode, SetTextColor,
    TRANSPARENT,
};
use windows_sys::Win32::System::Diagnostics::Debug::MessageBeep;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::HiDpi::GetDpiForWindow;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, GetFocus, ReleaseCapture, SetFocus, TRACKMOUSEEVENT, TrackMouseEvent,
    VK_CONTROL, VK_ESCAPE, VK_RETURN, VK_TAB,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, ES_AUTOHSCROLL,
    ES_AUTOVSCROLL, ES_MULTILINE, ES_WANTRETURN, GWLP_USERDATA, GetClientRect, GetCursorPos,
    GetDlgItem, GetMessageW, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW, HTCAPTION,
    IDC_ARROW, LoadCursorW, MB_ICONERROR, MSG, MoveWindow, PostQuitMessage, RegisterClassW,
    SW_SHOW, SWP_NOACTIVATE, SWP_NOZORDER, SendMessageW, SetForegroundWindow, SetWindowLongPtrW,
    SetWindowPos, SetWindowTextW, ShowWindow, TranslateMessage, WM_CTLCOLOREDIT, WM_DESTROY,
    WM_DPICHANGED, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCLBUTTONDOWN,
    WM_PAINT, WM_SETFONT, WNDCLASSW, WS_CHILD, WS_POPUP, WS_VISIBLE,
};

use crate::notify::WM_MOUSELEAVE;
use crate::reminder;
use crate::ui::{
    BAR, BAR_INK, BUTTON_SIZE, FIELD_SIZE, HINT_SIZE, HOVER, HOVER_FILLED, INK, LABEL, LABEL_SIZE,
    LABEL_TRACKING, PAPER, TITLE_SIZE, TITLE_TRACKING, contains, draw_text, fill, inset, mono_font,
    outline, point_of, scale, wide,
};

const CLASS_NAME: &str = "ReminderskiForm";
const TITLE: &str = "REMINDERSKI";

const ID_TEXT: i32 = 1;
const ID_WHEN: i32 = 2;

/// Logical pixels at 96 DPI.
const WIDTH: i32 = 540;
const TITLEBAR: i32 = 34;
const PADDING: i32 = 24;
const LABEL_GAP: i32 = 10;
const GROUP_GAP: i32 = 20;
const TEXT_HEIGHT: i32 = 76;
const ROW_HEIGHT: i32 = 40;
const QUICK_HEIGHT: i32 = 38;
const QUICK_GAP: i32 = 12;
const SET_WIDTH: i32 = 88;
const CANCEL_WIDTH: i32 = 118;
const CANCEL_HEIGHT: i32 = 38;
const HINT_HEIGHT: i32 = 17;
const EDIT_INSET: i32 = 10;

/// One click for the times a reminder actually gets set for.
const QUICK: [(&str, &str); 3] = [
    ("10 MIN", "10m"),
    ("1 HOUR", "1h"),
    ("TOMORROW 9AM", "tomorrow"),
];

const HINT: &str = "e.g. \"in 45m\"   \"at 12\"   \"tomorrow 9\"   \"monday 14\"";

/// Buttons that are not one of the quick picks, numbered on from them.
const SET: usize = QUICK.len();
const CANCEL: usize = QUICK.len() + 1;
const BUTTONS: usize = QUICK.len() + 2;

/// What the user accepted: the reminder itself, and how long from now it is due.
pub struct Entry {
    pub text: String,
    pub delay: Duration,
}

struct Form {
    window: HWND,
    text: HWND,
    when: HWND,
    state: Box<State>,
}

struct State {
    dpi: Cell<u32>,
    fonts: Cell<Fonts>,
    /// Handed to Windows to paint behind the edit controls, so it must outlive every repaint.
    field_brush: HBRUSH,
    hovered: Cell<Option<usize>>,
    pressed: Cell<Option<usize>>,
    tracking: Cell<bool>,
    /// Set by the window procedure when a button is clicked, read by the message loop, which is
    /// the half that can reach the fields.
    chosen: Cell<Option<usize>>,
}

#[derive(Clone, Copy)]
struct Fonts {
    title: HFONT,
    label: HFONT,
    field: HFONT,
    button: HFONT,
    hint: HFONT,
}

/// Where everything sits inside the client area, in device pixels.
struct Layout {
    titlebar: RECT,
    close: RECT,
    about_label: RECT,
    text_box: RECT,
    when_label: RECT,
    time_label: RECT,
    time_box: RECT,
    hint: RECT,
    buttons: [RECT; BUTTONS],
}

enum Answer {
    Accepted(Entry),
    Cancelled,
    Rejected,
}

/// Shows the form pre-filled with `initial`, and blocks until the user accepts or cancels.
pub fn show(initial: &str) -> io::Result<Option<Entry>> {
    let form = Form::create()?;
    unsafe { SetWindowTextW(form.text, wide(initial).as_ptr()) };

    // With a capture in hand the only thing left to say is when, so start there. Without one the
    // reminder still has to be written.
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
/// The keys are handled here rather than in the window procedure because the edit controls have
/// the focus and consume them, and intercepting before dispatch avoids subclassing both.
fn run_modal_loop(form: &Form) -> Option<Entry> {
    let mut entered = None;
    let mut message: MSG = unsafe { mem::zeroed() };
    loop {
        if unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } <= 0 {
            return entered;
        }

        // A button was clicked on the previous turn through the loop. When it did not close the
        // form the message still has to be dispatched, so only a close skips the rest.
        if let Some(button) = form.state.chosen.take()
            && form.settle(button, &mut entered)
        {
            continue;
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
                    form.settle(SET, &mut entered);
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
    /// Acts on a button. Returns true when the form is closing.
    ///
    /// Closing does not return from the message loop. Destroying the window posts a quit
    /// message, and that message has to be taken out of the queue here: the application's own
    /// loop would read it as a reason to shut down, and the reminder just set would never be
    /// delivered.
    fn settle(&self, button: usize, entered: &mut Option<Entry>) -> bool {
        match self.answer(button) {
            Answer::Accepted(entry) => {
                *entered = Some(entry);
                unsafe { DestroyWindow(self.window) };
                true
            }
            Answer::Cancelled => {
                unsafe { DestroyWindow(self.window) };
                true
            }
            Answer::Rejected => {
                self.reject();
                false
            }
        }
    }

    fn create() -> io::Result<Self> {
        register_class()?;

        let window = unsafe {
            CreateWindowExW(
                0,
                wide(CLASS_NAME).as_ptr(),
                wide(TITLE).as_ptr(),
                WS_POPUP,
                0,
                0,
                scale(WIDTH, 96),
                height(96),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }

        // Everything is laid out in the monitor's pixels, so the window is sized to match before
        // the fields are placed in it.
        let dpi = unsafe { GetDpiForWindow(window) };
        centre_on_active_monitor(window, dpi);

        // The edit styles are declared as i32 while the window styles are u32.
        // No scroll bar: a themed one is the only part of this window Windows would draw, and
        // ES_AUTOVSCROLL scrolls the text without it.
        let text = create_edit(
            window,
            ID_TEXT,
            (ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN) as u32,
        );
        let when = create_edit(window, ID_WHEN, ES_AUTOHSCROLL as u32);
        if text.is_null() || when.is_null() {
            let error = io::Error::last_os_error();
            unsafe { DestroyWindow(window) };
            return Err(error);
        }

        let state = Box::new(State::new(dpi));
        unsafe { SetWindowLongPtrW(window, GWLP_USERDATA, ptr::from_ref(&*state) as isize) };
        let field_font = state.fonts.get().field as WPARAM;
        for field in [text, when] {
            unsafe { SendMessageW(field, WM_SETFONT, field_font, 1) };
        }
        place_fields(window, dpi);

        Ok(Self {
            window,
            text,
            when,
            state,
        })
    }

    /// What pressing a button means, once the fields have been read.
    fn answer(&self, button: usize) -> Answer {
        if button == CANCEL {
            return Answer::Cancelled;
        }

        let text = read_text(self.text);
        if text.trim().is_empty() {
            return Answer::Rejected;
        }

        let expression = match QUICK.get(button) {
            Some((_, expression)) => (*expression).to_string(),
            None => read_text(self.when),
        };
        match reminder::parse_when(&expression) {
            Some(delay) => Answer::Accepted(Entry { text, delay }),
            None => Answer::Rejected,
        }
    }

    /// Says no without saying which half was wrong, which is the thing left to improve here.
    fn reject(&self) {
        unsafe {
            MessageBeep(MB_ICONERROR);
            SetFocus(self.when);
        }
    }
}

impl Drop for Form {
    fn drop(&mut self) {
        self.state.fonts.get().delete();
        if !self.state.field_brush.is_null() {
            unsafe { DeleteObject(self.state.field_brush as _) };
        }
    }
}

impl State {
    fn new(dpi: u32) -> Self {
        Self {
            dpi: Cell::new(dpi),
            fonts: Cell::new(Fonts::new(dpi)),
            field_brush: unsafe { CreateSolidBrush(PAPER) },
            hovered: Cell::new(None),
            pressed: Cell::new(None),
            tracking: Cell::new(false),
            chosen: Cell::new(None),
        }
    }

    /// Redraws at another monitor's scale. The fonts are built for one DPI, so they are
    /// thrown away and made again rather than stretched.
    fn adopt_dpi(&self, dpi: u32) {
        self.dpi.set(dpi);
        self.fonts.get().delete();
        self.fonts.set(Fonts::new(dpi));
    }
}

impl Fonts {
    fn new(dpi: u32) -> Self {
        Self {
            title: mono_font(dpi, TITLE_SIZE, true),
            label: mono_font(dpi, LABEL_SIZE, false),
            field: mono_font(dpi, FIELD_SIZE, false),
            button: mono_font(dpi, BUTTON_SIZE, false),
            hint: mono_font(dpi, HINT_SIZE, false),
        }
    }

    fn delete(&self) {
        for font in [self.title, self.label, self.field, self.button, self.hint] {
            if !font.is_null() {
                unsafe { DeleteObject(font as _) };
            }
        }
    }
}

/// The height the stacked rows add up to, in device pixels.
fn height(dpi: u32) -> i32 {
    let label = label_height(dpi);
    scale(TITLEBAR, dpi)
        + scale(PADDING, dpi) * 2
        + label * 3
        + scale(LABEL_GAP, dpi) * 4
        + scale(TEXT_HEIGHT, dpi)
        + scale(QUICK_HEIGHT, dpi)
        + scale(ROW_HEIGHT, dpi)
        + scale(HINT_HEIGHT, dpi)
        + scale(CANCEL_HEIGHT, dpi)
        + scale(GROUP_GAP, dpi) * 3
}

fn label_height(dpi: u32) -> i32 {
    scale(LABEL_SIZE, dpi) * 3 / 2
}

impl Layout {
    fn of(window: HWND, dpi: u32) -> Self {
        let mut client = RECT::default();
        unsafe { GetClientRect(window, &mut client) };

        let padding = scale(PADDING, dpi);
        let label_gap = scale(LABEL_GAP, dpi);
        let group_gap = scale(GROUP_GAP, dpi);
        let titlebar_height = scale(TITLEBAR, dpi);

        let left = padding;
        let right = client.right - padding;
        let mut cursor = titlebar_height + padding;

        // Every row is full width and stacked, so they are taken off the top in order.
        let mut row = |height: i32, gap: i32| {
            let rect = RECT {
                left,
                top: cursor,
                right,
                bottom: cursor + height,
            };
            cursor += height + gap;
            rect
        };

        let about_label = row(label_height(dpi), label_gap);
        let text_box = row(scale(TEXT_HEIGHT, dpi), group_gap);
        let when_label = row(label_height(dpi), label_gap);
        let quick_row = row(scale(QUICK_HEIGHT, dpi), group_gap);
        let time_label = row(label_height(dpi), label_gap);
        let time_row = row(scale(ROW_HEIGHT, dpi), label_gap);
        let hint = row(scale(HINT_HEIGHT, dpi), group_gap);
        let cancel_row = row(scale(CANCEL_HEIGHT, dpi), 0);

        let mut buttons = [RECT::default(); BUTTONS];

        // The last quick pick is stretched to the right edge, so dividing by three cannot leave
        // it a pixel or two narrow than the other two.
        let quick_gap = scale(QUICK_GAP, dpi);
        let quick_width = (quick_row.right - quick_row.left - quick_gap * 2) / 3;
        for (index, button) in buttons.iter_mut().take(QUICK.len()).enumerate() {
            let start = quick_row.left + (quick_width + quick_gap) * index as i32;
            *button = RECT {
                left: start,
                right: if index == QUICK.len() - 1 {
                    quick_row.right
                } else {
                    start + quick_width
                },
                ..quick_row
            };
        }

        let set_width = scale(SET_WIDTH, dpi);
        buttons[SET] = RECT {
            left: time_row.right - set_width,
            ..time_row
        };
        let time_box = RECT {
            right: buttons[SET].left - quick_gap,
            ..time_row
        };

        buttons[CANCEL] = RECT {
            left: cancel_row.right - scale(CANCEL_WIDTH, dpi),
            ..cancel_row
        };

        Self {
            titlebar: RECT {
                left: 0,
                top: 0,
                right: client.right,
                bottom: titlebar_height,
            },
            close: RECT {
                left: client.right - scale(46, dpi),
                top: 0,
                right: client.right,
                bottom: titlebar_height,
            },
            about_label,
            text_box,
            when_label,
            time_label,
            time_box,
            hint,
            buttons,
        }
    }

    fn hit(&self, point: POINT) -> Option<usize> {
        self.buttons
            .iter()
            .position(|button| contains(button, point))
    }
}

/// Puts the edit controls inside the boxes drawn for them.
///
/// The multiline box fills its frame and its text starts at the top. A single line control
/// draws its text at the top too, so the time field is made one line tall and that line is
/// centred in the box instead.
fn place_fields(window: HWND, dpi: u32) {
    let layout = Layout::of(window, dpi);
    let inset_by = scale(EDIT_INSET, dpi);

    let text_area = inset(&layout.text_box, inset_by, inset_by / 2);
    let line = scale(FIELD_SIZE, dpi) * 3 / 2;
    let box_height = layout.time_box.bottom - layout.time_box.top;
    let time_area = RECT {
        top: layout.time_box.top + (box_height - line) / 2,
        bottom: layout.time_box.top + (box_height - line) / 2 + line,
        ..inset(&layout.time_box, inset_by, 0)
    };

    for (id, area) in [(ID_TEXT, text_area), (ID_WHEN, time_area)] {
        unsafe {
            MoveWindow(
                GetDlgItem(window, id),
                area.left,
                area.top,
                area.right - area.left,
                area.bottom - area.top,
                1,
            )
        };
    }
}

fn create_edit(parent: HWND, id: i32, styles: u32) -> HWND {
    unsafe {
        CreateWindowExW(
            0,
            wide("EDIT").as_ptr(),
            ptr::null(),
            // No border of its own: the frame around it is drawn with everything else.
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

/// Centres the form on whichever monitor the mouse is on, which is where the user is working.
fn centre_on_active_monitor(window: HWND, dpi: u32) {
    let width = scale(WIDTH, dpi);
    let tall = height(dpi);

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
            bottom: tall,
        }
    };

    let left = work.left + (work.right - work.left - width) / 2;
    let top = work.top + (work.bottom - work.top - tall) / 2;
    unsafe { SetWindowPos(window, ptr::null_mut(), left, top, width, tall, 0) };
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
    let layout = Layout::of(window, state.dpi.get());

    match message {
        WM_PAINT => {
            paint(window, state, &layout);
            0
        }
        // The edit controls paint themselves, and this is what stops them arriving grey.
        WM_CTLCOLOREDIT => {
            unsafe {
                SetTextColor(wparam as _, INK);
                SetBkColor(wparam as _, PAPER);
            }
            state.field_brush as LRESULT
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
                // The message loop owns the fields, so it is the half that decides what the
                // press meant.
                state.chosen.set(Some(index));
            }
            unsafe { InvalidateRect(window, ptr::null(), 0) };
            0
        }
        // Dragged onto a monitor at another scale. Windows says where the window should go; its
        // size comes from the design at the new DPI rather than from the suggested rectangle, so
        // that hopping between monitors cannot round the window away.
        WM_DPICHANGED => {
            let dpi = (wparam & 0xffff) as u32;
            state.adopt_dpi(dpi);
            let suggested = unsafe { &*(lparam as *const RECT) };
            unsafe {
                SetWindowPos(
                    window,
                    ptr::null_mut(),
                    suggested.left,
                    suggested.top,
                    scale(WIDTH, dpi),
                    height(dpi),
                    SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            let field_font = state.fonts.get().field as WPARAM;
            for id in [ID_TEXT, ID_WHEN] {
                unsafe { SendMessageW(GetDlgItem(window, id), WM_SETFONT, field_font, 1) };
            }
            place_fields(window, dpi);
            unsafe { InvalidateRect(window, ptr::null(), 0) };
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

/// Asks for one mouse-leave message, so a button does not stay lit after the pointer goes.
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

/// Draws the whole window into a bitmap and blits it, so nothing flickers as buttons light up.
fn paint(window: HWND, state: &State, layout: &Layout) {
    let dpi = state.dpi.get();
    let fonts = state.fonts.get();
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
    draw_text(
        buffer,
        &inset(&layout.titlebar, scale(14, dpi), 0),
        &wide(TITLE),
        fonts.title,
        BAR_INK,
        TITLE_TRACKING,
        dpi,
        DT_LEFT | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );
    draw_text(
        buffer,
        &layout.close,
        &wide("[X]"),
        fonts.title,
        BAR_INK,
        0,
        dpi,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

    for (rect, caption) in [
        (&layout.about_label, "REMIND ME ABOUT"),
        (&layout.when_label, "WHEN?"),
        (&layout.time_label, "OR TYPE A TIME"),
    ] {
        draw_text(
            buffer,
            rect,
            &wide(caption),
            fonts.label,
            LABEL,
            LABEL_TRACKING,
            dpi,
            DT_LEFT | DT_TOP | DT_SINGLELINE | DT_NOPREFIX,
        );
    }

    outline(buffer, &layout.text_box, INK);
    outline(buffer, &layout.time_box, INK);
    draw_text(
        buffer,
        &layout.hint,
        &wide(HINT),
        fonts.hint,
        LABEL,
        0,
        dpi,
        DT_LEFT | DT_TOP | DT_SINGLELINE | DT_NOPREFIX,
    );

    for (index, button) in layout.buttons.iter().enumerate() {
        let filled = index == SET;
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
        let caption = match index {
            SET => "SET",
            CANCEL => "CANCEL",
            _ => QUICK[index].0,
        };
        draw_text(
            buffer,
            button,
            &wide(caption),
            fonts.button,
            if filled { PAPER } else { INK },
            LABEL_TRACKING,
            dpi,
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
