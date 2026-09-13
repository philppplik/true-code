//! End-to-end tests for the change-confirmation flow.
//!
//! The load-bearing guarantee of write mode: nothing reaches the disk that a
//! human has not seen and agreed to.
//!
//! These assert on the **file on disk**, not on the events. An event stream that
//! says "declined" while the bytes changed anyway is exactly the bug worth
//! catching, and only the filesystem can tell you it happened.

mod common;

use std::sync::Arc;

use common::{
    agent_with, answer, call_tool, config, drive, finish_reason, lib_contents, workspace,
};
use tc_agent::{AgentEvent, ApprovalRequest, Approver, Decision, FinishReason, PermissionMode};
use tc_providers::Provider;

use crate::common::ScriptedProvider;

/// An approver that answers from a script and records what it was asked.
#[derive(Debug)]
struct RecordingApprover {
    decision: Decision,
    asked: std::sync::Mutex<Vec<String>>,
}

impl RecordingApprover {
    fn new(decision: Decision) -> Self {
        Self { decision, asked: std::sync::Mutex::new(Vec::new()) }
    }

    fn asked(&self) -> Vec<String> {
        self.asked.lock().expect("the lock is not poisoned").clone()
    }
}

#[async_trait::async_trait]
impl Approver for RecordingApprover {
    async fn approve(&self, request: &ApprovalRequest) -> Decision {
        self.asked.lock().expect("the lock is not poisoned").push(request.summary());
        self.decision
    }
}

/// A turn asking to change `42` into `43`.
fn patch_turn() -> Vec<tc_core::Delta> {
    call_tool(
        "t1",
        "patch",
        serde_json::json!({ "path": "src/lib.rs", "old_string": "42", "new_string": "43" }),
    )
}

/// Runs one prompt with the given approver and permission mode.
async fn run_with(
    approver: Arc<RecordingApprover>,
    mode: PermissionMode,
    dir: &tempfile::TempDir,
    script: Vec<Vec<tc_core::Delta>>,
) -> Vec<AgentEvent> {
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(script));
    let config = config(5.0);
    let mut agent = agent_with(provider, mode, dir.path(), &config, approver);
    drive(&mut agent, "change the answer").await
}

#[tokio::test]
async fn a_declined_write_never_reaches_the_disk() {
    let dir = workspace();
    let before = lib_contents(&dir);
    let approver = Arc::new(RecordingApprover::new(Decision::Deny));

    let events = run_with(
        approver.clone(),
        PermissionMode::Write,
        &dir,
        vec![patch_turn(), answer("Understood, I left it alone.")],
    )
    .await;

    assert_eq!(lib_contents(&dir), before, "a refused change must not modify the file");
    assert_eq!(approver.asked().len(), 1, "the user was asked exactly once");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::ToolDeclined { .. })));
}

#[tokio::test]
async fn an_approved_write_is_applied() {
    let dir = workspace();
    let approver = Arc::new(RecordingApprover::new(Decision::Approve));

    run_with(
        approver.clone(),
        PermissionMode::Write,
        &dir,
        vec![patch_turn(), answer("Changed it.")],
    )
    .await;

    let after = lib_contents(&dir);
    assert!(after.contains("43"), "the approved change must be applied: {after}");
    assert_eq!(
        approver.asked()[0],
        "modify src/lib.rs  +1 −1",
        "the prompt must state exactly what changes"
    );
}

#[tokio::test]
async fn approving_a_tool_for_the_session_stops_the_asking() {
    let dir = workspace();
    let approver = Arc::new(RecordingApprover::new(Decision::ApproveToolForSession));

    // Two different patches, so loop detection stays out of it.
    let script = vec![
        patch_turn(),
        call_tool(
            "t2",
            "patch",
            serde_json::json!({ "path": "src/lib.rs", "old_string": "43", "new_string": "44" }),
        ),
        answer("Done."),
    ];

    run_with(approver.clone(), PermissionMode::Write, &dir, script).await;

    assert_eq!(approver.asked().len(), 1, "blanket approval must not ask again");
    assert!(lib_contents(&dir).contains("44"), "both changes were applied");
}

#[tokio::test]
async fn aborting_at_the_prompt_ends_the_run_without_writing() {
    let dir = workspace();
    let approver = Arc::new(RecordingApprover::new(Decision::Abort));

    let events =
        run_with(approver, PermissionMode::Write, &dir, vec![patch_turn(), answer("unreachable")])
            .await;

    assert_eq!(finish_reason(&events), Some(FinishReason::Declined));
    assert!(lib_contents(&dir).contains("42"), "nothing may be written on abort");
}

#[tokio::test]
async fn read_only_mode_never_offers_a_way_to_write() {
    let dir = workspace();
    let approver = Arc::new(RecordingApprover::new(Decision::Approve));

    let events = run_with(
        approver.clone(),
        PermissionMode::ReadOnly,
        &dir,
        vec![patch_turn(), answer("I cannot write.")],
    )
    .await;

    assert!(approver.asked().is_empty(), "an unavailable tool must not reach the prompt");
    assert!(
        events.iter().any(|event| matches!(
            event,
            AgentEvent::ToolFinished { tool, is_error: true } if tool == "patch"
        )),
        "the model is told the tool does not exist: {events:?}"
    );
    assert!(lib_contents(&dir).contains("42"));
}

#[tokio::test]
async fn shell_is_unavailable_below_full_mode() {
    let dir = workspace();
    let approver = Arc::new(RecordingApprover::new(Decision::Approve));

    let events = run_with(
        approver.clone(),
        PermissionMode::Write,
        &dir,
        vec![
            call_tool("t1", "shell", serde_json::json!({ "command": "echo hi" })),
            answer("I cannot run commands."),
        ],
    )
    .await;

    assert!(approver.asked().is_empty(), "write mode must not be able to run commands");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolFinished { tool, is_error: true } if tool == "shell"
    )));
}

#[tokio::test]
async fn a_write_that_changes_nothing_does_not_ask() {
    let dir = workspace();
    let current = lib_contents(&dir);
    let approver = Arc::new(RecordingApprover::new(Decision::Deny));

    run_with(
        approver.clone(),
        PermissionMode::Write,
        &dir,
        vec![
            call_tool(
                "t1",
                "write_file",
                serde_json::json!({ "path": "src/lib.rs", "content": current }),
            ),
            answer("Nothing to do."),
        ],
    )
    .await;

    assert!(
        approver.asked().is_empty(),
        "a no-op write must not spend a confirmation the user learns to dismiss"
    );
}

#[tokio::test]
async fn a_patch_that_cannot_apply_never_reaches_the_prompt() {
    let dir = workspace();
    let approver = Arc::new(RecordingApprover::new(Decision::Approve));

    let events = run_with(
        approver.clone(),
        PermissionMode::Write,
        &dir,
        vec![
            call_tool(
                "t1",
                "patch",
                serde_json::json!({
                    "path": "src/lib.rs",
                    "old_string": "not present anywhere",
                    "new_string": "x",
                }),
            ),
            answer("I misread the file."),
        ],
    )
    .await;

    assert!(approver.asked().is_empty(), "a failing preview must not ask the user anything");
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolFinished { tool, is_error: true } if tool == "patch"
    )));
}

#[tokio::test]
async fn a_write_outside_the_workspace_is_refused_before_the_prompt() {
    let dir = workspace();
    let approver = Arc::new(RecordingApprover::new(Decision::Approve));

    let events = run_with(
        approver.clone(),
        PermissionMode::Write,
        &dir,
        vec![
            call_tool(
                "t1",
                "write_file",
                serde_json::json!({ "path": "../../escaped.txt", "content": "owned" }),
            ),
            answer("I cannot write there."),
        ],
    )
    .await;

    assert!(
        approver.asked().is_empty(),
        "the workspace boundary holds without needing a human to notice"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        AgentEvent::ToolFinished { tool, is_error: true } if tool == "write_file"
    )));
}

#[tokio::test]
async fn the_approval_decision_is_recorded_in_the_session_log() {
    let dir = workspace();
    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![patch_turn(), answer("Done.")]));
    let config = config(5.0);

    let mut agent = agent_with(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(RecordingApprover::new(Decision::Deny)),
    );
    let log_path = agent.log().path().expect("the log is enabled").to_path_buf();

    drive(&mut agent, "change the answer").await;

    let log = std::fs::read_to_string(&log_path).expect("the log is readable");
    assert!(log.contains("approval_decided"), "the audit trail must record who allowed what");
    assert!(log.contains("\"approved\":false"), "unexpected log:\n{log}");
}
