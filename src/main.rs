// No console: the application lives in the notification area, and anything that goes wrong is
// written to the log beside the reminder file.
#![windows_subsystem = "windows"]

mod capture;
mod clipboard;
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

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, IsDialogMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
    PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage, WM_HOTKEY, WM_QUIT,
};

use crate::notify::{Notification, Outcome};
use crate::reminder::{Reminder, State};
use crate::store::Store;
use crate::tray::Tray;

const HOTKEY_ID: i32 = 1;
const VK_R: u32 = 0x52;

/// Wait forever, when there is nothing due to wait for.
const INFINITE: u32 = u32::MAX;

/// Never wait longer than this with a reminder pending. Waiting is measured in ticks, which do
/// not advance while the machine sleeps, so a reminder that came due during sleep is delivered
/// within a minute of waking rather than at the moment the wait was originally set to end. It
/// also covers the user moving the system clock.
const LONGEST_WAIT: u32 = 60_000;

struct App {
    store: Store,
    showing: Vec<Notification>,
}

fn main() -> io::Result<()> {
    capture::ignore_self_inflicted_ctrl_c();

    let path = store::default_path()?;
    log::write_to(path.with_file_name("reminderski.log"));
    let mut app = App {
        store: Store::open(path)?,
        showing: Vec::new(),
    };

    // Held until the loop ends, which is what puts the icon away again.
    let _tray = Tray::show()?;

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

    app.run();

    unsafe { UnregisterHotKey(ptr::null_mut(), HOTKEY_ID) };
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
                self.on_hotkey();
                continue;
            }
            // This is what gives the notifications Enter for the default button, Escape for
            // dismissal and Tab between the two, without subclassing anything.
            let handled = self
                .showing
                .iter()
                .any(|shown| unsafe { IsDialogMessageW(shown.window(), &message) } != 0);
            if handled {
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
        let next_due = self
            .store
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
        let due: Vec<usize> = self
            .store
            .reminders
            .iter()
            .enumerate()
            .filter(|(index, reminder)| reminder.is_due(now) && !self.is_showing(*index))
            .map(|(index, _)| index)
            .collect();

        for index in due {
            let slot = self.free_slot();
            let text = self.store.reminders[index].text.clone();
            let snoozes = self.store.reminders[index].snoozes;
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
        let mut answered = false;
        let mut index = 0;
        while index < self.showing.len() {
            if !self.showing[index].finished() {
                index += 1;
                continue;
            }
            let notification = self.showing.remove(index);
            let reminder = &mut self.store.reminders[notification.reminder];
            match notification.outcome() {
                Outcome::Done => reminder.state = State::Done,
                Outcome::Later(delay) => {
                    reminder.due_unix = reminder::now_unix() + delay.as_secs();
                    reminder.snoozes += 1;
                }
            }
            answered = true;
        }

        if answered && let Err(error) = self.store.save() {
            log::problem(&format!(
                "could not record the answered reminder(s): {error}"
            ));
        }
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

    fn on_hotkey(&mut self) {
        // A failed capture still opens the form, blank. Losing the user's keystroke because
        // their clipboard misbehaved would be worse than starting from an empty box.
        let captured = capture::capture_selection().unwrap_or_else(|error| {
            log::problem(&format!("capture failed: {error}"));
            None
        });

        match form::show(captured.as_deref().unwrap_or_default()) {
            Ok(Some(entry)) => {
                self.store
                    .reminders
                    .push(Reminder::due_in(entry.delay, entry.text));
                if let Err(error) = self.store.save() {
                    log::problem(&format!("could not store the reminder: {error}"));
                }
            }
            Ok(None) => {}
            Err(error) => log::problem(&format!("could not open the form: {error}")),
        }
    }
}
