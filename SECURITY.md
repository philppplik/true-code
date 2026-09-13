# Security Policy

## Supported versions

true-code is pre-1.0 and moves fast. Only the latest commit on `main` is
supported. There are no backports.

## Reporting a vulnerability

Use GitHub's [private vulnerability reporting](https://github.com/philppplik/true-code/security/advisories/new).
Please do not open a public issue for a security problem.

Include what you did, what happened, and what you expected. A minimal
reproduction is worth more than a long description.

Expect an acknowledgement within a week. This is a small project — if the
timeline slips, that is capacity, not indifference.

**Never include an API key, token or session log in a report.** Session logs can
contain excerpts of your source code and, in some configurations, credentials.
Redact before sending, and rotate anything you may have exposed.

## Threat model

A coding agent runs untrusted input through a program that can write files and
execute commands. The threats we consider in scope:

| Threat | Why it matters here |
|---|---|
| **Prompt injection** | Repository contents, web pages and issue text are *untrusted data*. Text inside a file must never be able to act as an instruction. |
| **Credential exposure** | API keys must never reach a config file, a log, the session event log, or a crash report. |
| **Path traversal** | A tool call must not escape the project root, including via symlinks. |
| **Destructive commands** | Irreversible operations require a human decision, in every mode. |
| **Supply chain** | Dependencies are scanned on every push; `unsafe` is forbidden workspace-wide. |

Out of scope: attacks that require an attacker to already control your machine or
your shell environment.

## Design commitments

These are architectural, not aspirational — a change that breaks one of them will
not be merged:

- `unsafe_code = "forbid"` across the workspace.
- API keys are read from the environment only, never from a file true-code writes.
- Nothing is written to your project without you seeing it first.
- Destructive operations are never auto-approved, including in unattended modes.

Several of these are enforcements that P0 does not yet need — the tool layer does
not exist yet. They are stated now so that the code that comes later has to meet
them, rather than being retrofitted.
