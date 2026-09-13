# ADR 0010 — Ask one question, at a commit point, and only if asked to

**Status:** accepted · **Date:** 2026-09-13

## Context

Heavy AI use measurably erodes understanding of one's own codebase. Developers
who mostly generate score worse on comprehension tests; most maintenance of
agent-written code is done by humans afterwards; mental models of a codebase now
go stale in weeks. An agent that leaves you less able to work on your own project
has taken something from you, whatever it gave.

No competing tool addresses this, and the reason is structural: learning is not
measurable in benchmarks and it *reduces* token consumption. That is exactly why
the gap stays open.

## Decision

When enabled, a run that changed code ends with **one** multiple-choice question
about the change it just made, generated from what actually changed, followed
immediately by an explanation.

`docs/RESEARCH-Lernen-2026-09.md` is the source, and five of its findings each
rule out something that would otherwise have been the obvious design:

| Finding | What it rules out |
|---|---|
| Retrieval beats re-reading (g = 0.74) | Printing a summary of the diff |
| Interrupt only at commit points; the window closes in under a minute | Asking mid-loop, or batching questions for later |
| Transient information effect | A dialog whose explanation scrolls away |
| Prior knowledge predicts; learning styles do not (d = 0.04) | A "visual learner" setting |
| Gamification lowered motivation *and* performance | Scores, streaks, badges |

The profile in `.truecode/learning.toml` therefore holds counts per concept and
nothing else. It feeds one line back into the question prompt: what this person
has previously got wrong, so the question prefers it and the explanation goes
deeper.

## Three decisions that will look wrong at first

**It is off by default.** An unrequested quiz after every edit is the fastest way
to make someone disable a feature permanently. `--learn` opts in.

**It does not block.** The name in the research is "comprehension *gate*", and
this is not one. By the time it asks, the change is applied — refusing to proceed
would protect nothing and only teach people to reach for the off switch. Naming
it a gate would be a claim the code does not support.

**It costs a separate model call**, billed and displayed like any other. Folding
it into the main turn would let the model skip the question whenever the
conversation got interesting, which is precisely when it matters. A feature that
spends the user's money quietly is the opposite of what this project is for.

## Consequences

- Question quality is entirely the model's. A bad question is a bad question, and
  the only guards are structural: the prompt forbids syntax and naming questions,
  and an unparseable or self-contradictory answer is dropped rather than shown.
- **A skipped question records nothing.** Counting it as wrong would make the
  profile lie, and the profile is the basis for what gets explained next.
- The explanation is shown whether the answer was right or wrong. Someone who
  guessed correctly has learned nothing yet.
- This closes P2, but it is the *first* version of the thread P4 continues:
  no fading of scaffolding as competence grows, no spaced revisiting, no
  explain-it-back mode. The profile is deliberately shaped to support those.
- The question covers *which files changed*, not the diff contents. Sending the
  full diff would ask better questions and cost more; worth revisiting with real
  usage rather than guessing now.
