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
