# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the version is `0.x`, breaking changes can land in a minor release.

## [Unreleased]

### Fixed

- **MCP servers launched with `npx` or `uvx` failed on Windows.** Rust's
  `Command::new` does not apply `PATHEXT`, so `npx` — installed as `npx.cmd` —
  was reported as "program not found" on machines where `npx --version` worked in
  the same terminal. The lookup is now done the way the shell would do it.

### Changed

- `.truecode/config.toml` is gitignored. It records which provider and model
  *you* use; the rules, server list and hooks stay tracked and shared.


### Added

- **Project commands.** A Markdown file in `.truecode/commands/` becomes a slash
  command, using the same frontmatter and `$ARGUMENTS` / `$1`…`$9` format as
  Claude Code so existing files work unchanged. `/help` lists them.
- **Hooks.** Shell commands in `.truecode/hooks.toml` run before a tool, after a
  tool, or at the end of a run. A pre-tool hook exiting **2** refuses the call
  and its stderr becomes the reason the model is given; any other failure is
  reported but does not block. `truecode hooks` shows what a project would run.
- `Agent::history()`, so tests can assert on what the model was actually told.


### Added

- **MCP client** (`tc-mcp`, on `rmcp` 3.3). Servers listed in
  `.truecode/mcp.toml` are started over stdio and their tools offered to the
  model as `mcp__<server>__<tool>`. `truecode mcp` checks the setup without
  starting a session and exits non-zero if a server failed.
- Every MCP tool call is confirmed before it runs, whatever the server's
  `readOnlyHint` claims — see ADR 0011.
- A server that will not start is reported and skipped; the rest of the session
  keeps the tools from the servers that did.

### Changed

- `Tool::name` and `Tool::description` return `&str` rather than `&'static str`,
  so a tool whose name is only known at runtime needs no leak.


### Added

- `-v` / `--verbose` logs true-code's own crates to stderr. `RUST_LOG` still wins.
- `/model` in the TUI: `/model` shows what is in use, `/model <id>` switches and
  keeps the conversation. The id is resolved before the switch, so a typo is
  refused immediately instead of failing as a 404 mid-turn.
- The binary installs under both `truecode` and `true-code`.
- Provider errors now carry advice: 401/403 points at `truecode auth login`, 404
  at `truecode models`, 402 names an empty account, 429 rate limiting, 5xx a
  provider outage, and a 400 mentioning context is named as context overflow.

### Fixed

- `truecode doctor` reported a mistyped model twice — once as the model, once as
  a missing API key — and printed an `ANTHROPIC_API_KEY` hint whatever the
  provider was. The key is now looked up by provider, and the hint names that
  provider's own variable.
- `truecode config` printed the configured model string without resolving it. It
  now shows the provider, what the id resolves to, and says when a model is not
  in the local catalogue and therefore unverified until the first request.


### Fixed

- **Choosing a provider in setup now works.** It writes both the provider *and* a
  matching model to `.truecode/config.toml`, instead of saving the key and then
  failing because the configured model belonged to someone else. That was a dead
  end for exactly the person the setup screen exists for.
- **Gateway model ids resolve.** `inclusionai/ling-3.0-flash-vl:free` — which is
  how OpenRouter actually names models — was being split at the first slash and
  looked up as a vendor called `inclusionai`. The configured provider now decides
  how an id is read, which is the only rule that survives a gateway whose ids look
  exactly like our prefixed form.

### Added

- **`truecode models <filter>`** asks OpenRouter for the **live** list — hundreds
  of models with real prices, `:free` variants marked as free. A compiled-in table
  would be wrong the day it shipped.
- **`truecode update`** compares the commit this binary was built from against the
  repository. It prints the update command rather than running it.
- **`truecode init`** writes a starter `.truecode/constraints.toml`. Every rule is
  commented out: a template that arrives switched on fires on your first change
  for reasons you never chose.
- **`/handoff`** writes a session summary to `.truecode/handoffs/` — what changed,
  what was verified, where it was left. Context degrades long before a window is
  full, and starting fresh is easier when the thread is written down.
- The default system prompt now carries the working practices this project was
  built with: say what you actually verified, name what your change does not
  cover, comments explain why, smallest change that works.

### Earlier in this cycle

- **The comprehension check** (`--learn`). A run that changed code ends with one
  question about that change, then the explanation — shown whether the answer was
  right or wrong, because guessing correctly teaches nothing
  ([ADR 0010](docs/adr/0010-comprehension-check.md)).
- **`truecode learn`** shows which concepts have come up and how they went. No
  score, no streak, no badges: gamification measurably lowered both intrinsic
  motivation and exam performance.
- The profile in `.truecode/learning.toml` feeds past difficulty back into the
  next question, so it prefers something you have struggled with. Differentiation
  on prior knowledge rather than on a "learning style", which is a neuromyth.
- A skipped question records nothing. Counting it as wrong would make the profile
  lie, and the profile decides what gets explained next.
- The question costs a separate model call, billed and displayed like any other.

### Earlier in this cycle

- **OpenRouter support**, and with it every model OpenRouter proxies — including
  ones newer than this build, because any `openrouter/<vendor>/<model>` resolves
  rather than only a fixed list. A model true-code cannot price reports **no
  cost** rather than a guessed one.
- **`truecode auth`** — `login`, `status`, `logout`. Keys go into the OS keyring
  (Credential Manager, Keychain, Secret Service), never into a file true-code
  wrote, and are read without echo so they miss your shell history.
- **A first-run setup screen.** Starting `truecode` with no key now asks which
  provider to use and takes the key in a masked field, instead of dead-ending on
  "set an environment variable".
- `openai/gpt-4.1` added to the catalogue alongside `gpt-4.1-mini`.

### Changed

- Model resolution returns an owned `ModelInfo` rather than a `&'static` one, so
  a gateway's catalogue is not limited to what was compiled in. Pre-1.0 breaking
  change to the library API.
- Environment variables still take precedence over the keyring, so CI stays
  predictable and a temporary override needs no cleanup.

### Earlier in this cycle

- **The proof panel.** Every run now ends with what the harness *observed* —
  files changed, verification commands run, their exit codes — and a verdict
  derived from those facts rather than from the model's summary. Changed code
  that nothing checked is reported as **UNVERIFIED**, in red, with what to do
  about it ([ADR 0009](docs/adr/0009-proof-panel.md)).
- A failing check outranks everything else in the verdict, so a run that broke
  the build cannot be summarised by what went well.
- **`truecode verify`** runs the project's build, test and lint commands. The
  commands come from the project's shape — Cargo.toml, go.mod, pyproject.toml,
  package.json — so it works with no configuration.
- Headless runs exit non-zero on an alarming verdict. A piped run reporting
  success without evidence is the failure this exists to prevent.
- Commands that prove nothing (`git status`, `ls`) are dropped from the panel
  rather than listed, so padding cannot hide the line that matters.

### Earlier in this cycle

- **`truecode doctor`** — checks the model, the API key, the project rules and
  whether the session directory is writable, and names what is missing. A first
  run fails for about four reasons; guessing which one from a stack trace is a bad
  first impression.
- **[docs/INSTALL.md](docs/INSTALL.md)** — a step-by-step install guide including
  Windows specifics, updating, uninstalling, and a troubleshooting section.
- `truecode --help` now ends with worked examples. The first question is always
  "what do I actually type", and a flag list does not answer it.

### Changed

- **The binary is now `truecode`, not `true-code`.** One less hyphen to remember,
  and it matches what the docs tell you to type. The project keeps its name.

### Earlier in this cycle

- **The constraint ledger.** Project rules in `.truecode/constraints.toml` are
  restated to the model on every request *and* checked mechanically against each
  change before it is applied. A violation turns the confirmation modal red and
  lists the broken rule above the diff
  ([ADR 0008](docs/adr/0008-constraint-ledger.md)).
- Three checkable rule shapes — `forbid_added` (scoped by `in_files` /
  `except_files`), `forbid_files`, `forbid_command` — plus `[[reminder]]` for
  anything that cannot be checked, labelled as such everywhere.
- `truecode constraints` lists what is in force and what is only a reminder.
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

- **Undo.** `/undo` in the TUI and `truecode undo` from the shell revert the last
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
- **Headless mode** (`truecode -p "…"`). The answer goes to stdout, the
  accounting to stderr, so piping stays clean.
- **Layered configuration** — defaults, user file, project file, environment,
  flags — with `truecode config` to show what actually took effect, and
  `truecode models` to show the catalogue and its assumed prices.
- **Append-only session event log format** ([ADR 0004](docs/adr/0004-append-only-event-log.md)),
  defined now so that resume and replay are derivations rather than retrofits.
- CI on Windows, Linux and macOS: format, clippy, tests, docs, MSRV and a
  dependency advisory scan.

### Security

- `unsafe` is forbidden across the workspace.
- API keys are read from the environment only, never from a configuration file.

[Unreleased]: https://github.com/philppplik/true-code/commits/main
