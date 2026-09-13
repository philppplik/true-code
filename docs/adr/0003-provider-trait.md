# ADR 0003 — A hand-rolled provider trait, not an LLM framework

**Status:** accepted · **Date:** 2026-09-13

## Context

Model neutrality is a core promise of true-code: the competing tools are each
locked to a single vendor, and choosing the right model per task is the single
largest lever on cost.

The options were an official vendor SDK per provider, a multi-provider crate, or a
narrow trait of our own with `reqwest` underneath.

## Decision

Define `Provider` in `tc-providers` — five methods, one normalised `Delta` stream
— and implement it per vendor with `reqwest` and `eventsource-stream`, roughly
150 lines each.

Everything the vendors disagree about is normalised at this boundary: streaming
event shapes, tool-call formats, usage accounting, stop reasons, cache metrics.

## Rationale

- **The interface is ours, only the implementation is replaceable.** When a crate
  is abandoned or changes shape, the blast radius is one file.
- **Cost accounting needs details frameworks drop.** Cached-input tokens are
  reported differently by every vendor, and a framework that flattens them to a
  single `input_tokens` makes an accurate cost display impossible. That is not an
  edge case here — it is the product.
- **Vendor quirks must be visible, not hidden.** OpenAI streams no usage unless
  `stream_options.include_usage` is set. A framework that silently omits it turns
  the cost display into a lie. In our own client that flag is a documented line of
  code with a test.

## Consequences

- More code up front, and SSE parsing is ours to get right. Mitigated by unit
  tests over recorded payloads for each event type, including unknown ones.
- Adding a provider is a deliberate act, not a config entry. Acceptable: the
  research is explicit that three excellent providers beat twenty-five mediocre
  ones.
- Unknown event types are ignored rather than treated as errors, so a vendor
  adding a field does not break a running turn.
- Cancellation is "drop the stream" — no cancel method, no half-cancelled state.
