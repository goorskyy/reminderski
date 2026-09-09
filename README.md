# Reminderski

[![CI](https://github.com/goorskyy/reminderski/actions/workflows/ci.yml/badge.svg)](https://github.com/goorskyy/reminderski/actions/workflows/ci.yml)

Local reminders for Windows. Select text, press **Ctrl+Alt+R**, say when.

![A reminder coming back, having been put off nine times](docs/screenshots/notification.png)

It waits in the notification area and costs nothing until it has something to say. When a reminder
falls due it appears in the corner without stealing the focus, so it never swallows what you are
typing: Ctrl+Alt+A to reach it, then Enter for Done or 1, 2, 3 to put it off.

Nothing leaves the machine.

## Install

Download the `.exe` from the [latest release](https://github.com/goorskyy/reminderski/releases/latest)
and run it. Nothing to install, nothing else to download. Take the `aarch64` one on an ARM64
machine.

An `R` appears in the notification area. Right-click it to quit, to open the dashboard, or to
start it with Windows.

## When

`in 40m` · `1h30m` · `at 3pm` · `today at 18:00` · `tomorrow` · `friday at 9` · `next tuesday`

[All of it](docs/time-expressions.md).

## Etc

Reminders are kept in `%APPDATA%\Reminderski\reminders.txt`, one per line.

[Known problems](FINDINGS.md) · [Ideas](FEATURES.md) · [Building](docs/building.md) ·
[House rules](AGENTS.md)

MIT. See [`LICENSE`](LICENSE).
