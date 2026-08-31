//! Capturing the selected text out of whatever application currently has focus.
//!
//! There is no Windows API that reads another application's selection. The portable way is to
//! ask the focused application to run its own Copy command by synthesising Ctrl+C, then read
//! what landed on the clipboard. Applications that do not implement Copy, or that Windows
//! shields from synthetic input, cannot be captured this way.

use std::io;
use std::mem;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::System::Console::{CTRL_C_EVENT, SetConsoleCtrlHandler};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows_sys::core::BOOL;

use crate::clipboard;

const VK_C: VIRTUAL_KEY = 0x43;

/// Set while a synthetic Ctrl+C is on its way to the focused application.
static COPY_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Stops our own synthetic Ctrl+C from terminating this process.
///
/// Ctrl+C typed into a console is not an ordinary keystroke: Windows turns it into a
/// CTRL_C_EVENT delivered to every process attached to that console. When the user triggers a
/// capture while the console this application runs in has the focus, that includes us, and the
/// default handler exits immediately with STATUS_CONTROL_C_EXIT — no window, no message.
///
/// Only the interrupt we caused ourselves is swallowed, so Ctrl+C typed by the user still quits.
pub fn ignore_self_inflicted_ctrl_c() {
    // Failure only means the interrupt keeps its default handling, which is what we have today.
    unsafe { SetConsoleCtrlHandler(Some(on_console_ctrl), 1) };
}

unsafe extern "system" fn on_console_ctrl(event: u32) -> BOOL {
    let ours = event == CTRL_C_EVENT && COPY_IN_FLIGHT.load(Ordering::SeqCst);
    // Non-zero means handled; zero passes the event to the default handler, which exits.
    ours as BOOL
}

/// How long to wait for the focused application to respond to the synthetic Copy. Applications
/// answer in a few milliseconds; heavier ones such as Office are slower, and a selection may be
/// large. Waiting longer costs nothing when text does arrive, because the wait ends as soon as
/// the clipboard changes.
const COPY_TIMEOUT: Duration = Duration::from_millis(600);
const COPY_POLL_INTERVAL: Duration = Duration::from_millis(5);

/// Give the focused application a moment to observe the modifier releases before Ctrl+C
/// arrives. Without this, applications that inspect the keyboard state rather than the key
/// events still see the hotkey's Alt held down.
const MODIFIER_SETTLE_DELAY: Duration = Duration::from_millis(20);

/// Copies the focused application's selection, leaving the user's clipboard as it was.
///
/// Returns `None` when nothing could be captured: no selection, or an application that does not
/// support Copy. That is an ordinary outcome, not an error — the caller opens a blank form.
pub fn capture_selection() -> io::Result<Option<String>> {
    let saved = clipboard::snapshot()?;
    let before = clipboard::sequence_number();

    COPY_IN_FLIGHT.store(true, Ordering::SeqCst);
    send_copy();
    let copied = wait_for_clipboard_change(before);
    COPY_IN_FLIGHT.store(false, Ordering::SeqCst);

    let text = if copied {
        clipboard::read_text()?
    } else {
        None
    };

    // Restore whatever the user had, on both paths. Their clipboard is not ours to spend.
    clipboard::restore(&saved)?;

    Ok(text.filter(|text| !text.is_empty()))
}

/// Sends Ctrl+C to the focused application.
fn send_copy() {
    // The user is still physically holding Ctrl+Alt+R when the hotkey fires. Alt in particular
    // must be released first, or the application sees Ctrl+Alt+C and does nothing.
    let held: Vec<VIRTUAL_KEY> = [VK_MENU, VK_SHIFT, VK_LWIN, VK_RWIN]
        .into_iter()
        .filter(|&key| is_down(key))
        .collect();
    let releases: Vec<INPUT> = held
        .iter()
        .map(|&key| key_event(key, KEYEVENTF_KEYUP))
        .collect();
    if !releases.is_empty() {
        send(&releases);
        thread::sleep(MODIFIER_SETTLE_DELAY);
    }

    // Ctrl is pressed explicitly rather than relying on the user still holding it, so the same
    // code works however the hotkey is eventually reconfigured.
    send(&[
        key_event(VK_CONTROL, 0),
        key_event(VK_C, 0),
        key_event(VK_C, KEYEVENTF_KEYUP),
        key_event(VK_CONTROL, KEYEVENTF_KEYUP),
    ]);
}

/// Waits until some application writes to the clipboard, or the timeout expires.
fn wait_for_clipboard_change(before: u32) -> bool {
    let deadline = Instant::now() + COPY_TIMEOUT;
    while Instant::now() < deadline {
        if clipboard::sequence_number() != before {
            return true;
        }
        thread::sleep(COPY_POLL_INTERVAL);
    }
    false
}

fn is_down(key: VIRTUAL_KEY) -> bool {
    // The high bit of the return value is set while the key is physically down.
    unsafe { GetAsyncKeyState(key as i32) as u16 & 0x8000 != 0 }
}

fn key_event(key: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: key,
                wScan: 0,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn send(events: &[INPUT]) {
    // A failed SendInput means the input was blocked, which surfaces as "nothing captured" a
    // few hundred milliseconds later. There is no better recovery available here.
    unsafe {
        SendInput(
            events.len() as u32,
            events.as_ptr(),
            mem::size_of::<INPUT>() as i32,
        )
    };
}
