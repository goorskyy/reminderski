# Reminderski

A tiny, fast, local reminder app inspired by Slack's reminder workflow.

The core interaction is:

Select text → global shortcut → enter `in 40m` → Enter → forget about it.

When the reminder is due, a minimal notification appears with quick actions such as snooze or done.

The initial target is Windows.

The application is intentionally small, monochrome, keyboard-first, and local.

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

## Project status

Early development.

The current execution plan is defined in `MILESTONE.md`.

Future ideas are kept in `FEATURES.md`.
