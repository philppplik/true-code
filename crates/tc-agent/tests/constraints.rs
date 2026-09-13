//! End-to-end tests for the constraint ledger.
//!
//! The measured problem these exist for: CLI agents violate explicitly stated
//! user constraints in roughly half of their failures. A rule that only lives in
//! the prompt is a wish. These tests check that a rule written down is actually
//! enforced at the gate, and — just as important — that a *clean* change is not
//! held up by one.

mod common;

use std::sync::Arc;

use common::{agent_with_ledger, answer, call_tool, config, drive, lib_contents, workspace};
use tc_agent::approval::ApproveAll;
use tc_agent::{AgentEvent, ApprovalRequest, Approver, Decision, PermissionMode};
use tc_providers::Provider;
use tc_tools::Ledger;

use crate::common::ScriptedProvider;

/// The original contents written by [`common::workspace`].
const ORIGINAL: &str = "pub fn answer() -> u32 { 42 }\n";

/// Writes a ledger into the workspace and loads it.
fn ledger_in(dir: &tempfile::TempDir, source: &str) -> Ledger {
    std::fs::create_dir_all(dir.path().join(".truecode")).expect("dir is creatable");
    std::fs::write(dir.path().join(tc_tools::constraints::CONSTRAINTS_FILE), source)
        .expect("file is writable");
    Ledger::load(dir.path()).expect("the ledger is valid")
}

/// An approver that records what it was shown.
#[derive(Debug, Default)]
struct Watcher {
    seen: std::sync::Mutex<Vec<ApprovalRequest>>,
}

impl Watcher {
    fn seen(&self) -> Vec<ApprovalRequest> {
        self.seen.lock().expect("the lock is not poisoned").clone()
    }
}

#[async_trait::async_trait]
impl Approver for Watcher {
    async fn approve(&self, request: &ApprovalRequest) -> Decision {
        self.seen.lock().expect("the lock is not poisoned").push(request.clone());
        Decision::Approve
    }
}

/// A turn that adds an `unwrap()` to `src/lib.rs`.
fn adds_unwrap() -> Vec<tc_core::Delta> {
    call_tool(
        "t1",
        "write_file",
        serde_json::json!({
            "path": "src/lib.rs",
            "content": "pub fn answer() -> u32 { load().unwrap() }\n",
        }),
    )
}

/// The rule that forbids it.
const NO_UNWRAP: &str = r#"
[[constraint]]
description = "No unwrap() in production code"
forbid_added = '\.unwrap\(\)'
in_files = ["**/*.rs"]
except_files = ["**/tests/**"]
"#;

#[tokio::test]
async fn a_violation_is_shown_to_the_user_before_the_change_is_applied() {
    let dir = workspace();
    let ledger = ledger_in(&dir, NO_UNWRAP);
    let approver = Arc::new(Watcher::default());
    let config = config(5.0);

    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![adds_unwrap(), answer("Done.")]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        approver.clone(),
        ledger,
    );

    drive(&mut agent, "make it compile").await;

    let seen = approver.seen();
    assert_eq!(seen.len(), 1, "the user was asked");
    assert!(seen[0].breaks_a_rule(), "the prompt must carry the violation");
    assert!(
        seen[0].violations[0].evidence.contains("unwrap"),
        "unexpected evidence: {:?}",
        seen[0].violations
    );
}

#[tokio::test]
async fn a_clean_change_is_not_held_up_by_an_unrelated_rule() {
    let dir = workspace();
    let ledger = ledger_in(&dir, NO_UNWRAP);
    let approver = Arc::new(Watcher::default());
    let config = config(5.0);

    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        call_tool(
            "t1",
            "patch",
            serde_json::json!({
                "path": "src/lib.rs", "old_string": "42", "new_string": "43",
            }),
        ),
        answer("Done."),
    ]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        approver.clone(),
        ledger,
    );

    drive(&mut agent, "change the answer").await;

    assert!(!approver.seen()[0].breaks_a_rule(), "a false positive would poison every prompt");
    assert!(lib_contents(&dir).contains("43"), "the change still applies");
}

#[tokio::test]
async fn auto_approval_refuses_a_change_that_breaks_a_rule() {
    let dir = workspace();
    let ledger = ledger_in(&dir, NO_UNWRAP);
    let config = config(5.0);

    // --yes in CI means "do not ask me about routine changes", not "ignore the
    // rules I wrote down". Unattended is exactly where this matters most.
    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![adds_unwrap(), answer("Blocked.")]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        ledger,
    );

    let events = drive(&mut agent, "make it compile").await;

    assert_eq!(lib_contents(&dir), ORIGINAL, "nothing may be written");
    assert!(events.iter().any(|event| matches!(event, AgentEvent::ToolDeclined { .. })));
}

#[tokio::test]
async fn auto_approval_still_allows_a_clean_change() {
    let dir = workspace();
    let ledger = ledger_in(&dir, NO_UNWRAP);
    let config = config(5.0);

    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        call_tool(
            "t1",
            "patch",
            serde_json::json!({
                "path": "src/lib.rs", "old_string": "42", "new_string": "43",
            }),
        ),
        answer("Done."),
    ]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        ledger,
    );

    drive(&mut agent, "change the answer").await;

    assert!(lib_contents(&dir).contains("43"), "the ledger must not block everything");
}

#[tokio::test]
async fn a_forbidden_command_is_caught_before_it_runs() {
    let dir = workspace();
    let ledger = ledger_in(
        &dir,
        r#"
        [[constraint]]
        description = "Never publish from an agent"
        forbid_command = 'cargo publish'
        "#,
    );
    let config = config(5.0);
    let marker = dir.path().join("published.txt");

    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        call_tool(
            "t1",
            "shell",
            serde_json::json!({
                "command": format!("cargo publish > {}", marker.display()),
            }),
        ),
        answer("Refused."),
    ]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Full,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        ledger,
    );

    drive(&mut agent, "publish it").await;

    assert!(!marker.exists(), "the command must never have run");
}

#[tokio::test]
async fn the_violation_is_recorded_in_the_session_log() {
    let dir = workspace();
    let ledger = ledger_in(&dir, NO_UNWRAP);
    let config = config(5.0);

    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![adds_unwrap(), answer("Done.")]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(Watcher::default()),
        ledger,
    );
    let log_path = agent.log().path().expect("the log is enabled").to_path_buf();

    drive(&mut agent, "make it compile").await;

    let log = std::fs::read_to_string(&log_path).expect("the log is readable");
    assert!(
        log.contains("constraint_violated"),
        "'I was warned and said yes' must be distinguishable from 'nobody noticed'"
    );
}

#[tokio::test]
async fn approving_anyway_tells_the_model_it_broke_a_rule() {
    let dir = workspace();
    let ledger = ledger_in(&dir, NO_UNWRAP);
    let config = config(5.0);

    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![adds_unwrap(), answer("Done.")]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(Watcher::default()),
        ledger,
    );

    drive(&mut agent, "make it compile").await;

    // The tool result the model receives is the only channel that stops it
    // reading one approval as standing permission.
    let log_path = agent.log().path().expect("the log is enabled");
    let log = std::fs::read_to_string(log_path).expect("the log is readable");
    assert!(log.contains("constraint_violated"));
    assert!(lib_contents(&dir).contains("unwrap"), "the user allowed it, so it applied");
}

#[tokio::test]
async fn a_project_without_rules_behaves_exactly_as_before() {
    let dir = workspace();
    let config = config(5.0);

    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![adds_unwrap(), answer("Done.")]));
    let mut agent = agent_with_ledger(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(ApproveAll),
        Ledger::default(),
    );

    drive(&mut agent, "make it compile").await;

    assert!(lib_contents(&dir).contains("unwrap"), "no rules means nothing to enforce");
}
