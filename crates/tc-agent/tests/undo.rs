//! End-to-end tests for checkpoints and undo.
//!
//! Confirmation lets you see a change coming; undo is what you reach for when you
//! approved it anyway. These tests assert on the **file on disk**, because that is
//! the only thing that can tell you whether a revert actually reverted.

mod common;

use std::sync::Arc;

use common::{agent_with, answer, call_tool, config, drive, lib_contents, workspace};
use tc_agent::approval::ApproveAll;
use tc_agent::checkpoint::{last_undoable, newest_session};
use tc_agent::{Agent, PermissionMode, SessionLog, UndoError};
use tc_core::Delta;
use tc_providers::Provider;

use crate::common::ScriptedProvider;

/// The original contents written by [`common::workspace`].
const ORIGINAL: &str = "pub fn answer() -> u32 { 42 }\n";

/// A turn that patches `42` into the given replacement.
fn patch_to(id: &str, replacement: &str) -> Vec<Delta> {
    call_tool(
        id,
        "patch",
        serde_json::json!({
            "path": "src/lib.rs",
            "old_string": "42",
            "new_string": replacement,
        }),
    )
}

/// Builds an agent in write mode that approves everything.
fn writing_agent(
    dir: &tempfile::TempDir,
    script: Vec<Vec<Delta>>,
    config: &tc_config::Config,
) -> Agent {
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(script));
    agent_with(provider, PermissionMode::Write, dir.path(), config, Arc::new(ApproveAll))
}

#[tokio::test]
async fn undo_restores_the_file_the_agent_changed() {
    let dir = workspace();
    let config = config(5.0);
    let mut agent = writing_agent(&dir, vec![patch_to("t1", "43"), answer("Changed it.")], &config);

    drive(&mut agent, "change the answer").await;
    assert!(lib_contents(&dir).contains("43"), "the change was applied");

    let message = agent.undo_last().expect("there is something to undo");

    assert_eq!(lib_contents(&dir), ORIGINAL, "the file must be exactly as it was");
    assert!(message.contains("src/lib.rs"), "unexpected message: {message}");
}

#[tokio::test]
async fn undo_walks_backwards_one_change_at_a_time() {
    let dir = workspace();
    let config = config(5.0);
    let mut agent = writing_agent(
        &dir,
        vec![
            patch_to("t1", "43"),
            call_tool(
                "t2",
                "patch",
                serde_json::json!({
                    "path": "src/lib.rs", "old_string": "43", "new_string": "44",
                }),
            ),
            answer("Done."),
        ],
        &config,
    );

    drive(&mut agent, "change it twice").await;
    assert!(lib_contents(&dir).contains("44"));

    agent.undo_last().expect("the second change is undone");
    assert!(lib_contents(&dir).contains("43"), "one step back, not all the way");

    agent.undo_last().expect("the first change is undone");
    assert_eq!(lib_contents(&dir), ORIGINAL, "two steps back reaches the start");
}

#[tokio::test]
async fn undoing_a_created_file_removes_it_again() {
    let dir = workspace();
    let config = config(5.0);
    let mut agent = writing_agent(
        &dir,
        vec![
            call_tool(
                "t1",
                "write_file",
                serde_json::json!({
                    "path": "src/new.rs", "content": "pub fn added() {}\n",
                }),
            ),
            answer("Created it."),
        ],
        &config,
    );

    drive(&mut agent, "add a module").await;
    assert!(dir.path().join("src/new.rs").exists(), "the file was created");

    let message = agent.undo_last().expect("there is something to undo");

    assert!(!dir.path().join("src/new.rs").exists(), "a created file must be removed");
    assert!(message.contains("created"), "the message must say what happened: {message}");
}

#[tokio::test]
async fn undo_reports_clearly_when_there_is_nothing_to_undo() {
    let dir = workspace();
    let config = config(5.0);
    let mut agent = writing_agent(&dir, vec![answer("I did not change anything.")], &config);

    drive(&mut agent, "just tell me about it").await;

    let error = agent.undo_last().expect_err("nothing was changed");

    assert!(matches!(error, UndoError::NothingToUndo));
    assert!(error.to_string().contains("not changed any files"), "unexpected: {error}");
}

#[tokio::test]
async fn a_declined_change_leaves_nothing_to_undo() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> =
        Arc::new(ScriptedProvider::new(vec![patch_to("t1", "43"), answer("Left alone.")]));
    let mut agent = agent_with(
        provider,
        PermissionMode::Write,
        dir.path(),
        &config,
        Arc::new(tc_agent::DenyAll),
    );

    drive(&mut agent, "change the answer").await;

    // No checkpoint is taken for a change that never happened, so undo must not
    // offer to "revert" a file to contents it already has.
    assert!(matches!(agent.undo_last(), Err(UndoError::NothingToUndo)));
    assert_eq!(lib_contents(&dir), ORIGINAL);
}

#[tokio::test]
async fn a_read_only_session_never_records_a_checkpoint() {
    let dir = workspace();
    let config = config(5.0);
    let provider: Arc<dyn Provider> = Arc::new(ScriptedProvider::new(vec![
        call_tool("t1", "read_file", serde_json::json!({ "path": "src/lib.rs" })),
        answer("It returns 42."),
    ]));
    let mut agent =
        agent_with(provider, PermissionMode::ReadOnly, dir.path(), &config, Arc::new(ApproveAll));

    drive(&mut agent, "what does it return?").await;

    let log = agent.log().path().expect("the log is enabled");
    let raw = std::fs::read_to_string(log).expect("the log is readable");
    assert!(!raw.contains("checkpointed"), "reading changes nothing, so nothing is saved");
}

#[tokio::test]
async fn the_checkpoint_is_recorded_before_the_change_is_applied() {
    let dir = workspace();
    let config = config(5.0);
    let mut agent = writing_agent(&dir, vec![patch_to("t1", "43"), answer("Done.")], &config);
    let log_path = agent.log().path().expect("the log is enabled").to_path_buf();

    drive(&mut agent, "change the answer").await;

    let raw = std::fs::read_to_string(&log_path).expect("the log is readable");
    let checkpointed = raw.find("checkpointed").expect("a checkpoint was recorded");
    let completed = raw.find("tool_completed").expect("the tool finished");

    assert!(
        checkpointed < completed,
        "a crash mid-write must still leave a usable checkpoint behind"
    );
}

#[tokio::test]
async fn undo_works_from_a_later_process_via_the_session_log() {
    let dir = workspace();
    let config = config(5.0);

    // Session one: make a change, then drop the agent entirely.
    {
        let mut agent = writing_agent(&dir, vec![patch_to("t1", "43"), answer("Done.")], &config);
        drive(&mut agent, "change the answer").await;
    }
    assert!(lib_contents(&dir).contains("43"));

    // A separate "process": nothing is carried over but the files on disk.
    let session_dir = newest_session(dir.path()).expect("a session was recorded");
    let events = session_dir.join("events.jsonl");
    let entry = last_undoable(&events).expect("the log is readable").expect("an undoable change");

    tc_agent::checkpoint::restore(&session_dir, dir.path(), &entry).expect("the restore succeeds");

    let mut log = SessionLog::reopen(&session_dir);
    log.append(tc_core::EventKind::Reverted {
        path: entry.path.clone(),
        checkpoint_seq: entry.seq,
    });

    assert_eq!(lib_contents(&dir), ORIGINAL, "undo must work after the session ended");
    assert!(
        last_undoable(&events).expect("the log is readable").is_none(),
        "the revert must be recorded, or undoing again would repeat itself"
    );
}

#[tokio::test]
async fn reopening_a_log_continues_the_sequence_numbers() {
    let dir = workspace();
    let config = config(5.0);
    let mut agent = writing_agent(&dir, vec![patch_to("t1", "43"), answer("Done.")], &config);
    let session_dir = agent.log().directory().expect("the log is enabled");

    drive(&mut agent, "change the answer").await;
    let before = std::fs::read_to_string(session_dir.join("events.jsonl")).unwrap();
    let highest: u64 = before
        .lines()
        .filter_map(|line| tc_core::Event::from_jsonl(line).ok())
        .map(|event| event.seq)
        .max()
        .expect("events were written");

    let mut reopened = SessionLog::reopen(&session_dir);
    reopened
        .append(tc_core::EventKind::Reverted { path: "src/lib.rs".to_owned(), checkpoint_seq: 0 });

    let after = std::fs::read_to_string(session_dir.join("events.jsonl")).unwrap();
    let sequences: Vec<u64> = after
        .lines()
        .filter_map(|line| tc_core::Event::from_jsonl(line).ok())
        .map(|event| event.seq)
        .collect();

    assert_eq!(*sequences.last().expect("an event"), highest + 1, "seq must continue, not restart");

    let mut unique = sequences.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), sequences.len(), "sequence numbers must stay unique");
}
