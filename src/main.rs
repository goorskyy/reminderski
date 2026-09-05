// No console: the application lives in the notification area, and anything that goes wrong is
// written to the log beside the reminder file.
#![windows_subsystem = "windows"]

mod autostart;
mod capture;
mod clipboard;
mod dashboard;
mod form;
mod log;
mod notify;
mod persona;
mod reminder;
mod store;
mod tray;
mod ui;

use std::io;
use std::mem;
use std::ptr;
use std::sync::{Arc, Mutex};

use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
    PeekMessageW, QS_ALLINPUT, SetForegroundWindow, TranslateMessage, WM_HOTKEY, WM_QUIT,
};

use crate::notify::{Notification, Outcome};
use crate::reminder::{Reminder, State};
use crate::store::Store;
use crate::tray::Tray;

const CAPTURE_HOTKEY: i32 = 1;
const ANSWER_HOTKEY: i32 = 2;
const VK_R: u32 = 0x52;
const VK_A: u32 = 0x41;

/// Wait forever, when there is nothing due to wait for.
const INFINITE: u32 = u32::MAX;

/// Never wait longer than this with a reminder pending. Waiting is measured in ticks, which do
/// not advance while the machine sleeps, so a reminder that came due during sleep is delivered
/// within a minute of waking rather than at the moment the wait was originally set to end. It
/// also covers the user moving the system clock.
const LONGEST_WAIT: u32 = 60_000;

/// The store is shared with the dashboard, which serves the page from a thread of its own. The
/// lock is taken for a few microseconds at a time and never while a window is open, so a browser
/// asking for the page cannot make the shortcut wait.
struct App {
    store: Arc<Mutex<Store>>,
    showing: Vec<Notification>,
}

fn main() -> io::Result<()> {
    capture::ignore_self_inflicted_ctrl_c();

    let path = store::default_path()?;
    log::write_to(path.with_file_name("reminderski.log"));
    let mut app = App {
        store: Arc::new(Mutex::new(Store::open(path)?)),
        showing: Vec::new(),
    };

    // The dashboard wakes this thread after changing a reminder, since the loop may otherwise
    // be asleep with nothing due for an hour.
    let address = match dashboard::start(app.store.clone(), unsafe { GetCurrentThreadId() }) {
        Ok(address) => Some(address),
        Err(error) => {
            // A dashboard that will not start is no reason to refuse to remind anybody.
            log::problem(&format!("the dashboard could not start: {error}"));
            None
        }
    };

    // A new release is a new download, which may not have landed where the last one did.
    autostart::follow_the_executable();

    // Held until the loop ends, which is what puts the icon away again.
    let _tray = Tray::show(address)?;

    // Without the capture shortcut there is no application, so failing to take it is fatal.
    register_hotkey(CAPTURE_HOTKEY, VK_R)?;
    // Answering one is a convenience by comparison. If something else already owns the
    // combination, say so in the log and carry on: the mouse still works.
    if let Err(error) = register_hotkey(ANSWER_HOTKEY, VK_A) {
        log::problem(&format!("the answer shortcut is not available: {error}"));
    }

    app.run();

    unsafe {
        UnregisterHotKey(ptr::null_mut(), CAPTURE_HOTKEY);
        UnregisterHotKey(ptr::null_mut(), ANSWER_HOTKEY);
    }
    Ok(())
}

/// A null window handle posts WM_HOTKEY to this thread's message queue, so no window is needed
/// to receive it.
fn register_hotkey(id: i32, key: u32) -> io::Result<()> {
    let registered = unsafe {
        RegisterHotKey(
            ptr::null_mut(),
            id,
            MOD_CONTROL | MOD_ALT | MOD_NOREPEAT,
            key,
        )
    };
    if registered == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

impl App {
    /// Waits for either a message or the next reminder falling due, whichever comes first.
    fn run(&mut self) {
        loop {
            // MWMO_INPUTAVAILABLE is what stops a message that arrived between the drain below
            // and this wait from being slept through.
            unsafe {
                MsgWaitForMultipleObjectsEx(
                    0,
                    ptr::null(),
                    self.wait_milliseconds(),
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                )
            };

            if !self.drain_messages() {
                return;
            }
            self.collect_answered();
            self.deliver_due();
        }
    }

    /// Handles every queued message. Returns false when the application should stop.
    fn drain_messages(&mut self) -> bool {
        let mut message: MSG = unsafe { mem::zeroed() };
        while unsafe { PeekMessageW(&mut message, ptr::null_mut(), 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_QUIT {
                return false;
            }
            if message.message == WM_HOTKEY {
                match message.wParam as i32 {
                    CAPTURE_HOTKEY => self.on_capture_hotkey(),
                    ANSWER_HOTKEY => self.reach_a_notification(),
                    _ => {}
                }
                continue;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        true
    }

    /// How long the loop may sleep before it must look at the reminders again.
    fn wait_milliseconds(&self) -> u32 {
        let now = reminder::now_unix();
        let store = self.reminders();
        let next_due = store
            .reminders
            .iter()
            .enumerate()
            .filter(|(index, reminder)| {
                reminder.state == State::Pending && !self.is_showing(*index)
            })
            .map(|(_, reminder)| reminder.due_unix)
            .min();

        match next_due {
            None => INFINITE,
            Some(due) if due <= now => 0,
            Some(due) => u32::try_from((due - now) * 1000)
                .unwrap_or(LONGEST_WAIT)
                .min(LONGEST_WAIT),
        }
    }

    /// Opens a notification for every reminder that has come due, including ones that fell due
    /// while the application was not running.
    fn deliver_due(&mut self) {
        let now = reminder::now_unix();
        // Read out what is needed and let go of the lock before opening any window.
        let due: Vec<(usize, String, u32)> = {
            let store = self.reminders();
            store
                .reminders
                .iter()
                .enumerate()
                .filter(|(index, reminder)| reminder.is_due(now) && !self.is_showing(*index))
                .map(|(index, reminder)| (index, reminder.text.clone(), reminder.snoozes))
                .collect()
        };

        for (index, text, snoozes) in due {
            let slot = self.free_slot();
            match Notification::show(&text, snoozes, index, slot) {
                Ok(notification) => self.showing.push(notification),
                // Without a window there is no way to tell the user, and re-trying every second
                // would be worse than saying so once and leaving the reminder pending.
                Err(error) => {
                    log::problem(&format!("could not show the reminder: {error}\n{text}"));
                }
            }
        }
    }

    /// Applies the outcome of every notification the user has closed.
    fn collect_answered(&mut self) {
        let mut answered = Vec::new();
        let mut index = 0;
        while index < self.showing.len() {
            if !self.showing[index].finished() {
                index += 1;
                continue;
            }
            let notification = self.showing.remove(index);
            answered.push((notification.reminder, notification.outcome()));
        }
        if answered.is_empty() {
            return;
        }

        let mut store = self
            .store
            .lock()
            .expect("the reminders are no longer readable");
        for (index, outcome) in answered {
            let reminder = &mut store.reminders[index];
            match outcome {
                Outcome::Done => reminder.state = State::Done,
                Outcome::Later(delay) => {
                    reminder.due_unix = reminder::now_unix() + delay.as_secs();
                    reminder.snoozes += 1;
                }
            }
        }

        if let Err(error) = store.save() {
            log::problem(&format!(
                "could not record the answered reminder(s): {error}"
            ));
        }
    }

    /// The reminders, for as long as the returned guard lives. Nothing that opens a window or
    /// waits on anything may be done while holding it.
    fn reminders(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store
            .lock()
            .expect("the reminders are no longer readable")
    }

    fn is_showing(&self, reminder: usize) -> bool {
        self.showing.iter().any(|shown| shown.reminder == reminder)
    }

    /// The lowest corner position no open notification is using.
    fn free_slot(&self) -> usize {
        (0..)
            .find(|slot| !self.showing.iter().any(|shown| shown.slot == *slot))
            .unwrap_or(0)
    }

    /// Puts the notification nearest the corner in the foreground, which is the only way the
    /// keyboard can reach it.
    ///
    /// A notification deliberately does not take the foreground when it appears, so that a
    /// reminder falling due mid-sentence does not swallow what is being typed. The cost of that
    /// is that its keys — Enter, Escape and the snoozes — cannot arrive either, because Windows
    /// delivers them to whichever window has the focus. This is the one place the user asks for
    /// it, so this is the one place it is taken.
    ///
    /// The lowest slot is the one at the bottom of the stack, nearest where the eye already is.
    /// Answering it closes it, so pressing the key again reaches the next one up.
    fn reach_a_notification(&mut self) {
        if let Some(notification) = self.showing.iter().min_by_key(|shown| shown.slot) {
            unsafe { SetForegroundWindow(notification.window()) };
        }
    }

    fn on_capture_hotkey(&mut self) {
        // A failed capture still opens the form, blank. Losing the user's keystroke because
        // their clipboard misbehaved would be worse than starting from an empty box.
        let captured = capture::capture_selection().unwrap_or_else(|error| {
            log::problem(&format!("capture failed: {error}"));
            None
        });

        match form::show(captured.as_deref().unwrap_or_default()) {
            Ok(Some(entry)) => {
                // The form has closed, so the lock is only taken now and let go immediately.
                let mut store = self
                    .store
                    .lock()
                    .expect("the reminders are no longer writable");
                store
                    .reminders
                    .push(Reminder::due_in(entry.delay, entry.text));
                if let Err(error) = store.save() {
                    log::problem(&format!("could not store the reminder: {error}"));
                }
            }
            Ok(None) => {}
            Err(error) => log::problem(&format!("could not open the form: {error}")),
        }
    }
}
