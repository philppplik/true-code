# ADR 0006 — Nothing changes without a human seeing the diff

**Status:** accepted · **Date:** 2026-09-13

## Context

A single "the agent deleted my file" moment ends adoption. Not for that user —
for everyone they tell. Trust is the product; the code changes are just how it is
delivered.

Meanwhile the measured failure modes are not exotic. Agents report success that
is not true, and violate explicitly stated constraints, in a sizeable fraction of
sessions. So the question is not *whether* the agent will propose something wrong
— it is what happens when it does.

## Decision

Three things, together.

**1. A preview, then the change.** Every mutating tool implements `preview`,
which computes exactly what it would do without doing it, and returns an `Effect`
— a diff for a write, a command line and risk rating for a shell call. The agent
never calls `run` on a mutating tool until an `Approver` has returned a decision.

The preview is a dry run over the *same* code path as the real call, not a second
implementation. `Patch::preview` and `Patch::run` both call `compute`.

**2. Approval is a callback, not a UI call.** Per ADR 0002, the loop asks an
`Approver` the caller supplies. The TUI implements one backed by a modal; headless
mode uses `DenyAll` unless `--yes`; tests use a recording stub. The loop is
unchanged by any of them.

**3. Three permission modes**, because the capabilities carry different risk:
reading cannot hurt you, editing is recoverable through version control, running
commands is neither. A tool the mode does not allow is **not offered to the model
at all** — advertising a capability and then refusing every call wastes context on
the schema and turns the next few turns into guesswork.

## Details that are load-bearing

- **The default everywhere is refusal.** `read-only` is the default mode, and an
  approver with nobody to ask denies. Opting in is one flag; opting out of a
  surprise is not possible.
- **A no-op write does not prompt.** Approval fatigue is the most-exploited
  weakness in agent systems: a prompt the user learns to dismiss has stopped
  protecting them. Every prompt must be worth reading.
- **A failing preview never reaches the human.** A `patch` whose snippet is not
  in the file is the model's mistake to fix; interrupting the user for it spends
  attention on nothing.
- **No "approve on Enter".** Enter sends prompts everywhere else in the TUI.
  Muscle memory must not be able to apply a change.
- **Approval is scoped per tool**, not globally. "Yes to all edits" and "yes to
  all shell commands" are very different promises.
- **The decision is logged.** `approval_decided` in the session log answers "who
  allowed this?", not only "what ran?".
- **Writes are atomic** — temporary file plus rename, on the same volume. A crash
  mid-write would otherwise leave a truncated source file, which is far worse than
  a failed tool call.

## Consequences

- Confirmation is enforced in the loop, so no tool can bypass it by construction —
  and the tests assert on the **file on disk**, because an event stream that says
  "declined" while the bytes changed is the bug worth catching.
- Interactive use costs a keystroke per change. That is the price, and it is the
  right one at this stage.
- `--yes` exists for CI and is genuinely dangerous. It is documented as such and
  is refused outside headless mode, where there would be a human to ask anyway.
- **This is not a sandbox.** The shell refusal list catches the plausible accident,
  not an adversary; the workspace boundary (ADR 0005) stops a confused model, not
  a bug in our own code. The OS-level sandbox is still on the roadmap and this ADR
  does not replace it.
- Undo and checkpoints are still missing. Confirmation means you see a change
  before it lands; it does not yet mean you can take it back afterwards.
