# ADR 0004 — Sessions are an append-only event log

**Status:** accepted · **Date:** 2026-09-13

## Context

Resume, undo, replay, cost reporting and post-mortem debugging all look like
separate features. They are the same feature seen from different angles: each one
needs to know what happened, in order, with everything that influenced it.

Deciding this late is expensive. A session modelled as mutable state can be made
resumable, but it can never be made replayable — the intermediate states were
overwritten and are simply gone.

The industry position in 2026 is blunt: agent failures in production are usually
irreproducible, because nobody recorded the moving parts.

## Decision

A session is an append-only sequence of events, written to
`.truecode/sessions/<id>/events.jsonl` as JSON Lines — one self-contained object
per line, appended and never rewritten.

```json
{"session_id":"7c9e…","seq":3,"at":"2026-09-13T15:24:05.123Z","kind":"user_message","content":"explain this diff"}
{"session_id":"7c9e…","seq":4,"at":"2026-09-13T15:24:09.880Z","kind":"turn_completed","stop_reason":"end_turn","usage":{"input_tokens":1204,"output_tokens":301,"cache_read_tokens":8000,"cache_write_tokens":0},"cost":{"usd":0.0087}}
```

The event type is flattened into the object rather than nested, so the log stays
greppable with ordinary tools.

`seq` is monotonic within a session so that `replay --step N` is addressable
without counting lines.

## Rationale

- **JSON Lines over a database.** Debuggable with `tail` and `grep`, diffable,
  and a crash mid-write costs one truncated line rather than the session. A
  database goes in later for the *index*, which is a different problem.
- **Append-only over mutable state.** Replay, run-diff (comparing two runs of the
  same task) and "save this failure as an evaluation case" all become derivations
  of the log rather than new subsystems.
- **Cost is recorded per turn, not summed at the end.** A total that cannot be
  broken down cannot be acted on.

## Consequences

- Logs grow. Rotation and retention are needed before this goes wide.
- **Logs can contain source code excerpts.** They stay local, sharing is opt-in,
  and API keys must never be written into an event. This is a standing
  constraint on every future event type, recorded here so it is not rediscovered
  the hard way.
- Every new capability must express itself as an event, which is a mild but
  deliberate design constraint.
- P0 defines the format and writes nothing yet; the writer lands with the agent
  loop in P1. Defining it now is the whole point — the shape has to exist before
  the first turn is recorded.
