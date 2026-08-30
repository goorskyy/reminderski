# AGENTS.md

## Engineering Rules

These rules are mandatory unless explicitly overridden by the human.

### 1. Human controls the repository

- Never push without explicit human approval.

### 2. Small increments

Work only on the current task/milestone.

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
