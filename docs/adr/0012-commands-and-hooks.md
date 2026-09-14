# 0012 — Reusable prompts and shell hooks

Status: accepted · 2026-09-14 · builds on [0011](0011-mcp-client.md)

## Context

P3's remaining pieces after MCP: slash commands / skills, hooks, and using
true-code from CI. All three are about letting a project shape the harness
without changing its code.

## Decision

### Commands are Markdown files, in someone else's format

`.truecode/commands/<name>.md`, YAML frontmatter with `description` and
`argument-hint`, body using `$ARGUMENTS` and `$1`…`$9`.

This is Claude Code's format, adopted deliberately. The plan's own note says a
compatible format lowers the switching cost, and that only works if we match
theirs rather than asking them to match ours. Someone's existing command files
work here unchanged.

Frontmatter keys we do not know are **ignored, not rejected** — a file written
for another harness with a `model:` key should still run.

**A command is text substitution and nothing else.** No shell execution, no file
inclusion, no conditionals. A prompt file that can run commands is a prompt file
that a pull request can turn into a backdoor, and the value of these is that
they are cheap to read before you trust one.

A placeholder with no matching argument becomes empty rather than staying as a
literal `$2`, which would reach the model as nonsense to guess at. A body with no
placeholders gets the arguments appended on their own line — silently discarding
what someone typed is the worse failure.

### Hooks: two exit codes, one distinction

`.truecode/hooks.toml`, events `pre-tool`, `post-tool`, `stop`, an optional
`matches` regex on the tool name.

**Exit code 2 from a pre-tool hook refuses the call**, and the hook's stderr
becomes the reason the model is given. **Any other non-zero code is reported but
does not block.** A hook that is merely broken — a lint command that is not
installed — must not make the agent unusable, while a hook that deliberately
refuses must be obeyed. Two codes carry that distinction without a config flag.

Pre-tool hooks run **before the preview**, because a preview can touch the
filesystem and a refusal should cost nothing. Post-tool hooks run only after a
tool **succeeded**: linting a change that failed to apply reports problems that
do not exist. Stop hooks run **before the proof panel**, so a hook that runs the
tests becomes evidence the panel reports rather than a footnote after it.

Patterns are compiled at load, so a bad regex is a startup error rather than a
surprise halfway through a run. A 60-second timeout kills a hook that hangs; the
usual cause is a command waiting for input nobody will type.

**Hooks are not a security boundary, and the docs say so.** They run with the
user's full privileges from a file in the repository. Anyone who can land a
commit can run a command on the next session — the same trust model as a
`Makefile` or a git hook. Stated rather than discovered.

### CI is the headless path, already built

`truecode -p` has existed since P0. P3 adds the two setup checks that make it
usable unattended: `truecode mcp` and `truecode doctor` both exit non-zero when
something is wrong, so a workflow can fail before it burns tokens.

## Consequences

Good: a project can ship its own commands and its own guard rails in the
repository, reviewed like any other file. The three enforcement layers now read
clearly as a progression — constraints are mechanical and cannot be argued with,
hooks are arbitrary and can refuse, approval is the human.

Bad: hooks add a shell process per tool call when configured broadly. A
`matches` pattern is the answer, and the docs lead with it, but nothing stops
someone hooking every tool with a slow command.

Also bad: commands are not namespaced, so a project command named `undo` would
be unreachable — built-ins are matched first. That is the right precedence and
the wrong silence; it should warn at load. Not done.

Not covered: no user-level command directory, so commands are per project only.
No `SKILL.md`-style progressive disclosure — these are flat prompts, not skills
with bundled resources. No hook events beyond the three; `session-start` and
`user-prompt` are plausible next ones and were left out rather than guessed at.
