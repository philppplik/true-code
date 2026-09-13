# Contributing to true-code

Thanks for looking. This project is early — a spike, not a product — so the most
valuable contributions right now are sharp questions and small, focused patches.

## Before you write code

Open an issue first for anything beyond a bug fix or a typo. The roadmap in
[`docs/PLAN-v0.1.md`](docs/PLAN-v0.1.md) deliberately excludes a long list of
tempting features; a patch that implements one of them will be declined no matter
how good it is. Asking first saves your evening.

## Setup

```bash
rustup toolchain install stable      # 1.85 or newer
git clone https://github.com/philppplik/true-code
cd true-code
cargo test --workspace
```

No API key is needed to build or test. Every test runs offline — provider code is
tested against recorded payloads, not against live endpoints.

## The checks that must pass

CI runs exactly these, on Windows, Linux and macOS:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo doc --workspace --no-deps      # with RUSTDOCFLAGS="-D warnings"
```

Run them locally before pushing. A failing `main` blocks everyone.

## Code standards

- **Engine and UI stay separate.** `tc-core`, `tc-config` and `tc-providers` must
  never depend on `ratatui` or `crossterm`. Anything that only works inside the
  TUI cannot be tested in CI or driven by an editor.
- **`unsafe` is forbidden** workspace-wide. If you believe you need it, open an
  issue — that is an architecture discussion, not a patch.
- **No `unwrap()` outside tests.** Errors are context for the user, not a crash.
- **Errors are typed in libraries** (`thiserror`) and contextual in the binary
  (`anyhow`). An error message should tell the user what to do next.
- **Tests describe behaviour, not implementation.** `budget_warns_before_it_stops`
  tells a reader what is guaranteed; `test_budget_2` does not.
- **Comments explain *why*.** The code already says what it does.

## Secrets

API keys come from the environment, never from a config file, and never appear in
logs or the session event log. A patch that reads a key from a file will be
declined. If you think you have leaked a key, rotate it first, then tell us.

## Commits

[Conventional Commits](https://www.conventionalcommits.org/):

```
feat(providers): add streaming support for Ollama
fix(tui): restore the terminal when the provider panics
docs(adr): record why the provider trait is hand-rolled
```

Types in use: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `ci`.

Keep commits small and self-contained. A commit that mixes a refactor with a
behaviour change is hard to review and impossible to revert cleanly.

## Pull requests

`main` is protected: changes land through pull requests with green CI.

- Describe **what changed and why**, not just what the diff shows.
- Say what you tested, and what you did not.
- Under 300 lines where you can. Large diffs get reviewed late and shallowly —
  that is a measured effect, not an opinion.

## Architecture decisions

Anything structural gets a one-page ADR in [`docs/adr/`](docs/adr/): context,
decision, consequences. It takes ten minutes and answers "why on earth is this
like that?" six months later.

## Code of conduct

Be decent. Assume good faith. Critique code, not people. Conduct that makes this
a worse place to work will get you removed, without a long process about it.
