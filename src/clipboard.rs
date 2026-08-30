//! Clipboard reading, plus save and restore of the user's clipboard contents.
//!
//! Capturing a selection works by making the focused application copy, which necessarily
//! overwrites the clipboard. Everything the user had there is saved first and put back
//! afterwards, so the capture is invisible to them.

use std::io;
use std::ptr;
use std::thread;
use std::time::Duration;

use windows_sys::Win32::Foundation::GlobalFree;
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};

const CF_UNICODETEXT: u32 = 13;

/// Formats whose clipboard handle is not memory allocated by `GlobalAlloc`, so it cannot be
/// copied byte-for-byte. Bitmaps, metafiles and palettes are GDI objects, and `CF_OWNERDISPLAY`
/// is rendered by the owning window on demand.
const NON_GLOBAL_FORMATS: [u32; 8] = [
    2,    // CF_BITMAP
    3,    // CF_METAFILEPICT
    9,    // CF_PALETTE
    14,   // CF_ENHMETAFILE
    0x80, // CF_OWNERDISPLAY
    0x82, // CF_DSPBITMAP
    0x83, // CF_DSPMETAFILEPICT
    0x8E, // CF_DSPENHMETAFILE
];

/// Only one process may hold the clipboard open at a time, and Explorer or a clipboard
/// manager may hold it for a few milliseconds. Retry rather than dropping the capture.
const OPEN_ATTEMPTS: u32 = 10;
const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

/// Everything that was on the clipboard, in every format that can be restored.
pub struct Snapshot {
    formats: Vec<(u32, Vec<u8>)>,
}

/// Holds the clipboard open and closes it on drop, including on early return.
struct ClipboardLock;

impl ClipboardLock {
    fn acquire() -> io::Result<Self> {
        for _ in 0..OPEN_ATTEMPTS {
            if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                return Ok(Self);
            }
            thread::sleep(OPEN_RETRY_DELAY);
        }
        Err(io::Error::last_os_error())
    }
}

impl Drop for ClipboardLock {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

/// Changes whenever any process writes to the clipboard. Cheap, and does not require the
/// clipboard to be open.
pub fn sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Reads the clipboard as Unicode text, or `None` when it holds no text.
pub fn read_text() -> io::Result<Option<String>> {
    let _clipboard = ClipboardLock::acquire()?;

    let handle = unsafe { GetClipboardData(CF_UNICODETEXT) };
    if handle.is_null() {
        return Ok(None);
    }
    let Some(bytes) = (unsafe { read_global(handle) }) else {
        return Ok(None);
    };

    let (pairs, _) = bytes.as_chunks::<2>();
    let units: Vec<u16> = pairs
        .iter()
        .map(|pair| u16::from_ne_bytes(*pair))
        .take_while(|&unit| unit != 0)
        .collect();
    Ok(Some(String::from_utf16_lossy(&units)))
}

/// Copies out every restorable format currently on the clipboard.
pub fn snapshot() -> io::Result<Snapshot> {
    let _clipboard = ClipboardLock::acquire()?;

    let mut formats = Vec::new();
    let mut format = 0;
    loop {
        // Zero means the enumeration finished. It also means failure, but the two are only
        // distinguishable via GetLastError, and either way there is nothing more to read.
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            break;
        }
        if NON_GLOBAL_FORMATS.contains(&format) {
            continue;
        }
        let handle = unsafe { GetClipboardData(format) };
        if handle.is_null() {
            // The owning application failed to render this format on demand. Skipping it
            // loses that one format rather than the whole clipboard.
            continue;
        }
        if let Some(bytes) = unsafe { read_global(handle) } {
            formats.push((format, bytes));
        }
    }
    Ok(Snapshot { formats })
}

/// Puts a snapshot back, replacing whatever the clipboard holds now.
pub fn restore(snapshot: &Snapshot) -> io::Result<()> {
    let _clipboard = ClipboardLock::acquire()?;

    if unsafe { EmptyClipboard() } == 0 {
        return Err(io::Error::last_os_error());
    }
    for (format, bytes) in &snapshot.formats {
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let destination = unsafe { GlobalLock(handle) };
        if destination.is_null() {
            unsafe { GlobalFree(handle) };
            return Err(io::Error::last_os_error());
        }
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), destination.cast::<u8>(), bytes.len());
            GlobalUnlock(handle);
        }
        if unsafe { SetClipboardData(*format, handle) }.is_null() {
            // Ownership only transfers to the system on success, so this block still owns it.
            unsafe { GlobalFree(handle) };
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Copies the bytes behind a `GlobalAlloc`-backed clipboard handle.
///
/// # Safety
///
/// `handle` must be a clipboard handle for a format backed by movable global memory, obtained
/// while the clipboard is open.
unsafe fn read_global(handle: *mut std::ffi::c_void) -> Option<Vec<u8>> {
    let size = unsafe { GlobalSize(handle) };
    if size == 0 {
        return None;
    }
    let source = unsafe { GlobalLock(handle) };
    if source.is_null() {
        return None;
    }
    let bytes = unsafe { std::slice::from_raw_parts(source.cast::<u8>(), size) }.to_vec();
    unsafe { GlobalUnlock(handle) };
    Some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// The clipboard is process-wide, so these tests cannot run concurrently.
    static CLIPBOARD_LOCK: Mutex<()> = Mutex::new(());

    fn serialized() -> MutexGuard<'static, ()> {
        CLIPBOARD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn set_text(text: &str) {
        let mut units: Vec<u16> = text.encode_utf16().collect();
        units.push(0);
        let bytes = units.iter().flat_map(|unit| unit.to_ne_bytes()).collect();
        restore(&Snapshot {
            formats: vec![(CF_UNICODETEXT, bytes)],
        })
        .expect("failed to write test text to the clipboard");
    }

    #[test]
    fn reads_back_multiline_unicode_text() {
        let _guard = serialized();
        let original = snapshot().expect("snapshot failed");

        let text = "Zażółć gęślą jaźń\r\nsecond line\r\n🎉 non-BMP";
        set_text(text);
        assert_eq!(read_text().expect("read failed").as_deref(), Some(text));

        restore(&original).expect("restore failed");
    }

    #[test]
    fn restore_puts_back_replaced_contents() {
        let _guard = serialized();
        let original = snapshot().expect("snapshot failed");

        set_text("what the user had");
        let saved = snapshot().expect("snapshot failed");

        set_text("what copying overwrote it with");
        restore(&saved).expect("restore failed");

        assert_eq!(
            read_text().expect("read failed").as_deref(),
            Some("what the user had")
        );

        restore(&original).expect("restore failed");
    }

    #[test]
    fn sequence_number_changes_when_the_clipboard_is_written() {
        let _guard = serialized();
        let original = snapshot().expect("snapshot failed");

        let before = sequence_number();
        set_text("anything at all");
        assert_ne!(before, sequence_number());

        restore(&original).expect("restore failed");
    }
}
