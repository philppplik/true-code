# ADR 0001 — Rust with ratatui and crossterm

**Status:** accepted · **Date:** 2026-09-13

## Context

true-code is a terminal harness that must start instantly, stream tokens without
stutter, run as a single self-contained binary, and treat Windows as a first-class
platform rather than a port.

The realistic alternatives were TypeScript on Node or Bun (fastest to iterate,
what Claude Code uses), Go (simple concurrency, easy cross-compilation), and Rust.

## Decision

Rust, with [ratatui](https://ratatui.rs) for widgets, [crossterm](https://github.com/crossterm-rs/crossterm)
as the terminal backend, and [tokio](https://tokio.rs) as the async runtime.
Edition 2024, MSRV 1.85.

## Rationale

- **Single binary, no runtime.** A harness that requires a Node install before it
  can help you is a worse first experience, and a worse CI dependency.
- **Windows support is the deciding factor.** crossterm supports ConPTY and the
  Windows console properly. The long-term differentiator identified in the
  research — a real Windows sandbox — needs direct access to Win32 job objects and
  restricted tokens, which is straightforward from Rust and painful from Node.
- **No stutter under streaming.** No garbage-collection pauses in the render loop.
- **ratatui is the de-facto standard** for Rust TUIs: actively maintained, large
  widget ecosystem, and an immediate-mode model that suits a screen whose content
  changes on every frame.

## Consequences

- Development is slower than in TypeScript, especially early. Accepted: the
  project is a multi-month effort where correctness and trust matter more than
  the speed of the first month.
- ratatui provides no scrollback; a pager has to be built by hand.
- The LLM tooling ecosystem in Rust is thinner than in Python or TypeScript. This
  pushed the decision in ADR 0003 (hand-rolled provider trait), which turned out
  to be the right call independently.
- MSRV 1.85 is required by edition 2024 and is checked in CI, so it cannot drift
  silently.
