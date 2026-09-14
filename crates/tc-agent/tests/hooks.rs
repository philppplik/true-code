//! End-to-end tests for hooks.
//!
//! The unit tests in `hooks.rs` prove exit code 2 produces a `Refused`. These
//! prove what actually matters: that a refusal reaches the agent loop and the
//! file on disk is left alone. A hook that reports a refusal nobody enforces is
//! worse than no hook, because it looks like a guard rail.

mod common;

use std::sync::Arc;

use common::{ScriptedProvider, answer, call_tool, config, drive, lib_contents, workspace};
use tc_agent::approval::ApproveAll;
use tc_agent::hooks::Hooks;
use tc_agent::{Agent, AgentSetup, HookSet, PermissionMode, SessionLog};
use tc_core::SessionId;
use tc_providers::Provider;
use tc_tools::{Ledger, ToolContext, ToolSet};

/// The contents written by [`common::workspace`].
const ORIGINAL: &str = "pub fn answer() -> u32 { 42 }\n";

/// Builds an agent whose hooks come from a TOML snippet.
fn agent_with_hooks(
    provider: Arc<dyn Provider>,
    root: &std::path::Path,
    config: &tc_config::Config,
    hooks: &str,
) -> Agent {
    let parsed: Hooks = toml::from_str(hooks).expect("the hook list parses");
    Agent::new(
        AgentSetup {
            hooks: HookSet::compile(parsed).expect("the patterns compile"),
            provider,
            tools: ToolSet::for_mode(PermissionMode::Write),
            tool_ctx: ToolContext::new(root),
            ledger: Ledger::default(),
            log: SessionLog::create(root, SessionId::new()),
            approver: Arc::new(ApproveAll),
            mode: PermissionMode::Write,
            teaching: false,
            profile: tc_agent::Profile::default(),
        },
        config,
    )
}

/// A model that tries to overwrite `src/lib.rs`, then gives up.
fn tries_to_write() -> Arc<dyn Provider> {
    Arc::new(ScriptedProvider::new(vec![
        call_tool(
            "c1",
            "write_file",
            serde_json::json!({ "path": "src/lib.rs", "content": "pub fn answer() -> u32 { 0 }\n" }),
        ),
        answer("I could not write it."),
    ]))
}

#[tokio::test]
async fn a_pre_tool_hook_that_refuses_stops_the_write_from_happening() {
    // The whole point: not that a Refused value was produced, but that the file
    // on disk is untouched afterwards.
    let dir = workspace();
    let config = config(1.0);
    let mut agent = agent_with_hooks(
        tries_to_write(),
        dir.path(),
        &config,
        "[[hook]]\nevent = \"pre-tool\"\nmatches = \"write_file\"\ncommand = \"exit 2\"",
    );

    drive(&mut agent, "change the answer").await;

    assert_eq!(lib_contents(&dir), ORIGINAL, "the hook refused, so nothing may have been written");
}

#[tokio::test]
async fn the_model_is_told_why_the_call_was_refused() {
    // Without the reason, the model's next move is a guess.
    let dir = workspace();
    let config = config(1.0);
    let command = if cfg!(windows) {
        "echo not during a release >&2 & exit 2"
    } else {
        "echo 'not during a release' >&2; exit 2"
    };
    let mut agent = agent_with_hooks(
        tries_to_write(),
        dir.path(),
        &config,
        &format!("[[hook]]\nevent = \"pre-tool\"\ncommand = \"{command}\""),
    );

    drive(&mut agent, "change the answer").await;

    let told = transcript(&agent);

    assert!(
        told.contains("not during a release"),
        "the hook's own words must reach the model, got {told}"
    );
}

/// Everything the model was told, flattened.
fn transcript(agent: &Agent) -> String {
    format!("{:?}", agent.history())
}

#[tokio::test]
async fn a_hook_for_a_different_tool_does_not_interfere() {
    let dir = workspace();
    let config = config(1.0);
    let mut agent = agent_with_hooks(
        tries_to_write(),
        dir.path(),
        &config,
        "[[hook]]\nevent = \"pre-tool\"\nmatches = \"shell\"\ncommand = \"exit 2\"",
    );

    drive(&mut agent, "change the answer").await;

    assert_ne!(lib_contents(&dir), ORIGINAL, "a hook scoped to `shell` must not block a write");
}

#[tokio::test]
async fn a_merely_broken_hook_does_not_block_the_agent() {
    // Someone's lint command being missing must not make the tool unusable.
    let dir = workspace();
    let config = config(1.0);
    let mut agent = agent_with_hooks(
        tries_to_write(),
        dir.path(),
        &config,
        "[[hook]]\nevent = \"pre-tool\"\ncommand = \"exit 1\"",
    );

    drive(&mut agent, "change the answer").await;

    assert_ne!(lib_contents(&dir), ORIGINAL, "exit 1 reports a problem; it does not refuse");
}

#[tokio::test]
async fn a_post_tool_hooks_findings_reach_the_model() {
    let dir = workspace();
    let config = config(1.0);
    let command = if cfg!(windows) {
        "echo clippy is unhappy >&2 & exit 1"
    } else {
        "echo 'clippy is unhappy' >&2; exit 1"
    };
    let mut agent = agent_with_hooks(
        tries_to_write(),
        dir.path(),
        &config,
        &format!("[[hook]]\nevent = \"post-tool\"\ncommand = \"{command}\""),
    );

    drive(&mut agent, "change the answer").await;

    assert_ne!(lib_contents(&dir), ORIGINAL, "a post-tool hook runs after the write, not instead");

    let told = transcript(&agent);
    assert!(
        told.contains("clippy is unhappy"),
        "the model must learn what the hook found, got {told}"
    );
}
