# ADR 0009 — Report what was observed, never what was claimed

**Status:** accepted · **Date:** 2026-09-13

## Context

A large share of agent sessions end with the agent reporting a status that is not
true. Not from dishonesty — because nothing ever required it to be true. "Done"
is free to say, and every incentive in a coding agent points at saying it.

The related failure is worse: an agent that edits the test until it passes. Any
design where the agent both does the work *and* reports on the work has this
problem built in, and no amount of prompting removes it.

## Decision

true-code does not ask the model whether it succeeded. At the end of every run it
reports what the **harness observed**:

- which files changed — recorded when the checkpoint was taken, before the write;
- which verification commands ran and what they exited with — read from the shell
  tool's own output;
- which project rules were broken.

From those facts a verdict follows mechanically:

| Situation | Verdict |
|---|---|
| A check exited non-zero | **FAILING** |
| Tests ran and passed | verified |
| Something ran, but no tests | partly verified |
| Files changed, nothing ran | **UNVERIFIED** |
| Nothing changed, nothing ran | no panel at all |

**The empty case is the feature.** Anything can print a green tick after a good
run. The value is in refusing to print one that was not earned, in red, with what
to do about it — `truecode verify`, or `--permission-mode full` so the agent can
run the tests itself.

## What it deliberately does not do

- **It does not read the model's prose.** A confident answer is not evidence,
  including a well-written one. The integration test for this has the model say
  "Done — everything works correctly now" while the panel returns UNVERIFIED.
- **It does not run anything by itself.** Verification is either something the
  agent did (full mode, confirmed like any command) or something the user runs.
  A panel that silently executed commands to fill itself in would be a permission
  hole wearing a feature's clothes.
- **It does not count activity as evidence.** A command classified as `Other` —
  `git status`, `ls` — is dropped from the panel entirely. Padding invites
  skimming past the line that matters.

## Details that carry weight

- **A failure outranks everything.** A run that changed code and broke the build
  must not be summarised by what else went well.
- **Every exit goes through one place.** A run that ended without a panel would
  let "it stopped" read as "it worked", so the budget stop, the loop stop and the
  turn limit all emit the panel first.
- **The panel is per prompt, not per session.** The question it answers is what
  *this* task did, not what the afternoon amounted to. A later prompt cannot
  inherit an earlier one's green tick.
- **Headless exits non-zero on an alarming verdict.** A piped run that reports
  success without evidence is exactly the failure this exists to prevent.
- **The exit-code parser lives beside its formatter** in `tc-tools::shell`, with a
  round-trip test. A parser that drifted from the format would silently stop
  finding evidence and report "unverified" for commands that really ran — a false
  negative that looks like working software.

## Consequences

- Verdicts are only as good as the classifier, which is keyword matching over
  command strings. It errs toward `Other`, so it under-claims rather than
  over-claims; a project with an unusual test command gets `unverified` when it
  deserved better. That is the right direction to be wrong in.
- **A passing test suite is not proof the change was correct** — only that the
  existing tests still pass. The panel says "verified", which is a claim about
  the checks, not about intent. The Intent-vs-Diff check is a separate piece of
  work and is not this.
- In `write` mode the agent cannot run anything, so an honest run there is almost
  always `unverified`. That is accurate rather than pessimistic, and the mode
  prompt already tells the model not to claim otherwise.
- `truecode verify` exists so the verdict is actionable. It detects the project
  from its shape, so it works the first time in a repository true-code has never
  seen.
