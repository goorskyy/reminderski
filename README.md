# Reminderski

A tiny, fast, local reminder app inspired by Slack's reminder workflow.

The core interaction is:

Select text → global shortcut → enter `in 40m` → Enter → forget about it.

When the reminder is due, a minimal notification appears with quick actions such as snooze or done.

The initial target is Windows.

The application is intentionally small, monochrome, keyboard-first, and local.

## Installing

Download `reminderski-x86_64-pc-windows-msvc.exe` from the
[latest release](https://github.com/goorskyy/reminderski/releases/latest) and run it. There is
nothing to install and nothing else to download: the executable is self-contained.

On an ARM64 machine take `reminderski-aarch64-pc-windows-msvc.exe` instead. The x64 one also
runs there, emulated.

Running it puts an `R` in the notification area and nothing else on screen. Leave it there and
press Ctrl+Alt+R whenever you want to set a reminder. To stop it, right-click the icon and
choose Quit.

It does not start with Windows, so run it again after a restart.

Releases are cut by tagging: pushing a tag such as `v0.0.1` builds that commit and attaches the
executables to a GitHub release of the same name.

## Building

Requires a Rust MSVC toolchain and Visual Studio Build Tools with the *Desktop development
with C++* workload, which provides the linker.

The workload's default components cover x64 and x86 only. On an ARM64 machine also add
`Microsoft.VisualStudio.Component.VC.Tools.ARM64`, or `cargo build` fails with
`linker link.exe not found`.

```
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets
```

## Time expressions

Relative: `in 40m`, `40m`, `1h30m`, `2 days 3 hours`. The leading `in` is optional.

Absolute: `at 15:00`, `at 3pm`, `at 3:30 pm`, `at 9`, `today at 18:00`, `tomorrow`,
`tomorrow at 9:30`. A clock time that has already passed means tomorrow, and a bare `tomorrow`
means 09:00.

## Data

Reminders are stored in `%APPDATA%\Reminderski\reminders.txt`, one per line.

Anything that goes wrong is written to `%APPDATA%\Reminderski\reminderski.log`. No file means
nothing has gone wrong.

## Project status

Early development.

The current execution plan is defined in `MILESTONE.md`.

Future ideas are kept in `FEATURES.md`.
