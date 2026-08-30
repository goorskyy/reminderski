# Milestones

Execution plan for taking Reminderski from an empty repository to a satisfactory MVP.

Milestones are ordered. Complete and verify the current milestone before moving to the next one.

The plan may be adjusted when development reveals new information, but avoid adding work that is not necessary for the MVP.

---

## M1 — Repository Setup

**Goal:** Establish the smallest working Rust project.

### Work

- Create Rust project.
- Establish basic project structure.
- Configure formatting and linting.
- Verify clean build.
- Establish basic GitHub repository configuration where appropriate.

### Done when

- Clean checkout builds successfully.
- Basic automated checks run successfully.
- Project is ready for feature development.

---

## M2 — Windows Text Selection PoC

**Goal:** Prove that Reminderski can capture the complete selected text from any Windows application that supports the standard Copy command.

### Work

- Register global keyboard shortcut.
- Detect shortcut while another application is focused.
- Trigger the focused application's standard Copy command.
- Read the resulting Unicode text from the clipboard.
- Preserve and restore the user's clipboard contents on both success and failure.
- Open a minimal free-text input form after the shortcut.
- Pre-fill the form with captured text when a copyable selection exists.
- Leave the form blank and ready for manual entry when no text can be captured.

### Done when

The global shortcut captures the complete Unicode selection, including multiline text, without requiring the user to copy it manually.

The same shortcut opens an editable blank form when the focused application has no copyable text selection.

The user's clipboard contents are unchanged after both successful and unsuccessful capture attempts.

Capture is verified on a recorded matrix containing at least a native text editor, browser, terminal, Office-style editor, and rich-text editor.

Applications or surfaces that prevent copying, expose no textual selection, or are blocked by Windows security boundaries are explicitly recorded as unsupported by this capture mechanism.

No reminder functionality is part of this milestone.

---

## M3 — Reminder Creation

**Goal:** Turn captured text into a scheduled reminder.

### Work

- Minimal reminder input UI.
- Preserve captured text or manually entered text.
- Parse initial Slack-style time expressions.
- Create reminder.
- Persist reminder locally.

### Done when

The complete flow works:

Select text
→ Shortcut
→ Enter time
→ Reminder is stored

The application survives restart without losing the reminder.

---

## M4 — Reminder Scheduling & Notification

**Goal:** Deliver a reminder at the requested time.

### Work

- Schedule stored reminders.
- Handle application restart.
- Handle Windows sleep/wake.
- Display notification.
- Show ASCII persona.
- Provide Done action.
- Provide Snooze action.

### Done when

A user can create a reminder, wait for it, receive it, complete it, or snooze it.

---

## M5 — Local Dashboard

**Goal:** Provide a simple local overview of reminders.

### Work

- Local web server/dashboard.
- Active reminders.
- Completed reminders.
- Basic reminder actions.

### Done when

The user can inspect and manage reminders through the local dashboard.

---

## M6 — MVP Polish

**Goal:** Make the complete workflow reliable and pleasant enough to call Reminderski an MVP.

### Work

- Fix issues discovered during previous milestones.
- Improve startup and interaction speed.
- Verify persistence and recovery.
- Polish minimal monochrome UI.
- Verify keyboard workflow.
- Remove unnecessary dependencies and complexity.
- Clean up obvious technical debt.

### Done when

The complete application can be used reliably for everyday personal reminders.

The human explicitly considers the MVP satisfactory.

---

# MVP Completion

This milestone plan is considered complete when the human is happy with the MVP.

After MVP completion, future work should normally be introduced as new milestones rather than continuously expanding this plan.
