# Findings

Things that are wrong, or feel wrong, in the running app — noticed while building something else
or just using it, and left alone at the time. Drop things here as you notice them; when there's a
moment, fix them together.

## The notification

- The window (`src/notify.rs`, `WIDTH`/`HEIGHT` = 520x284 at 96 DPI) feels big compared to the
  original .NET Reminderski. Reads fine on one screen, looks oversized on another. Need to check:
  - Is it the raw size, or DPI scaling (`scale(...)` in `notify.rs`) making it bigger on some
    monitors than others?
  - Compare side-by-side with the .NET version's actual window size.
  - Whether it can shrink safely without clipping the persona note, the label, or the
    snooze/done buttons.
- Several notifications stack upwards, but the rest do not move down when one is answered.
- A reminder too long for the window is clipped rather than scrolled.
- Nothing on the window says that Ctrl+Alt+A reaches it, or that Enter and 1, 2, 3 answer it
  once it has been reached.

## The input form

- Pressing the shortcut again while the form is open does nothing. It should raise the form that
  is already there.
- A time it cannot read and an empty reminder produce the same beep, so there is no way to tell
  which half was wrong.
- Quitting from the tray while the form is open only closes the form. The form's message loop
  takes the quit message meant for the application.

## Time expressions

- Day names are not understood: `friday` and `next tuesday` are rejected.

## Leftovers

- The console control handler in `capture.rs` guards against the synthetic Ctrl+C killing the
  process through its own console. There is no console any more, so it now guards nothing.
