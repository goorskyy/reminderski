# AGENTS.md

## Engineering Rules

These rules are mandatory unless explicitly overridden by the human.

### 1. Human controls the repository

- Never push without explicit human approval.

### 2. Small increments

Work only on the current task.

Implement the smallest solution that satisfies the requirements. Do not implement speculative features, future architecture, or unrelated improvements.

Prefer simple, direct code over abstractions and frameworks.

### 3. Test every change

After meaningful changes:

1. build/check;
2. run relevant automated tests.

Never hide, weaken, delete, or skip tests just to make them pass.

A task is not done while known relevant failures remain.

When implementation and automated tests are complete, stop and wait for human review. Do not continue with additional work unless instructed.

### 4. Readability over cleverness

Code should read like a book:

- clear names;
- small functions;
- straightforward control flow;
- minimal indirection.

Do not add complexity merely for "proper architecture", extensibility, or engineering fashion.

Avoid unnecessary abstractions, interfaces, generic code, wrappers, dependency injection, frameworks, and dependencies.

Every abstraction or dependency should solve a real current problem.

### 5. Comments

Prefer self-explanatory code.

Do not comment obvious operations or restate the code.

Comments are for non-obvious reasons, platform quirks, workarounds, or important invariants that cannot be expressed clearly in code.

### 6. Performance & size

The application should be fast, lightweight, and responsive.

Responsiveness comes first and is not negotiable. The shortcut must stay instant and the idle
cost near zero, whatever else is added. Executable size is allowed to grow when something is
worth it, so size alone is not a reason to refuse a dependency; what it solves, and what it
drags in behind it, still is.

Prefer:

- low startup latency;
- fast shortcut response;
- low idle resource usage;
- minimal dependencies;
- minimal unnecessary background work.

Do not sacrifice significant readability for unmeasured micro-optimizations.

### 7. Errors

Handle failures deliberately and proportionally.

Never silently lose important data or claim an operation succeeded when it failed.

Do not build elaborate error-handling machinery for trivial cases.

### 8. Platform code

Keep Windows-specific code isolated where practical.

Document non-obvious Windows API behavior or workarounds in code comments.

Target behavior must be verified on the actual target platform; automated tests alone are not sufficient for platform integration.

**The human does that verification, by running the application.** Do not drive the running
application from a script: synthetic keystrokes go to whichever window has focus, so they land
in the human's own window, and the capture shortcut sends a real Ctrl+C to whatever is in front
of it, which can interrupt their work.

Capturing a window as an image to check a visual change is the exception, and is welcome —
looking at the UI is the part the human would rather not do by hand. Where something genuinely
must be driven, post messages to the window rather than synthesising input.

### 9. Definition of done

Before stopping, verify:

- scope is respected;
- code is readable;
- unnecessary complexity was avoided;
- relevant automated tests pass;
- build/check passes;
- no known relevant regression remains.

Then report:

Changed
Tested
Result
Known limitations

and wait for human review.

### Golden rule

Keep it small. Keep it readable. Test it. Stop. Ask.
