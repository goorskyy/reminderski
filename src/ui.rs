//! The small Win32 pieces both windows need: the user's own fonts, DPI scaling and wide strings.

use std::iter::once;
use std::mem;
use std::ptr;

use windows_sys::Win32::Graphics::Gdi::{CreateFontIndirectW, HFONT, LOGFONTW};
use windows_sys::Win32::UI::HiDpi::SystemParametersInfoForDpi;
use windows_sys::Win32::UI::WindowsAndMessaging::{NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS};

/// A fixed-pitch face for the persona, which only lines up in one.
const MONOSPACE_FACE: &str = "Consolas";

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}

pub fn scale(logical: i32, dpi: u32) -> i32 {
    logical * dpi as i32 / 96
}

/// The font Windows uses for UI text, at the given DPI. Null when it cannot be read, in which
/// case controls keep the default font rather than failing outright.
pub fn ui_font(dpi: u32) -> HFONT {
    match message_font(dpi) {
        Some(logical) => unsafe { CreateFontIndirectW(&logical) },
        None => ptr::null_mut(),
    }
}

/// The same size and weight as the UI font, in a fixed-pitch face.
pub fn monospace_font(dpi: u32) -> HFONT {
    let Some(mut logical) = message_font(dpi) else {
        return ptr::null_mut();
    };

    let face = wide(MONOSPACE_FACE);
    logical.lfFaceName = [0; 32];
    logical.lfFaceName[..face.len()].copy_from_slice(&face);
    unsafe { CreateFontIndirectW(&logical) }
}

fn message_font(dpi: u32) -> Option<LOGFONTW> {
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
    (read != 0).then_some(metrics.lfMessageFont)
}
