//! The icon in the notification area: the only sign the application is running, and the only
//! way to stop it now that there is no console to press Ctrl+C in.
//!
//! The icon is drawn rather than shipped as a resource, which keeps the build to `cargo build`
//! with no resource compiler in it. It is the note the notifications carry, reduced to the few
//! rectangles that survive being sixteen pixels across, on the black tile the title bars use.

use std::io;
use std::mem;
use std::ptr;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC,
    HBITMAP, HDC, ReleaseDC, SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_NONE, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW, Shell_NotifyIconW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GetCursorPos, GetSystemMetrics, HICON, ICONINFO, IDC_ARROW,
    LoadCursorW, MF_CHECKED, MF_STRING, MF_UNCHECKED, PostQuitMessage, RegisterClassW, SM_CXSMICON,
    SM_CYSMICON, SW_SHOWNORMAL, SetForegroundWindow, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP,
    WM_COMMAND, WM_LBUTTONDBLCLK, WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

use crate::settings::{self, Shortcut};
use crate::ui::{BAR, BAR_INK, fill, outline, wide};
use crate::{autostart, log};

const CLASS_NAME: &str = "ReminderskiTray";

/// The message the shell sends us about the icon, and the items on its menu.
const TRAY_MESSAGE: u32 = WM_APP + 1;
const ID_DASHBOARD: usize = 1;
const ID_SETTINGS: usize = 2;
const ID_AUTOSTART: usize = 3;
const ID_QUIT: usize = 4;

/// The dashboard address, so the menu can open it. Set once, before the icon appears.
static ADDRESS: OnceLock<String> = OnceLock::new();

pub struct Tray {
    window: HWND,
    icon: HICON,
}

impl Tray {
    /// `address` is where the dashboard is listening, when it managed to start.
    pub fn show(address: Option<String>, capture: Shortcut) -> io::Result<Self> {
        if let Some(address) = address {
            let _ = ADDRESS.set(address);
        }
        register_class()?;

        // The tip names the shortcut, which is no longer a fact the application can assume.
        let tooltip = format!("Reminderski — {} to capture", settings::describe(capture));

        // Never shown. It exists because the shell needs a window to send the icon's messages
        // to, and because a popup menu needs one to belong to.
        let window = unsafe {
            CreateWindowExW(
                0,
                wide(CLASS_NAME).as_ptr(),
                wide(&tooltip).as_ptr(),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        if window.is_null() {
            return Err(io::Error::last_os_error());
        }

        let icon = draw_icon();
        let mut data = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: window,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: icon,
            ..unsafe { mem::zeroed() }
        };
        copy_into(&mut data.szTip, &tooltip);

        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
            let error = io::Error::last_os_error();
            unsafe { DestroyWindow(window) };
            return Err(error);
        }

        Ok(Self { window, icon })
    }
}

impl Tray {
    /// Says something over the icon, in the balloon the shell puts there.
    ///
    /// Whether it is seen is not ours to decide: notifications can be turned off per
    /// application, and the shell holds them back while the screen is being shared. Nothing
    /// here depends on one arriving, which is why a refusal is not reported.
    pub fn announce(&self, title: &str, text: &str) {
        let mut data = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: 1,
            uFlags: NIF_INFO,
            dwInfoFlags: NIIF_NONE,
            ..unsafe { mem::zeroed() }
        };
        copy_into(&mut data.szInfoTitle, title);
        copy_into(&mut data.szInfo, text);

        unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
    }
}

/// Fills one of the shell's fixed-width fields, cutting the text short rather than overrunning.
///
/// The balloon holds 256 characters and its heading 64, which a reminder can exceed. Losing the
/// end of a sentence is better than losing the message.
fn copy_into(field: &mut [u16], text: &str) {
    let source = wide(text);
    if source.len() <= field.len() {
        field[..source.len()].copy_from_slice(&source);
        return;
    }
    let kept = field.len() - 1;
    field[..kept].copy_from_slice(&source[..kept]);
    field[kept] = 0;
}

impl Drop for Tray {
    fn drop(&mut self) {
        let data = NOTIFYICONDATAW {
            cbSize: mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.window,
            uID: 1,
            ..unsafe { mem::zeroed() }
        };
        // Without this the icon stays in the tray until something makes the shell notice.
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &data);
            DestroyWindow(self.window);
            DestroyIcon(self.icon);
        }
    }
}

/// The note, at whatever size the shell asks icons to be.
fn draw_icon() -> HICON {
    let width = unsafe { GetSystemMetrics(SM_CXSMICON) };
    let height = unsafe { GetSystemMetrics(SM_CYSMICON) };

    let screen = unsafe { GetDC(ptr::null_mut()) };
    let buffer = unsafe { CreateCompatibleDC(screen) };
    let colour = unsafe { CreateCompatibleBitmap(screen, width, height) };
    let previous = unsafe { SelectObject(buffer, colour as _) };

    let square = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    fill(buffer, &square, BAR);
    draw_note(buffer, width, height);

    // An all-zero mask means every pixel of the tile is opaque.
    let mask: HBITMAP = unsafe { CreateBitmap(width, height, 1, 1, ptr::null()) };
    let info = ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: colour,
    };
    let icon = unsafe { CreateIconIndirect(&info) };

    unsafe {
        SelectObject(buffer, previous);
        DeleteObject(mask as _);
        DeleteObject(colour as _);
        DeleteDC(buffer);
        ReleaseDC(ptr::null_mut(), screen);
    }
    icon
}

/// The pinned note, in the fewest rectangles it can be recognised from.
///
/// Every position is a fraction of the tile rather than a fixed pixel count, because the shell
/// asks for sixteen, twenty or twenty-four across depending on how far the display is scaled and
/// the mark has to hold together at all three. The wavy lower edge and the (o) of the pin are a
/// pixel each at the smallest size, so the pin is kept as a dot and the wave is dropped.
fn draw_note(hdc: HDC, width: i32, height: i32) {
    // Sixteenths of the tile: the size the mark was drawn at, so the fractions read as pixels.
    let across = |sixteenths: i32| width * sixteenths / 16;
    let down = |sixteenths: i32| height * sixteenths / 16;
    let box_of = |left, top, right, bottom| RECT {
        left: across(left),
        top: down(top),
        right: across(right),
        bottom: down(bottom),
    };

    let pin = box_of(7, 1, 9, 3);
    let note = box_of(2, 4, 14, 14);
    let left_eye = box_of(4, 7, 6, 9);
    let right_eye = box_of(10, 7, 12, 9);
    let mouth = box_of(6, 11, 10, 12);

    fill(hdc, &pin, BAR_INK);
    outline(hdc, &note, BAR_INK);
    fill(hdc, &left_eye, BAR_INK);
    fill(hdc, &right_eye, BAR_INK);
    fill(hdc, &mouth, BAR_INK);
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

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        TRAY_MESSAGE if (lparam as u32) == WM_RBUTTONUP => {
            show_menu(window);
            0
        }
        // Double-clicking the icon is the shortest way to the dashboard.
        TRAY_MESSAGE if (lparam as u32) == WM_LBUTTONDBLCLK => {
            open_dashboard();
            0
        }
        WM_COMMAND if (wparam & 0xffff) == ID_DASHBOARD => {
            open_dashboard();
            0
        }
        WM_COMMAND if (wparam & 0xffff) == ID_SETTINGS => {
            open_settings();
            0
        }
        WM_COMMAND if (wparam & 0xffff) == ID_AUTOSTART => {
            let wanted = !autostart::is_enabled();
            if let Err(error) = autostart::set(wanted) {
                log::problem(&format!("could not change the startup entry: {error}"));
            }
            0
        }
        WM_COMMAND if (wparam & 0xffff) == ID_QUIT => {
            // Ends the application's message loop, which then puts the icon away.
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

/// Hands the address to whatever the machine opens web pages with.
/// Opens the settings file in whatever this machine edits text with.
///
/// The path is worked out again rather than carried here. It is the one the application read at
/// startup, and by the time there is a menu to click, that read has already succeeded.
fn open_settings() {
    let Ok(path) = crate::store::default_path() else {
        return;
    };
    let settings = path.with_file_name("settings.txt");
    unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            wide("open").as_ptr(),
            wide(&settings.to_string_lossy()).as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
}

fn open_dashboard() {
    let Some(address) = ADDRESS.get() else {
        return;
    };
    unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            wide("open").as_ptr(),
            wide(address).as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
}

fn show_menu(window: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    unsafe {
        if ADDRESS.get().is_some() {
            AppendMenuW(
                menu,
                MF_STRING,
                ID_DASHBOARD,
                wide("Open dashboard").as_ptr(),
            );
        }
        // The tick is read off the registry each time the menu opens, so it still agrees after
        // the entry has been turned off somewhere else, such as Task Manager's Startup list.
        let ticked = if autostart::is_enabled() {
            MF_CHECKED
        } else {
            MF_UNCHECKED
        };
        AppendMenuW(
            menu,
            MF_STRING | ticked,
            ID_AUTOSTART,
            wide("Start with Windows").as_ptr(),
        );
        AppendMenuW(menu, MF_STRING, ID_SETTINGS, wide("Settings").as_ptr());
        AppendMenuW(menu, MF_STRING, ID_QUIT, wide("Quit Reminderski").as_ptr());
    }

    let mut cursor = POINT::default();
    unsafe {
        GetCursorPos(&mut cursor);
        // Taking the foreground first is what makes the menu close when clicked away from, and
        // the empty message afterwards is what stops it sticking the second time.
        SetForegroundWindow(window);
        TrackPopupMenu(
            menu,
            TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            0,
            window,
            ptr::null(),
        );
        DefWindowProcW(window, WM_NULL, 0, 0);
        DestroyMenu(menu);
    }
}
