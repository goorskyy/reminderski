mod capture;
mod clipboard;
mod form;

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
    capture::ignore_self_inflicted_ctrl_c();

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
            on_hotkey();
        }
    }
}

fn on_hotkey() {
    // A failed capture still opens the form, blank. Losing the user's keystroke because their
    // clipboard misbehaved would be worse than starting from an empty box.
    let captured = capture::capture_selection().unwrap_or_else(|error| {
        eprintln!("capture failed: {error}");
        None
    });

    match form::show(captured.as_deref().unwrap_or_default()) {
        // M3 turns this into a stored reminder.
        Ok(Some(text)) => println!("--- submitted ---\n{text}\n---"),
        Ok(None) => println!("--- cancelled ---"),
        Err(error) => eprintln!("could not open the form: {error}"),
    }
}
