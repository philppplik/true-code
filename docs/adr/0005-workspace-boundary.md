# ADR 0005 — The workspace is the security boundary

**Status:** accepted · **Date:** 2026-09-13

## Context

Once a model can call tools, repository contents stop being inert data. A file,
an issue body or a web page fetched into context can contain text aimed at the
agent — and the agent has a filesystem. Prompt injection is not a hypothetical
here; it is the expected operating condition.

The kernel-level answer (Landlock, seccomp, Seatbelt) is Unix-only, which is
exactly the gap true-code exists to close on Windows. Something has to hold
before that lands.

## Decision

Every path a model supplies is resolved by `tc_tools::path::resolve_in_workspace`
before it reaches the filesystem. The rule is one sentence: **the real, symlink-
resolved path must be inside the workspace root, or the call fails.**

Three properties follow from doing it this way rather than with string checks:

* `src/../README.md` is **allowed** — it resolves inside. A `..`-rejecting filter
  would refuse legitimate paths.
* A symlink inside the workspace pointing at `/etc/shadow` is **refused** — it
  resolves outside. A `..`-rejecting filter would let it through, because the
  path contains no `..` at all.
* A file that does not exist yet is still checked, by canonicalising the deepest
  existing ancestor. Otherwise the check would be unusable for writes.

Alongside it, two limits that are about cost and quality rather than security:
output is truncated with a visible marker, and walks respect `.gitignore`.

## Consequences

- The boundary is enforced in exactly one function, with adversarial tests
  (traversal, absolute paths, deep escapes, symlinks on Unix). Every tool
  inherits it; no tool can opt out.
- A refused path is a **tool error, not a crash**: the model reads the message
  and corrects itself. That is verified end-to-end in `tests/agent_loop.rs`.
- `require_git(false)` is set on every walk, so `.gitignore` applies to a project
  folder that is not a git repository. Surprising default, deliberate override.
- This is an application-layer boundary. It stops a confused or injected model;
  it does not stop a bug in our own code from reading a file. The OS-level
  sandbox is still needed and is still on the roadmap — this ADR does not claim
  to replace it.
- The symlink test is Unix-only, because creating symlinks on Windows needs
  elevation. The check itself is platform-independent; the coverage is not.
