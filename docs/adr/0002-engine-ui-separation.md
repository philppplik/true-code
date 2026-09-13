# ADR 0002 — The engine never depends on the UI

**Status:** accepted · **Date:** 2026-09-13

## Context

The obvious way to build a TUI application is to let the UI own the state and call
the network from where the keystroke arrives. It is also the cheapest mistake in
the project: once the engine reaches into `ratatui` types, the agent can only run
inside a terminal.

That closes off headless CI runs, scripting, editor integration over ACP, and
reproducible evaluation — the last of which is how agent quality is measured at
all.

## Decision

`tc-core`, `tc-config` and `tc-providers` must not depend on `ratatui` or
`crossterm`. The dependency direction is one-way:

```
tc-cli ──► tc-tui ──► tc-providers ──► tc-config ──► tc-core
   └──────────────────────┴──────────────────┴───────────┘
```

Two front-ends consume the same engine from day one: the TUI and the headless
`-p` mode. A second consumer is what keeps the boundary honest — a rule with only
one caller erodes quietly.

Permission prompts, when they arrive, will be a callback the caller supplies, not
a TUI call inside the loop.

## Consequences

- Streaming reaches the UI over an `mpsc` channel rather than by direct call. More
  plumbing, but the event loop cannot be blocked by the network — which is what
  makes `Esc` reliable rather than best-effort.
- The `App` state machine is pure and unit-tested without a terminal.
- ACP and LSP integration later become another consumer of the same engine, not a
  rewrite.
- The cost is real: some state is threaded through more layers than a monolith
  would need.
