mod capture;
mod clipboard;

use std::io;
use std::mem;
use std::ptr;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

const HOTKEY_ID: i32 = 1;
const VK_R: u32 = 0x52;

fn main() -> io::Result<()> {
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
    run_message_loop();

    unsafe { UnregisterHotKey(ptr::null_mut(), HOTKEY_ID) };
    Ok(())
}

fn run_message_loop() {
    let mut message: MSG = unsafe { mem::zeroed() };
    loop {
        // Returns 0 on WM_QUIT and -1 on error; neither leaves anything worth continuing for.
        if unsafe { GetMessageW(&mut message, ptr::null_mut(), 0, 0) } <= 0 {
            return;
        }
        if message.message == WM_HOTKEY {
            report(capture::capture_selection());
        }
    }
}

/// Prints the captured text. Stands in for the input form until the next step of M2.
fn report(captured: io::Result<Option<String>>) {
    match captured {
        Ok(Some(text)) => println!(
            "--- captured {} chars ---\n{text}\n---",
            text.chars().count()
        ),
        Ok(None) => println!("--- nothing captured; a blank form would open ---"),
        Err(error) => eprintln!("--- capture failed: {error} ---"),
    }
}
