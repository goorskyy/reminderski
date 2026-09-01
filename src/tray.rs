//! The icon in the notification area: the only sign the application is running, and the only
//! way to stop it now that there is no console to press Ctrl+C in.
//!
//! The icon is drawn rather than shipped as a resource, which keeps the build to `cargo build`
//! with no resource compiler in it. A black tile with a white R is also what the title bars
//! look like, so it belongs to the same design.

use std::io;
use std::mem;
use std::ptr;
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateBitmap, CreateCompatibleBitmap, CreateCompatibleDC, DT_CENTER, DT_NOPREFIX,
    DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject, GetDC, HBITMAP, ReleaseDC, SelectObject,
    SetBkMode, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, GetCursorPos, GetSystemMetrics, HICON, ICONINFO, IDC_ARROW,
    LoadCursorW, MF_STRING, PostQuitMessage, RegisterClassW, SM_CXSMICON, SM_CYSMICON,
    SetForegroundWindow, TPM_RIGHTBUTTON, TrackPopupMenu, WM_APP, WM_COMMAND, WM_NULL,
    WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};

use crate::ui::{BAR, BAR_INK, draw_text, fill, mono_font, wide};

const CLASS_NAME: &str = "ReminderskiTray";
const TOOLTIP: &str = "Reminderski — Ctrl+Alt+R to capture";

/// The message the shell sends us about the icon, and the one menu item there is.
const TRAY_MESSAGE: u32 = WM_APP + 1;
const ID_QUIT: usize = 1;

pub struct Tray {
    window: HWND,
    icon: HICON,
}

impl Tray {
    pub fn show() -> io::Result<Self> {
        register_class()?;

        // Never shown. It exists because the shell needs a window to send the icon's messages
        // to, and because a popup menu needs one to belong to.
        let window = unsafe {
            CreateWindowExW(
                0,
                wide(CLASS_NAME).as_ptr(),
                wide(TOOLTIP).as_ptr(),
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
        let tip = wide(TOOLTIP);
        data.szTip[..tip.len()].copy_from_slice(&tip);

        if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
            let error = io::Error::last_os_error();
            unsafe { DestroyWindow(window) };
            return Err(error);
        }

        Ok(Self { window, icon })
    }
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

/// A black tile with a white R, at whatever size the shell asks icons to be.
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
    unsafe { SetBkMode(buffer, TRANSPARENT as i32) };

    // The letter is asked for at the tile's height, which the icon sizes are a multiple of, so
    // this comes out the same shape however far the display is scaled.
    let font = mono_font(96, height * 3 / 4, true);
    draw_text(
        buffer,
        &square,
        &wide("R"),
        font,
        BAR_INK,
        0,
        96,
        DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
    );

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
        DeleteObject(font as _);
        DeleteObject(mask as _);
        DeleteObject(colour as _);
        DeleteDC(buffer);
        ReleaseDC(ptr::null_mut(), screen);
    }
    icon
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
        WM_COMMAND if (wparam & 0xffff) == ID_QUIT => {
            // Ends the application's message loop, which then puts the icon away.
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}

fn show_menu(window: HWND) {
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    unsafe { AppendMenuW(menu, MF_STRING, ID_QUIT, wide("Quit Reminderski").as_ptr()) };

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
