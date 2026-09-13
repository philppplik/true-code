# ADR 0008 — Rules are checked, not just stated

**Status:** accepted · **Date:** 2026-09-13

## Context

The most expensive measured failure of CLI coding agents is not that they cannot
code. It is that they violate a rule the user stated **explicitly** — roughly half
of CLI agent failures are instruction-following failures.

"No new dependencies", said once in turn one, has no force by turn ten. It is
buried under tool results, it competes with everything else in the window, and
nothing anywhere verifies it. The user finds out at review time, if at all.

The tempting response is to say it louder in the system prompt. That does not
work, because the failure is not one of emphasis.

## Decision

Rules live in `.truecode/constraints.toml`, and two things happen to them.

**They are restated in full on every request.** Appended after the cacheable
prefix and the mode section, so prompt caching still hits and the rules do not
decay with the conversation.

**They are checked mechanically at the approval gate**, against the previewed
change, before it is applied. A violation turns the confirmation modal red and is
listed above the diff.

Three rule shapes, deliberately no more:

- `forbid_added` — a regex that must not appear in any **added** line, optionally
  scoped by `in_files` / `except_files`.
- `forbid_files` — globs the change must not touch at all.
- `forbid_command` — a regex a shell command must not match.

Anything that cannot be expressed that way goes in `[[reminder]]`, which is sent
to the model and **labelled as unchecked**, in the prompt and in `true-code
constraints`.

## Details that carry the weight

- **Only added lines are judged.** A pre-existing `unwrap()` is not the agent's
  fault, and *deleting* one must never read as a violation.
- **A violation does not block; it interrupts.** The human decides. Mechanical
  rules produce false positives, and an agent that cannot be overridden is worse
  than one that can be argued with.
- **Except in CI.** `ApproveAll` — what `--yes` installs — refuses a change that
  breaks a rule. `--yes` means "do not ask me about routine changes", not "ignore
  the rules I wrote down", and unattended is where an unnoticed violation does the
  most damage.
- **"Always allow this tool" does not extend to violations.** A blanket approval
  covers the routine case; a broken rule is precisely the case worth interrupting
  for.
- **If it was allowed anyway, the model is told.** The tool result names the rule
  it broke, so one approval is not read as standing permission.
- **A malformed ledger is a hard error.** Silently skipping a rule the user wrote
  down is the exact failure this exists to prevent, so an unknown key or an
  invalid regex stops the session and names the field.
- **It is recorded.** `constraint_violated` goes into the session log whether or
  not the change was allowed, so "I was warned and said yes" stays distinguishable
  from "nobody noticed".

## Consequences

- Every rule costs context on every request. The ledger must stay short; this
  project's own `.truecode/constraints.toml` has five checks and three reminders,
  and that is already near the useful limit.
- Regex and globs cannot express most real constraints. "Keep the public API
  stable" is a reminder, not a check, and the UI says so rather than implying a
  green tick was earned.
- A badly written rule is worse than no rule: it fires on clean changes, and users
  learn to approve past it. `true-code constraints` exists so a rule can be read
  back and checked before it is trusted.
- Extraction of constraints from the *prompt* — "don't add dependencies", typed in
  chat — is not implemented. It needs a model call and is a separate decision.
- This is the first half of the research's "proof, not claims" thread. The second
  half, an evidence block replacing the word "done", is still ahead.
