mod capture;
mod clipboard;
mod form;
mod reminder;
mod store;

use std::io;
use std::mem;
use std::ptr;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
    WM_HOTKEY, WM_QUIT,
};

use crate::reminder::{Reminder, State};
use crate::store::Store;

const HOTKEY_ID: i32 = 1;
const VK_R: u32 = 0x52;

/// Wait forever, when there is nothing due to wait for.
const INFINITE: u32 = u32::MAX;

/// Never wait longer than this with a reminder pending. Waiting is measured in ticks, which do
/// not advance while the machine sleeps, so a reminder that came due during sleep is delivered
/// within a minute of waking rather than at the moment the wait was originally set to end. It
/// also covers the user moving the system clock.
const LONGEST_WAIT: u32 = 60_000;

fn main() -> io::Result<()> {
    capture::ignore_self_inflicted_ctrl_c();

    let mut store = Store::open(store::default_path()?)?;

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
    println!(
        "{} reminder(s) waiting in {}",
        store.pending(),
        store.path().display()
    );
    run_message_loop(&mut store);

    unsafe { UnregisterHotKey(ptr::null_mut(), HOTKEY_ID) };
    Ok(())
}

/// Waits for either a message or the next reminder falling due, whichever comes first.
fn run_message_loop(store: &mut Store) {
    loop {
        // MWMO_INPUTAVAILABLE is what stops a message that arrived between the drain below and
        // this wait from being slept through.
        unsafe {
            MsgWaitForMultipleObjectsEx(
                0,
                ptr::null(),
                wait_milliseconds(&store.reminders),
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };

        if !drain_messages(store) {
            return;
        }
        deliver_due(store);
    }
}

/// Handles every queued message. Returns false when the application should stop.
fn drain_messages(store: &mut Store) -> bool {
    let mut message: MSG = unsafe { mem::zeroed() };
    while unsafe { PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
        if message.message == WM_QUIT {
            return false;
        }
        if message.message == WM_HOTKEY {
            on_hotkey(store);
        }
    }
    true
}

/// How long the loop may sleep before it must look at the reminders again.
fn wait_milliseconds(reminders: &[Reminder]) -> u32 {
    let now = reminder::now_unix();
    let next_due = reminders
        .iter()
        .filter(|reminder| reminder.state == State::Pending)
        .map(|reminder| reminder.due_unix)
        .min();

    match next_due {
        None => INFINITE,
        Some(due) if due <= now => 0,
        Some(due) => u32::try_from((due - now) * 1000)
            .unwrap_or(LONGEST_WAIT)
            .min(LONGEST_WAIT),
    }
}

/// Announces every reminder that has come due, including ones that fell due while the
/// application was not running.
fn deliver_due(store: &mut Store) {
    let now = reminder::now_unix();
    let mut delivered = false;

    for reminder in &mut store.reminders {
        if !reminder.is_due(now) {
            continue;
        }
        // The notification window with its persona, Done and Snooze replaces this next.
        println!("--- due ---\n{}\n---", reminder.text);
        reminder.state = State::Done;
        delivered = true;
    }

    if delivered && let Err(error) = store.save() {
        eprintln!("could not record the delivered reminder(s): {error}");
    }
}

fn on_hotkey(store: &mut Store) {
    // A failed capture still opens the form, blank. Losing the user's keystroke because their
    // clipboard misbehaved would be worse than starting from an empty box.
    let captured = capture::capture_selection().unwrap_or_else(|error| {
        eprintln!("capture failed: {error}");
        None
    });

    match form::show(captured.as_deref().unwrap_or_default()) {
        Ok(Some(entry)) => {
            let summary = format!(
                "--- stored, due in {}s ---\n{}\n---",
                entry.delay.as_secs(),
                entry.text
            );
            store
                .reminders
                .push(Reminder::due_in(entry.delay, entry.text));
            match store.save() {
                Ok(()) => println!("{summary}"),
                Err(error) => eprintln!("could not store the reminder: {error}"),
            }
        }
        Ok(None) => println!("--- cancelled ---"),
        Err(error) => eprintln!("could not open the form: {error}"),
    }
}
