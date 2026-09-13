# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the version is `0.x`, breaking changes can land in a minor release.

## [Unreleased]

### Added

- **The constraint ledger.** Project rules in `.truecode/constraints.toml` are
  restated to the model on every request *and* checked mechanically against each
  change before it is applied. A violation turns the confirmation modal red and
  lists the broken rule above the diff
  ([ADR 0008](docs/adr/0008-constraint-ledger.md)).
- Three checkable rule shapes — `forbid_added` (scoped by `in_files` /
  `except_files`), `forbid_files`, `forbid_command` — plus `[[reminder]]` for
  anything that cannot be checked, labelled as such everywhere.
- `true-code constraints` lists what is in force and what is only a reminder.
- `--yes` now refuses a change that breaks a rule. It means "do not ask me about
  routine changes", not "ignore the rules I wrote down".
- A blanket "always allow this tool" no longer covers a change that breaks a rule.
- `constraint_violated` is recorded in the session log whether or not the change
  was allowed, so "I was warned and said yes" stays distinguishable from "nobody
  noticed".
- A malformed ledger is a hard error naming the offending rule and field, rather
  than a silently skipped rule.

### Changed

- `Agent::new` takes an `AgentSetup` struct instead of seven positional
  arguments. Pre-1.0 breaking change to the library API.

### Earlier in this cycle

- **Undo.** `/undo` in the TUI and `true-code undo` from the shell revert the last
  change true-code made. Run it again to step further back. Files are copied aside
  before they are changed, and undo is derived entirely from the session log — so
  it works from a fresh shell long after the session ended
  ([ADR 0007](docs/adr/0007-undo-from-the-event-log.md)).
- Undoing a file the agent *created* deletes it again, rather than leaving an
  empty shell behind.
- `/help` in the TUI. An unrecognised slash command is answered locally instead of
  being sent to the model, which would charge for a confused reply.

### Earlier in this cycle

- **Editing, behind a confirmation.** `write_file` and `patch` in write mode,
  `shell` in full mode. Every change is previewed as a diff and applied only if
  you agree; every command is shown before it runs. See
  [ADR 0006](docs/adr/0006-confirm-before-changing.md).
- **Three permission modes** (`--permission-mode read-only|write|full`), defaulting
  to read-only. A tool the mode does not allow is not offered to the model at all,
  rather than offered and then refused.
- `patch` refuses an ambiguous match and says how to make it unique, so a
  mis-targeted edit fails instead of changing the wrong line.
- Writes go through a temporary file and a rename, so a crash cannot leave a
  truncated source file.
- The shell tool refuses a short list of catastrophic commands outright, even
  with approval, and flags high-risk ones prominently at the prompt.
- The system prompt now states what the agent can actually do in the current mode,
  including admitting in write mode that it cannot run the tests.
- Approval decisions are recorded in the session log, so the audit trail answers
  "who allowed this?" and not only "what ran?".
- `--yes` for headless runs, and a distinct exit code when a change was declined.

### Earlier in this cycle

- **The agent loop.** true-code now reads your actual code before answering,
  instead of guessing from the prompt. Every tool call is visible in the
  transcript as it runs.
- **Four read-only tools** — `read_file` (with line ranges), `list_dir`, `glob`
  and `grep`. All are scoped to the project directory by a symlink-resolving path
  check, respect `.gitignore`, and truncate oversized output with a visible marker
  so the model never mistakes a fragment for a whole file.
- **Guard rails that stop with a stated reason**: a turn limit, a session budget
  checked *before* each request, and loop detection for an agent repeating one
  identical call. A silent stop is indistinguishable from success, so there are
  none.
- **Session logging** to `.truecode/sessions/<id>/events.jsonl`, including tool
  calls recorded before execution — so a run that crashes mid-tool still shows
  what was attempted.
- **Tool-call support in both providers**, including Anthropic's partial-JSON
  block streaming and OpenAI's index-keyed accumulation. A tool call is only
  emitted once its arguments parse.
- A distinct exit code (`2`) when a guard rail stopped a headless run, so a
  script can tell "answered" from "gave up".

### Changed

- `Message` now carries content blocks rather than a single string, so a turn can
  mix text and tool calls. Pre-1.0 breaking change to the library API.

### Earlier in this cycle

- **Streaming TUI** built on ratatui and crossterm: live transcript, status bar,
  context gauge, and a cost readout that changes colour as the budget is consumed.
- **Instant abort.** `Esc` cancels a running turn by dropping the HTTP stream. The
  event loop never blocks on the network, so the key always responds.
- **Cost accounting from turn one.** Per-turn token usage and price, session
  totals, cache-hit rate, and a hard session budget that stops the session instead
  of producing a surprise bill.
- **Model-neutral provider layer** with a normalised delta stream. Anthropic
  Messages and OpenAI Chat Completions wire protocols, including prompt-cache
  accounting on both.
- **Headless mode** (`true-code -p "…"`). The answer goes to stdout, the
  accounting to stderr, so piping stays clean.
- **Layered configuration** — defaults, user file, project file, environment,
  flags — with `true-code config` to show what actually took effect, and
  `true-code models` to show the catalogue and its assumed prices.
- **Append-only session event log format** ([ADR 0004](docs/adr/0004-append-only-event-log.md)),
  defined now so that resume and replay are derivations rather than retrofits.
- CI on Windows, Linux and macOS: format, clippy, tests, docs, MSRV and a
  dependency advisory scan.

### Security

- `unsafe` is forbidden across the workspace.
- API keys are read from the environment only, never from a configuration file.

[Unreleased]: https://github.com/philppplik/true-code/commits/main
