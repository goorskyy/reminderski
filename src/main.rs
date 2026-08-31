mod capture;
mod clipboard;
mod form;
mod reminder;
mod store;

use std::io;
use std::mem;
use std::path::Path;
use std::ptr;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

use crate::reminder::Reminder;

const HOTKEY_ID: i32 = 1;
const VK_R: u32 = 0x52;

fn main() -> io::Result<()> {
    capture::ignore_self_inflicted_ctrl_c();

    let store_path = store::default_path()?;
    let stored = store::load(&store_path)?;

    // A null window handle posts WM_HOTKEY to this thread's message queue, so no window is
    // needed to receive it.
    if unsafe {
        RegisterHotKey(
            ptr::null_mut(),
            HOTKEY_ID,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            VK_R,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }

    println!("Reminderski: press Ctrl+Alt+R to capture the selected text. Ctrl+C here to quit.");
    println!("{} reminder(s) in {}", stored.len(), store_path.display());
    run_message_loop(&store_path);

    unsafe { UnregisterHotKey(ptr::null_mut(), HOTKEY_ID) };
    Ok(())
}

fn run_message_loop(store_path: &Path) {
    let mut message: MSG = unsafe { mem::zeroed() };
    loop {
        // Returns 0 on WM_QUIT and -1 on error; neither leaves anything worth continuing for.
        if unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } <= 0 {
            return;
        }
        if message.message == WM_HOTKEY {
            on_hotkey(store_path);
        }
    }
}

fn on_hotkey(store_path: &Path) {
    // A failed capture still opens the form, blank. Losing the user's keystroke because their
    // clipboard misbehaved would be worse than starting from an empty box.
    let captured = capture::capture_selection().unwrap_or_else(|error| {
        eprintln!("capture failed: {error}");
        None
    });

    match form::show(captured.as_deref().unwrap_or_default()) {
        Ok(Some(entry)) => {
            let reminder = Reminder::due_in(entry.delay, entry.text);
            match store::append(store_path, &reminder) {
                // M4 turns a stored reminder into a notification at its due time.
                Ok(()) => println!(
                    "--- stored, due in {}s ---\n{}\n---",
                    entry.delay.as_secs(),
                    reminder.text
                ),
                Err(error) => eprintln!("could not store the reminder: {error}"),
            }
        }
        Ok(None) => println!("--- cancelled ---"),
        Err(error) => eprintln!("could not open the form: {error}"),
    }
}
