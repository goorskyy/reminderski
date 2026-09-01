//! Where anything that goes wrong is written, now that there is no console to write it to.
//!
//! Only problems are recorded. A log of everything that went right would be noise nobody reads,
//! and the reminder file already says what was stored.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::reminder::local_now;

static FILE: OnceLock<PathBuf> = OnceLock::new();

/// Names the file to write to. Anything reported before this is lost, which only covers the
/// moments before the store's location is known.
pub fn write_to(path: PathBuf) {
    let _ = FILE.set(path);
}

pub fn problem(what: &str) {
    let Some(path) = FILE.get() else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };

    let now = local_now();
    // Failing to write about a failure leaves nothing sensible to do about it.
    let _ = writeln!(
        file,
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}  {what}",
        now.wYear, now.wMonth, now.wDay, now.wHour, now.wMinute, now.wSecond
    );
}
