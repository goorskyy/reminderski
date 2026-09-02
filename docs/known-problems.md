# Known problems

Things that are wrong rather than things that are missing. Each was found while building
something else and left alone at the time.

## The input form

- Pressing the shortcut again while the form is open does nothing. It should raise the form that
  is already there.
- A time it cannot read and an empty reminder produce the same beep, so there is no way to tell
  which half was wrong.
- Quitting from the tray while the form is open only closes the form. The form's message loop
  takes the quit message meant for the application.

## The notification

- Several notifications stack upwards, but the rest do not move down when one is answered.
- A reminder too long for the window is clipped rather than scrolled.
- The keyboard does not reach it until it has been clicked. It deliberately does not take the
  foreground when it appears, which also leaves it unable to hear a key.

## Time expressions

- Day names are not understood: `friday` and `next tuesday` are rejected.

## Leftovers

- The console control handler in `capture.rs` guards against the synthetic Ctrl+C killing the
  process through its own console. There is no console any more, so it now guards nothing.
