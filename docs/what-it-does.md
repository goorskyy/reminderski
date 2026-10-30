# What Reminderski does on a machine

For whoever decides whether it may run on one. Everything it touches is listed here; anything not
listed, it does not do. The source is in this repository, and every claim below can be checked
against it.

## In short

- One executable, under 400 KB. No installer, no service, no driver, no administrator rights.
- No outgoing network connections, ever: no telemetry, no update check, no name lookups.
- One optional listening socket, on 127.0.0.1 only, which can be turned off.
- No keyboard or mouse hooks. Two registered shortcuts, and one synthetic Ctrl+C when the user
  presses the capture shortcut.
- Its files live in one folder under the user's own profile, in plain text.

## The executable

Built from this repository on GitHub's hosted Windows runners when a version is tagged, for x64
and ARM64. Each release carries a `checksums.txt` with the SHA-256 of every executable, so a
copy can be matched to the release it came from, or allowed by its hash.

It is written in Rust, and its only dependencies are the Rust standard library and
[`windows-sys`](https://crates.io/crates/windows-sys), Microsoft's own bindings to the Windows
API (with `windows-link`, which they use to link against Windows' own DLLs). The C runtime is
linked in, so nothing else needs to be installed.

It runs as the user who starts it, as one ordinary process, and asks for nothing more. It can be
built from source with `cargo build --release`; see [building.md](building.md).

## Keyboard

Two global shortcuts are registered with `RegisterHotKey`: one opens the form (Ctrl+Alt+R unless
changed) and one brings a notification to the front (Ctrl+Alt+A). Windows tells the application
when one of those two combinations is pressed, and about nothing else. There is no keyboard
hook, no mouse hook, and no reading of what is typed into other applications.

The shortcuts can be changed in `settings.txt`, and each must include a modifier, so that no
single key is ever taken away from other applications.

## Synthetic input

When, and only when, the capture shortcut is pressed, Reminderski sends keystrokes with
`SendInput` to whichever window is in front:

1. the release of any of Alt, Shift and the Windows key still held down from the shortcut, so the
   application in front does not see Ctrl+Alt+C;
2. Ctrl+C.

That asks the application in front to copy its own selection, which is the only way to read a
selection on Windows without hooking into other processes. Nothing else is ever sent. Windows
does not let it reach an application running as administrator, which then simply gives a blank
form.

## Clipboard

Copying overwrites the clipboard, so on the capture shortcut, around that Ctrl+C:

1. Everything on the clipboard, in every format that is held as plain memory, is copied into
   Reminderski's own memory.
2. It waits up to 600 ms for the clipboard to change.
3. If it did, the text on it is read and put in the form.
4. What was saved in step 1 is put back.

The clipboard is not read at any other time, and not watched. The saved copy is only ever in
memory and is dropped straight after. The captured text is stored only if the user saves the
reminder it was put in.

## Files

All in `%APPDATA%\Reminderski`, the user's own roaming application data:

| File | What is in it |
|---|---|
| `reminders.txt` | Every reminder, one per line, as typed or captured, including finished ones and ones marked deleted. Plain text. |
| `reminders.tmp` | The next version of `reminders.txt`, for the moment it is being written, then renamed over it. |
| `settings.txt` | The two shortcuts and whether the dashboard is served. Written on first run. |
| `reminderski.log` | Problems only, with the time: a file that could not be written, a shortcut another application already has. When a notification cannot be shown, the text of that reminder is written here too, so it is not lost. |

Nothing is encrypted. On a domain with roaming profiles, `%APPDATA%` is copied to the profile
server with the rest of the profile, so reminders made from work text stay on the organisation's
own systems. Nothing is written anywhere else.

## Registry

One value, and only when the user ticks *Start with Windows* on the tray menu:

```
HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run
    Reminderski = "<path to the executable>"
```

It is the same list Task Manager shows under Startup, and can be turned off there. If the
executable is moved, the value is updated to the new path the next time it starts. Unticking
removes the value. Nothing else in the registry is read or written.

## Network

No outgoing connections, of any kind, at any time.

The only socket is the dashboard: a page listing the reminders, served on `127.0.0.1:7654`, or on
a free port chosen by Windows if that one is taken. It listens on the loopback address only, so
no other machine can reach it. Windows Firewall does not ask about loopback listeners.

Browsers let any web page send requests to 127.0.0.1, so the dashboard answers only its own
page:

- every request must name 127.0.0.1 or localhost, with the dashboard's port, as its host, which
  turns away a site that points its own name at 127.0.0.1;
- every change must carry an `X-Reminderski` header, which a page from another site cannot add
  without the server's permission, and the dashboard never gives it.

To have no socket at all, set this in `settings.txt` and restart Reminderski:

```
dashboard = off
```

The networking code is still in the executable then, but nothing is listening, which
`netstat -ano` shows.

## What it opens

Only when picked from its tray menu, and through `ShellExecute`, so with whatever the user has
set as their default:

- *Open dashboard* opens the dashboard's address in the default browser;
- *Settings* opens `settings.txt` in the default editor.

## Windows of its own

An icon in the notification area, and the balloons Windows shows over it. The form, which opens
on the capture shortcut and takes the focus because it was asked for. Notifications, which stay
on top of other windows but do not take the focus, so a reminder coming due cannot swallow what
is being typed.

## What it does not do

- Install anything, run as a service, load a driver, or ask for administrator rights.
- Hook the keyboard or the mouse, inject code into other processes, or read other applications'
  windows.
- Take screenshots or record the screen.
- Connect to anything, update itself, or report anything anywhere.
- Read the clipboard other than on the capture shortcut.

## Checking it

- **Files and registry:** Process Monitor from Sysinternals, filtered on `reminderski.exe`.
- **Network:** `netstat -ano`, filtered on its process ID, shows at most one listening socket on
  127.0.0.1, and none with `dashboard = off`.
- **The executable:** compare its SHA-256 with the release's `checksums.txt`, or build it from
  this repository.
