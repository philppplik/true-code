<div align="center">

# true-code

**An agentic coding harness that shows you what it did — and what it cost.**

[![CI](https://github.com/philppplik/true-code/actions/workflows/ci.yml/badge.svg)](https://github.com/philppplik/true-code/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org)

</div>

---

## Why this exists

Coding agents that write code are a solved problem. Three things are not, and the
incentives of the big vendors point away from all three:

| Gap | What the research says | What true-code does |
|---|---|---|
| **Cost is a black box** | Analysts report that no vendor has strong cost-optimisation features — and vendors earn on token volume | Live cost per turn, hard budget caps, cache-hit visibility |
| **Nobody understands the generated code** | Developers report eroding codebase comprehension from heavy AI use | A LEARN mode that explains before it applies, and checks understanding |
| **Agents claim success instead of proving it** | A large share of agent sessions end in inaccurate self-reporting | Evidence blocks: no turn is "done" without machine-checkable proof |

An independent, open-source harness has no conflict of interest on cost. That
advantage is structural and cannot be competed away.

Full analysis: [`docs/RESEARCH-Gaps-2026-09.md`](docs/RESEARCH-Gaps-2026-09.md) ·
Roadmap: [`docs/PLAN-v0.1.md`](docs/PLAN-v0.1.md)

## Status

**Phase P1 — a read-only agent.** What works today:

- **Agent loop with tools.** The agent reads your actual code before answering:
  `read_file`, `list_dir`, `glob`, `grep`. You see every call as it happens.
- **Read-only by design.** Nothing can be modified yet. Writing lands together
  with diff confirmation, not before it.
- **Guard rails that stop with a reason** — turn limit, session budget, and loop
  detection for an agent repeating itself. No silent stops.
- **Live token and cost accounting**, with a budget that actually halts the run.
- **Append-only session log** in `.truecode/sessions/`, ready for replay.
- **Model-neutral provider layer** (Anthropic and OpenAI wire protocols),
  including tool calls and prompt-cache accounting on both.
- **Headless mode** (`-p`) for scripts and CI.

Not yet built: writing and shell tools, diff confirmation, permission modes,
OS sandboxing, MCP.
See [the roadmap](docs/PLAN-v0.1.md#6-roadmap-realistisch-mit-abnahmekriterien).

> Treat this as a spike, not a product. It will change shape.

## Install

Requires [Rust](https://rustup.rs) 1.88 or newer.

```bash
git clone https://github.com/philppplik/true-code
cd true-code
cargo build --release
```

The binary lands in `target/release/true-code` (`.exe` on Windows).

## Use

Set the API key for whichever provider you want. Keys are read from the
environment only — never from a config file, so a committed config can never
leak a credential.

```bash
export ANTHROPIC_API_KEY=...   # PowerShell: $env:ANTHROPIC_API_KEY = "..."
```

```bash
true-code                      # interactive TUI
true-code -p "explain this error" > answer.md
true-code models               # catalogue with context windows and assumed prices
true-code config               # resolved config, and where each part came from
```

In the TUI: `Enter` sends · `Esc` aborts the running turn · `Ctrl+C` quits ·
`PgUp`/`PgDn` scroll.

## Configure

Precedence, lowest to highest: defaults → user config → project config →
environment → command-line flags.

```toml
# .truecode/config.toml
model = "anthropic/claude-haiku-4-5"

[budget]
session_limit_usd = 2.0
warn_at_percent = 70
```

Run `true-code config` to see what actually took effect.

## Platform support

Windows is a first-class target, not an afterthought — kernel sandboxing is
Unix-only in every competing tool, which leaves a real gap. CI runs the full
suite on Windows, Linux and macOS on every push.

## Architecture

```
crates/
├── tc-core/       domain model — messages, tool calls, usage, cost, event log
├── tc-config/     layered config, model catalogue, secret resolution
├── tc-providers/  one narrow trait, one normalised stream, many vendors
├── tc-tools/      workspace-scoped tools with a hard path boundary
├── tc-agent/      the loop — turns, tool execution, guard rails, session log
├── tc-tui/        ratatui interface — streaming, cost, instant abort
└── tc-cli/        the `true-code` binary
```

**Architecture law #1:** the engine never depends on the UI. Anything that only
works inside the TUI cannot be tested in CI, scripted, or driven by an editor.

Decisions are recorded as ADRs in [`docs/adr/`](docs/adr/).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). In short: `cargo fmt`, `cargo clippy
-- -D warnings` and `cargo test` must be clean, commits follow
[Conventional Commits](https://www.conventionalcommits.org/), and `main` is
protected — changes land through pull requests.

## Security

Never paste an API key into an issue. Report vulnerabilities privately as
described in [SECURITY.md](SECURITY.md).

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option — the Rust ecosystem convention, and compatible with the MCP and ACP
ecosystems.
