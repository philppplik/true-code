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
Edition 2024, MSRV 1.88.

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
- MSRV is **1.88**, checked in CI so it cannot drift silently. Edition 2024 alone
  would only require 1.85; the higher floor comes from ratatui 0.30, which was
  adopted to clear a use-after-free advisory in a transitive dependency. Picking
  the newer compiler over the older dependency is the trade this project will
  keep making.

## Amendment, 2026-09-13 — licensing

Originally released under the Rust ecosystem's customary `MIT OR Apache-2.0`.
Now **MIT only**, at the maintainer's decision.

The dual licence exists mainly to offer Apache-2.0's explicit patent grant. MIT
alone is shorter, understood by everyone, and imposes no constraint that matters
for a project of this size. Contributors should be aware the terms are simply
MIT; anyone who needs an explicit patent grant should raise it in an issue.
