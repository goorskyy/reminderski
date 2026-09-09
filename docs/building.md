# Building

Requires a Rust MSVC toolchain and Visual Studio Build Tools with the *Desktop development with
C++* workload, which provides the linker.

The workload's default components cover x64 and x86 only. On an ARM64 machine also add
`Microsoft.VisualStudio.Component.VC.Tools.ARM64`, or `cargo build` fails with
`linker link.exe not found`.

```
cargo build
cargo test
cargo fmt --check
cargo clippy --all-targets
```

A debug build has one extra key: click a notification and press **M** to step the character
through every mood. It is compiled out of a release build.
