# Features

Ideas and potential future features for Reminderski.

Nothing in this file is approved work until it is picked.

The order is the priority: the first line is what would help most, the last what would help
least. Roughly, it runs from making the app trustworthy, through the friction of using it every
day, to reaching places it does not yet go. The numbers move when the order does, so they name a
position rather than a feature.

| # | Feature | Effort | Description |
|---|---|---|---|
| 1 | Confirm a reminder was saved | Low | The form closes instantly on confirm with no acknowledgement. Show something like "will remind you at <date/time>" so it's clear it was saved and for when. |
| 2 | Launch confirmation | Low | Show something at startup (e.g. a tray balloon) confirming the app has started, so double-clicking the .exe doesn't feel like nothing happened. |
| 3 | Delete reminders | Medium | Remove a reminder for good. Needs a stable identifier per reminder first, since the dashboard and any open notification address them by position. |
| 4 | Reminder editing | Low | Edit an existing reminder. Wants the same stable identifier that deleting does, so the two belong together. |
| 5 | Tune the two windows | Low | Spacing, proportions and weight across the form and the notification, once they have been lived with. The notification feeling oversized is already written down in FINDINGS.md. |
| 6 | Recurring reminders | Medium | Support reminders that repeat on a schedule. The one capability of the Slack workflow that is missing outright. |
| 7 | Custom snooze durations | Low | Allow configurable or additional quick snooze options. These are the most-pressed buttons in the application. |
| 8 | Configurable shortcut | Low | Let the user change the global capture shortcut, which is fixed at Ctrl+Alt+R and may already belong to something else. |
| 9 | More reminder syntax | Low | Expand Slack-style time expressions further. Day names are understood; `in 2 weeks` and month ends are not. |
| 10 | Live dashboard | Low | Push changes to the open page instead of re-reading every fifteen seconds. |
| 11 | Search | Medium | Search active and historical reminders. Worth more the longer the history gets. |
| 12 | winget package | Medium | Publish a winget manifest so Reminderski can be installed with `winget install`, pointing at the GitHub release. |
| 13 | Import/export | Medium | Export and restore reminder data. Half free already, since the store is one text file. |
| 14 | URL capture | Medium | Capture a URL together with selected text when available. |
| 15 | Application context | Medium | Store the source application/window together with a reminder. |
| 16 | Multiple personas | Low | Different ASCII personas for notifications. |
| 17 | CLI client | Medium | Terminal client for creating, listing, completing and snoozing reminders. |
| 18 | Browser integration | Medium | Dedicated browser integration for capturing reminders. |
| 19 | API | High | Provide an API for external clients and integrations. Everything below this line wants it first. |
| 20 | Slack integration | Medium | Create or manage Reminderski reminders through Slack. |
| 21 | Teams integration | Medium | Create or manage Reminderski reminders through Microsoft Teams. |
| 22 | Calendar integration | High | Connect reminders with calendar events. |
| 23 | Cloud sync | High | Optional backend allowing reminders to be shared across devices. |
| 24 | Local network sync | High | Synchronize clients without relying on a cloud service. |
| 25 | macOS app | High | Desktop client with the same core workflow as Windows. |
| 26 | Mobile app | High | Mobile client sharing the same reminder ecosystem. |
