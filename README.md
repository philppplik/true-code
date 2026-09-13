<div align="center">

# true-code

**An agentic coding harness that shows you what it did — and what it cost.**

[![CI](https://github.com/philppplik/true-code/actions/workflows/ci.yml/badge.svg)](https://github.com/philppplik/true-code/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](#license)
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

**Phase P1 — an agent that can change code, once you agree.** What works today:

- **Agent loop with tools.** The agent reads your actual code before answering:
  `read_file`, `list_dir`, `glob`, `grep` — and in write mode `write_file` and
  `patch`, in full mode `shell`. You see every call as it happens.
- **Nothing changes without you seeing the diff.** Every write is previewed and
  confirmed before it is applied; every command is shown before it runs
  ([ADR 0006](docs/adr/0006-confirm-before-changing.md)).
- **Three permission modes**, defaulting to the safe one. A tool the mode does
  not allow is never offered to the model at all.
- **Guard rails that stop with a reason** — turn limit, session budget, and loop
  detection for an agent repeating itself. No silent stops.
- **Live token and cost accounting**, with a budget that actually halts the run.
- **Project rules that are actually checked.** Write them once in
  `.truecode/constraints.toml`; they are restated on every request *and* verified
  against the change before it is applied
  ([ADR 0008](docs/adr/0008-constraint-ledger.md)).
- **Undo.** `/undo` in the TUI, or `true-code undo` from the shell — which works
  long after the session ended, because it reads the session log
  ([ADR 0007](docs/adr/0007-undo-from-the-event-log.md)).
- **Append-only session log** in `.truecode/sessions/`, recording what ran, what
  you allowed, and what can still be taken back.
- **Model-neutral provider layer** (Anthropic and OpenAI wire protocols),
  including tool calls and prompt-cache accounting on both.
- **Headless mode** (`-p`) for scripts and CI.

Not yet built: OS sandboxing, MCP, the LEARN mode.
See [the roadmap](docs/PLAN-v0.1.md#6-roadmap-realistisch-mit-abnahmekriterien).

> Undo reverts what *true-code* changed, newest first. It is not version control:
> it knows nothing about edits you made by hand in between, and restoring a
> checkpoint will overwrite them. Commit your work.

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
true-code                                  # interactive TUI, read-only
true-code --permission-mode write          # can edit files, with confirmation
true-code --permission-mode full           # can also run commands
true-code -p "explain this error" > answer.md
true-code models                           # catalogue with assumed prices
true-code config                           # resolved config, and where it came from
```

In the TUI: `Enter` sends · `Esc` aborts the running turn · `Ctrl+C` quits ·
`PgUp`/`PgDn` scroll · `/undo` reverts the last change · `/help` lists commands.

When the agent proposes a change, a diff appears and waits:
`y` apply · `n` skip · `a` always allow this tool · `Esc` stop the run.
There is deliberately no "apply on Enter".

### Permission modes

| Mode | Can do | Confirmation |
|---|---|---|
| `read-only` *(default)* | read, search | none needed |
| `write` | + create and edit files | diff, per change |
| `full` | + run shell commands | diff or command, per call |

Headless runs refuse changes, because there is nobody to ask. `--yes` approves
everything and is meant for CI — it is exactly as dangerous as it sounds.

### Project rules

The measured failure of CLI agents is not bad code — it is ignoring a rule you
stated. Saying it once in a prompt does not survive ten turns. So write it down:

```toml
# .truecode/constraints.toml
[[constraint]]
description = "No unwrap() in production code"
forbid_added = '\.unwrap\('
in_files = ["**/src/**/*.rs"]

[[constraint]]
description = "Never publish from an agent session"
forbid_command = 'cargo publish'

[[reminder]]
text = "Comments explain why, not what."
```

Rules are restated to the model on every request **and** checked against the
change before it is applied. A violation turns the confirmation red and lists the
rule above the diff — you still decide, but you decide knowingly.

`[[reminder]]` entries are sent to the model but **not** checked. They are
labelled that way everywhere, because a green tick nobody earned is worse than no
tick at all.

```bash
true-code constraints   # what is in force, and what is only a reminder
```

In headless mode `--yes` still refuses a change that breaks a rule. "Do not ask me
about routine changes" is not "ignore the rules I wrote down".

This repository uses its own ledger — see
[`.truecode/constraints.toml`](.truecode/constraints.toml).

### Taking a change back

```bash
true-code undo    # revert the last change true-code made in this project
```

Run it again to step further back. It reads the newest session log, so it works
from a fresh shell, tomorrow, after you have closed everything.

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

[MIT](LICENSE). Permissive, short enough to read in a minute, and compatible with
the MCP and ACP ecosystems this project intends to join.
