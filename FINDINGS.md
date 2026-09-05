# Findings

Things that are wrong, or feel wrong, in the running app — noticed while building something else
or just using it, and left alone at the time. Drop things here as you notice them; when there's a
moment, fix them together.

## The notification

- It reads fine on one screen and looked oversized on another, and only the second half of that
  was dealt with: the whole design came down to 80% (`DESIGN_PERCENT` in `src/ui.rs`). Whether
  the difference between the two screens was ever really about their DPI was never answered, so
  it may yet turn out that one number for every monitor is the wrong shape of fix.
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

## Leftovers

- The console control handler in `capture.rs` guards against the synthetic Ctrl+C killing the
  process through its own console. There is no console any more, so it now guards nothing.
