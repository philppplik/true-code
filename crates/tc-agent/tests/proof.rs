//! End-to-end tests for the proof panel.
//!
//! The claim this feature makes is narrow and absolute: true-code reports what it
//! **observed**, never what the model said about it. These tests exist mainly to
//! pin down the uncomfortable case — a run that changed code and proved nothing
//! must say so, however confidently the model wrote its answer.

mod common;

use std::sync::Arc;

use common::{agent_with, answer, call_tool, config, drive, workspace};
use tc_agent::approval::ApproveAll;
use tc_agent::{AgentEvent, PermissionMode, Proof, Verdict};
use tc_providers::Provider;

use crate::common::ScriptedProvider;

/// Extracts the proof panel from a run's events.
fn proof_of(events: &[AgentEvent]) -> Option<Proof> {
    events.iter().find_map(|event| match event {
        AgentEvent::Proven { proof } => Some(proof.clone()),
        _ => None,
    })
}

/// A turn that edits `src/lib.rs`.
fn edits_lib() -> Vec<tc_core::Delta> {
    call_tool(
        "t1",
        "patch",
        serde_json::json!({ "path": "src/lib.rs", "old_string": "42", "new_string": "43" }),
    )
}

/// A turn that runs a command exiting with `code`.
fn runs(id: &str, command: &str) -> Vec<tc_core::Delta> {
    call_tool(id, "shell", serde_json::json!({ "command": command }))
}

/// Runs a script in the given mode with everything approved.
async fn run(
    dir: &tempfile::TempDir,
    mode: PermissionMode,
    script: Vec<Vec<tc_core::Delta>>,
) -> Vec<AgentEvent> {
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(script));
    let config = config(5.0);
    let mut agent = agent_with(provider, mode, dir.path(), &config, Arc::new(ApproveAll));
    drive(&mut agent, "do the thing").await
}

#[tokio::test]
async fn a_change_with_no_checks_is_reported_as_unverified() {
    let dir = workspace();

    // The model sounds certain. The panel does not care.
    let events = run(
        &dir,
        PermissionMode::Write,
        vec![edits_lib(), answer("Done — everything works correctly now.")],
    )
    .await;

    let proof = proof_of(&events).expect("a panel is produced");
    assert_eq!(proof.verdict(), Verdict::Unverified, "a confident sentence is not evidence");
    assert_eq!(proof.changed, vec!["src/lib.rs".to_owned()]);
    assert!(proof.checks.is_empty());
}

#[tokio::test]
async fn a_passing_test_command_earns_a_verified_verdict() {
    let dir = workspace();

    let events = run(
        &dir,
        PermissionMode::Full,
        vec![edits_lib(), runs("t2", "echo running cargo test"), answer("Changed and tested.")],
    )
    .await;

    let proof = proof_of(&events).expect("a panel is produced");
    assert_eq!(proof.verdict(), Verdict::Verified);
    assert_eq!(proof.checks.len(), 1);
    assert!(proof.checks[0].passed());
}

#[tokio::test]
async fn a_failing_check_is_reported_however_the_model_summarises_it() {
    let dir = workspace();
    // `exit 1` after a word the classifier reads as a test.
    let failing = if cfg!(windows) { "echo test && exit 1" } else { "echo test; exit 1" };

    let events = run(
        &dir,
        PermissionMode::Full,
        vec![edits_lib(), runs("t2", failing), answer("All good!")],
    )
    .await;

    let proof = proof_of(&events).expect("a panel is produced");
    assert_eq!(
        proof.verdict(),
        Verdict::Failing,
        "the exit code decides, not the answer text: {:?}",
        proof.checks
    );
}

#[tokio::test]
async fn a_command_that_proves_nothing_earns_no_credit() {
    let dir = workspace();

    let events = run(
        &dir,
        PermissionMode::Full,
        vec![edits_lib(), runs("t2", "echo hello"), answer("Done.")],
    )
    .await;

    let proof = proof_of(&events).expect("a panel is produced");
    assert!(proof.checks.is_empty(), "an unrelated command is not evidence");
    assert_eq!(proof.verdict(), Verdict::Unverified);
}

#[tokio::test]
async fn a_read_only_run_that_changes_nothing_produces_no_panel() {
    let dir = workspace();

    let events = run(
        &dir,
        PermissionMode::ReadOnly,
        vec![
            call_tool("t1", "read_file", serde_json::json!({ "path": "src/lib.rs" })),
            answer("It returns 42."),
        ],
    )
    .await;

    assert!(proof_of(&events).is_none(), "an empty panel after a question is noise");
}

#[tokio::test]
async fn the_panel_arrives_before_the_run_is_declared_finished() {
    let dir = workspace();

    let events = run(&dir, PermissionMode::Write, vec![edits_lib(), answer("Done.")]).await;

    let proven = events
        .iter()
        .position(|event| matches!(event, AgentEvent::Proven { .. }))
        .expect("a panel is produced");
    let finished = events
        .iter()
        .position(|event| matches!(event, AgentEvent::Finished { .. }))
        .expect("the run finishes");

    assert!(proven < finished, "'it stopped' must never be read before 'here is what it did'");
}

#[tokio::test]
async fn a_guard_rail_stop_still_produces_a_panel() {
    let dir = workspace();

    // The budget runs out after the edit. Evidence of what already happened must
    // survive an abrupt ending — that is when it matters most.
    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![edits_lib(), edits_lib()]));
    let config = config(0.0005);
    let mut agent =
        agent_with(provider, PermissionMode::Write, dir.path(), &config, Arc::new(ApproveAll));

    let events = drive(&mut agent, "keep going").await;

    let proof = proof_of(&events).expect("a panel is produced even on an abrupt stop");
    assert_eq!(proof.changed, vec!["src/lib.rs".to_owned()]);
}

#[tokio::test]
async fn each_prompt_gets_its_own_panel() {
    let dir = workspace();
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        edits_lib(),
        answer("Changed it."),
        answer("Nothing more to do."),
    ]));
    let config = config(5.0);
    let mut agent =
        agent_with(provider, PermissionMode::Write, dir.path(), &config, Arc::new(ApproveAll));

    let first = drive(&mut agent, "change it").await;
    let second = drive(&mut agent, "anything else?").await;

    assert_eq!(proof_of(&first).expect("a panel").changed.len(), 1);
    assert!(
        proof_of(&second).is_none(),
        "the second prompt changed nothing, so it must not inherit the first one's credit"
    );
}
