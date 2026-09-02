//! The small Win32 pieces both windows need: the user's own fonts, DPI scaling and wide strings.

use std::iter::once;
use std::mem;

use windows_sys::Win32::Foundation::{COLORREF, LPARAM, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontIndirectW, CreateSolidBrush, DT_CALCRECT, DT_CENTER, DT_NOPREFIX, DT_SINGLELINE,
    DeleteObject, DrawTextW, FillRect, FrameRect, HBRUSH, HDC, HFONT, LOGFONTW, SelectObject,
    SetTextCharacterExtra, SetTextColor,
};

/// Everything is set in one fixed-pitch face, which is also the only kind the persona lines up in.
const MONOSPACE_FACE: &str = "Consolas";

/// One type scale for the whole application, in logical pixels at 96 DPI. Both windows read
/// their sizes from here so that the same kind of thing is never set two ways.
pub const TITLE_SIZE: i32 = 13;
pub const LABEL_SIZE: i32 = 13;
/// The examples under the time field, which sit below the labels.
pub const HINT_SIZE: i32 = 11;
pub const BUTTON_SIZE: i32 = 14;
/// What you type, and what the reminder says when it comes back.
pub const FIELD_SIZE: i32 = 15;
pub const MESSAGE_SIZE: i32 = 17;
/// The note is thirteen characters wide and six tall, which is what sets this: any larger and
/// it stops fitting the face box.
pub const PERSONA_SIZE: i32 = 17;

/// Letter spacing for the uppercase runs, which is most of both windows.
pub const TITLE_TRACKING: i32 = 3;
pub const LABEL_TRACKING: i32 = 2;

pub fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}

pub fn scale(logical: i32, dpi: u32) -> i32 {
    logical * dpi as i32 / 96
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

/// The palette, read off the photographs of the original.
pub const INK: COLORREF = rgb(0x0d, 0x0d, 0x0b);
pub const PAPER: COLORREF = rgb(0xff, 0xff, 0xff);
pub const LABEL: COLORREF = rgb(0x8b, 0x89, 0x7f);
pub const BAR: COLORREF = rgb(0x0a, 0x0a, 0x08);
pub const BAR_INK: COLORREF = rgb(0xf2, 0xf1, 0xea);
/// A hovered outline button, and a hovered filled one.
pub const HOVER: COLORREF = rgb(0xf0, 0xef, 0xea);
pub const HOVER_FILLED: COLORREF = rgb(0x26, 0x26, 0x1f);

pub const fn rgb(red: u8, green: u8, blue: u8) -> COLORREF {
    red as u32 | (green as u32) << 8 | (blue as u32) << 16
}

pub fn fill(hdc: HDC, rect: &RECT, color: COLORREF) {
    with_brush(color, |brush| unsafe {
        FillRect(hdc, rect, brush);
    });
}

/// A one pixel outline, which is the only border this design has.
pub fn outline(hdc: HDC, rect: &RECT, color: COLORREF) {
    with_brush(color, |brush| unsafe {
        FrameRect(hdc, rect, brush);
    });
}

fn with_brush(color: COLORREF, draw: impl FnOnce(HBRUSH)) {
    let brush = unsafe { CreateSolidBrush(color) };
    if brush.is_null() {
        return;
    }
    draw(brush);
    unsafe { DeleteObject(brush as _) };
}

#[allow(clippy::too_many_arguments)]
pub fn draw_text(
    hdc: HDC,
    rect: &RECT,
    content: &[u16],
    font: HFONT,
    color: COLORREF,
    tracking: i32,
    dpi: u32,
    flags: u32,
) {
    let previous = unsafe { SelectObject(hdc, font as _) };
    unsafe {
        SetTextColor(hdc, color);
        // Letter spacing is what makes the uppercase runs read as labels rather than shouting.
        SetTextCharacterExtra(hdc, scale(tracking, dpi));
    }

    let mut area = *rect;
    let mut flags = flags;
    if tracking != 0 && flags & DT_CENTER != 0 {
        // GDI centres a run by a width it measures without counting the character spacing it
        // then draws with, so the text lands right of centre by half of everything the spacing
        // adds — which grows with the length of the label. Measuring the run and placing it by
        // hand is the only way to get it actually centred.
        area.left =
            rect.left + ((rect.right - rect.left) - spaced_width(hdc, content, dpi, tracking)) / 2;
        flags &= !DT_CENTER;
    }
    unsafe { DrawTextW(hdc, content.as_ptr(), -1, &mut area, flags) };

    unsafe {
        SetTextCharacterExtra(hdc, 0);
        SelectObject(hdc, previous);
    }
}

/// How wide a single line of `content` actually comes out, spacing included.
///
/// The spacing goes after every character, so it widens the run by one gap per character. The
/// gap after the last one is trailing air rather than ink, which is why it is taken off again.
fn spaced_width(hdc: HDC, content: &[u16], dpi: u32, tracking: i32) -> i32 {
    let extra = scale(tracking, dpi);
    let characters = content.len().saturating_sub(1) as i32;

    let mut measured = RECT::default();
    unsafe {
        SetTextCharacterExtra(hdc, 0);
        DrawTextW(
            hdc,
            content.as_ptr(),
            -1,
            &mut measured,
            DT_SINGLELINE | DT_NOPREFIX | DT_CALCRECT,
        );
        SetTextCharacterExtra(hdc, extra);
    }
    (measured.right - measured.left) + extra * (characters - 1).max(0)
}

/// Draws several lines centred in a box. DT_VCENTER only works on one line, so the block is
/// measured first and its top edge moved down by half of what is left over.
pub fn centred_block(
    hdc: HDC,
    rect: &RECT,
    content: &[u16],
    font: HFONT,
    color: COLORREF,
    dpi: u32,
) {
    let previous = unsafe { SelectObject(hdc, font as _) };
    let mut measured = *rect;
    unsafe {
        DrawTextW(
            hdc,
            content.as_ptr(),
            -1,
            &mut measured,
            DT_CENTER | DT_NOPREFIX | DT_CALCRECT,
        );
        SelectObject(hdc, previous);
    }

    let slack = (rect.bottom - rect.top) - (measured.bottom - measured.top);
    let area = RECT {
        top: rect.top + slack.max(0) / 2,
        ..*rect
    };
    draw_text(
        hdc,
        &area,
        content,
        font,
        color,
        0,
        dpi,
        DT_CENTER | DT_NOPREFIX,
    );
}

pub fn contains(rect: &RECT, point: POINT) -> bool {
    point.x >= rect.left && point.x < rect.right && point.y >= rect.top && point.y < rect.bottom
}

/// The cursor position carried by a mouse message.
pub fn point_of(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam & 0xffff) as i16 as i32,
        y: ((lparam >> 16) & 0xffff) as i16 as i32,
    }
}

pub fn inset(rect: &RECT, horizontal: i32, vertical: i32) -> RECT {
    RECT {
        left: rect.left + horizontal,
        top: rect.top + vertical,
        right: rect.right - horizontal,
        bottom: rect.bottom - vertical,
    }
}
