# ADR 0007 — Undo is derived from the event log

**Status:** accepted · **Date:** 2026-09-13

## Context

ADR 0006 made sure you see a change before it lands. That is only half of the
trust story. People approve changes and then regret them, and "you could see it
coming" is no comfort once the file is overwritten. Without undo, the honest
advice is "commit before every prompt", which is a workaround, not a feature.

Three options were on the table:

1. **A git stash or commit before each change.** Free history, but it hijacks the
   user's git state — index, stash stack, reflog — and does nothing at all in a
   project that is not a repository.
2. **A git worktree per task**, as the original plan suggested. Genuinely good for
   *parallel* agents, which is not the problem here, and it forces every user into
   a branch workflow to get a feature as basic as undo.
3. **Copy the file aside before changing it**, and record that in the session log.

## Decision

Option 3. Before a tool modifies a file, its bytes are copied into the session's
`backups/` directory, and a `Checkpointed` event is appended to the log — **in
that order**, so a crash between the two loses nothing that matters.

Undo is then a query over the log, exactly as ADR 0004 said it would be: find the
newest `Checkpointed` that no `Reverted` refers to, restore it, append a
`Reverted`. The log is the only state; there is no second index to keep in sync.

`Reverted` points at the checkpoint's **sequence number**, not at the path. That
is what makes repeated edits to one file undo one step at a time instead of
jumping straight back to the original.

Two entry points, one implementation: `/undo` inside the TUI, and `truecode undo`
from the shell — which works long after the session ended, because the log is on
disk and nothing is held in memory.

## Consequences

- **Undo composes with git rather than competing with it.** Your index, stash and
  branches are untouched, and it works in a directory that was never a repository.
- The log earns its keep. Resume, replay and now undo are all derivations of one
  append-only file; this is the first time that design has paid a dividend.
- `SessionLog::reopen` has to continue the sequence numbering rather than restart
  it, or `checkpoint_seq` would stop addressing an event uniquely.
- **Backups accumulate.** A long session with many edits leaves many copies under
  `.truecode/sessions/`. Retention is not implemented and will be needed.
- **This is not version control.** It undoes changes *true-code* made, newest
  first. It knows nothing about edits you made by hand in between, and restoring a
  checkpoint will overwrite them. The README says so in the same words.
- A failed checkpoint is reported but does **not** block the change. Refusing to
  edit because the *backup* could not be written would be a strange way to protect
  someone; instead they are told they are working without a net.
- Undo currently reverts one file per step, because one tool call changes one
  file. When a single call starts touching several, this needs to become
  transactional.
