# 0011 — MCP client, and why every foreign tool is confirmed

Status: accepted · 2026-09-14 · supersedes nothing

## Context

P3 is extensibility, and its acceptance criterion is that a server true-code did
not write works. MCP is the way that happens: it is the de-facto standard, the
server ecosystem is large, and there is an official Rust SDK (`rmcp`, now at
3.3, MSRV 1.88 — the plan's "2.0" is out of date).

The interesting question is not how to speak the protocol. It is what authority
a foreign server has once it is speaking.

MCP defines a `readOnlyHint` annotation on tools, and the specification is
explicit that annotations are **hints** which are "not guaranteed to provide a
faithful description of tool behavior". Anyone can publish a server. A server
that wants its tool to run unattended has only to claim the tool is read-only.

## Decision

**Client only, never a server.** true-code starts other people's servers over
stdio. It does not expose itself as one. That is a different product with a
different threat model, and P3 does not ask for it.

**Every MCP tool is treated as mutating.** `is_read_only()` returns false
unconditionally and `preview()` returns `Effect::Execute` with
`Risk::High { reason: "an MCP server decides what this does" }`. The hint is not
read. High rather than moderate because the risk is not that a particular call
is known to be dangerous — it is that nothing in this process knows what it does
at all.

**Tools are namespaced `mcp__<server>__<tool>`, on the user's name for the
server**, taken from `mcp.toml` rather than from the server's self-report. Two
servers offering `search` stay distinguishable, and no server can name a tool
`read_file` and shadow the built-in one. `ToolSet::extend` appends, so a
built-in is always found first regardless.

**A separate `.truecode/mcp.toml`.** The server list is what people paste from a
README. Keeping it out of `config.toml` means a bad paste cannot take the model
and budget settings with it. `deny_unknown_fields` is on: a mistyped key is
refused rather than silently dropped, because a tool that never appears is
harder to diagnose than a parse error.

**No `${VAR}` expansion in `env`.** Values are literal. A config file that can
read the ambient environment is a config file that can forward a credential to a
server the user did not inspect. Someone who genuinely wants to pass a secret can
export it before starting true-code; that is a deliberate act, in the shell,
where they can see it.

**One failed server is not a failed session.** Failures are collected per server
and reported on stderr; the tools from the servers that did start are still
offered. A missing `npx` costs one server's tools, not the run.

## Consequences

Good: any of the existing MCP servers works without true-code knowing anything
about it. Nothing is hardcoded — the plan's rule that web, DB, Jira and browser
access arrive "exclusively over MCP" is now enforceable rather than aspirational.
`truecode mcp` checks a setup without starting a session and exits non-zero on
failure, so it can be a CI step.

Bad: a chatty MCP server means a confirmation per call, and there is currently no
per-tool "always allow" that persists across sessions. That is a real cost and
the honest trade for not trusting a self-report. `a` (always, this session) in
the approval prompt already takes the edge off.

Also bad: MCP tool schemas are passed to the model unchanged and unbudgeted. Ten
servers will cost real context on every request. There is no limit yet; when one
is needed it belongs next to the tool list, not inside this crate.

Verified against `@modelcontextprotocol/server-filesystem` on Windows: 14 tools,
correctly namespaced, with no shadowing of the built-in `read_file`. Doing so
turned up a Windows bug worth recording — Rust's `Command::new` does not apply
`PATHEXT`, so `npx` (installed as `npx.cmd`) reported "program not found" on a
machine where `npx --version` worked in the same terminal. `which::resolve`
performs the lookup the shell would. For a Windows-first project that was not a
papercut.

Not covered: stdio transport only. HTTP/SSE servers, MCP resources, prompts and
sampling are all unimplemented. Resources and prompts are the likely next step;
sampling — letting a server ask *our* model for a completion — inverts the trust
direction and should not be added without its own decision.
