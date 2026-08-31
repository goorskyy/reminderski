# Capture compatibility

Reminderski has no way to read another application's selection directly. It releases the
modifiers the user is still holding, synthesises Ctrl+C, waits up to 600 ms for the clipboard
sequence number to change, reads `CF_UNICODETEXT`, and restores the previous clipboard. Anything
that does not answer a standard Copy therefore captures nothing, which is an ordinary outcome:
the form opens blank and ready for typing.

Verified on Windows 11 Home (26200), aarch64, on 2026-08-31.

## Results

| Surface | Result |
| --- | --- |
| Notepad | Captured, including multiline text. |
| Browser, selected page text | Captured. |
| Windows Terminal, with a selection | Captured. |
| Word / Excel | Captured. |
| Outlook compose / WordPad (rich text) | Captured as plain text. |
| Any of the above with nothing selected | Blank form, clipboard untouched. |
| Elevated window | Blank form, no error. |

Rows above were exercised by hand in the real applications. The empty-selection and elevated
cases were the ones worth confirming individually; the rest behaved the same way as each other,
so they are recorded as a group rather than as separately timed measurements.

## Unsupported by this mechanism

- **Elevated windows.** An unelevated process cannot send input to a higher-integrity window;
  Windows drops it silently. The synthetic Ctrl+C never arrives, so the form opens blank. There
  is no error to report and nothing to retry.
- **Anything with no Copy command**, such as a selection that is not text, or a control that
  refuses to copy. Indistinguishable from an empty selection, and handled the same way.
- **Formatting.** Only `CF_UNICODETEXT` is read, so rich text arrives as plain text by design.

## Known side effect

Capturing while a console window has focus sends that console a genuine Ctrl+C, which the
console turns into an interrupt for whatever is running in it. Terminals copy instead when text
is selected, so this only bites on an empty selection. No application can distinguish our
synthetic Ctrl+C from a typed one, so this is inherent to the approach. Reminderski ignores the
interrupt it causes itself (see `ignore_self_inflicted_ctrl_c` in `src/capture.rs`); other
processes sharing that console do not.
