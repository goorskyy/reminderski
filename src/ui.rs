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

/// A fixed-pitch font of a given height in logical pixels, scaled to the monitor.
///
/// The design is monospace throughout, so the windows ask for a size rather than inheriting
/// whatever the system has been set to.
pub fn mono_font(dpi: u32, height: i32, bold: bool) -> HFONT {
    // The fields below are single bytes in LOGFONTW, and naming the values here keeps the
    // widths from having to be juggled at each call.
    const DEFAULT_CHARSET: u8 = 1;
    const CLEARTYPE_QUALITY: u8 = 5;
    const FIXED_PITCH_MODERN: u8 = 1 | 48;

    let mut logical: LOGFONTW = unsafe { mem::zeroed() };
    // Negative asks for the character height rather than the cell height.
    logical.lfHeight = -scale(height, dpi);
    logical.lfWeight = if bold { 700 } else { 400 };
    logical.lfCharSet = DEFAULT_CHARSET;
    logical.lfQuality = CLEARTYPE_QUALITY;
    logical.lfPitchAndFamily = FIXED_PITCH_MODERN;

    let face = wide(MONOSPACE_FACE);
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
