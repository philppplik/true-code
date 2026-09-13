# Changelog

All notable changes to this project are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and
this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
While the version is `0.x`, breaking changes can land in a minor release.

## [Unreleased]

### Added

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
