# Code signing policy

Free code signing provided by [SignPath.io](https://signpath.io/), certificate by
[SignPath Foundation](https://signpath.org/).

**Not in effect yet.** The certificate has been applied for and no release is signed so far.
Until one is, Windows will say the publisher is unknown, and the checksums and build provenance
published with every release are how to tell what you downloaded.

## Who

One maintainer, who wrote it, is the only person who can commit, and is the only person who can
approve a signing request.

| Role | Person |
|---|---|
| Author, committer, reviewer | Michał Górski |
| Approver | Michał Górski |

## What gets signed, and where it comes from

Nothing is built on a developer machine. Pushing a tag is what publishes, and
[the release workflow](../.github/workflows/release.yml) builds both executables on a GitHub
runner from the commit that tag names. GitHub signs a provenance statement saying which workflow
and which commit produced each one, so anybody can check that the executable they have is that
build and not something else wearing its name:

```
gh attestation verify reminderski-x86_64-pc-windows-msvc.exe --repo goorskyy/reminderski
```

The SHA-256 of every executable is published beside it as `checksums.txt`.

## Privacy

Reminderski sends nothing anywhere. It opens no outbound connection, and it has one dependency,
`windows-sys`, which is bindings to the Windows API and no more.

Reminders are lines in a text file under `%APPDATA%\Reminderski` on the machine that created
them. The dashboard is a page served to your own browser by a listener bound to `127.0.0.1`, so
it is reachable from this machine only and never leaves it.

## Reporting a problem

Open an issue at <https://github.com/goorskyy/reminderski/issues>. For something you would
rather not say in public, say so in the issue without the details and a private channel can be
arranged.
