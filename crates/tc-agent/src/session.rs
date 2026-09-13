//! The session log writer.
//!
//! Implements the format specified in ADR 0004: append-only JSON Lines, one
//! self-contained object per line, under `.truecode/sessions/<id>/events.jsonl`.
//!
//! # Failure policy
//!
//! Logging must never take down a session. If the log cannot be written — a
//! read-only checkout, a full disk — the agent reports it once and keeps working.
//! Losing the audit trail is bad; losing the user's in-progress work because the
//! audit trail failed is worse.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use tc_core::{Event, EventKind, SessionId};

/// Directory, relative to the project root, holding session logs.
const SESSIONS_DIR: &str = ".truecode/sessions";

/// File name of the event log inside a session directory.
const EVENTS_FILE: &str = "events.jsonl";

/// Appends events for one session.
#[derive(Debug)]
pub struct SessionLog {
    id: SessionId,
    /// `None` once writing has failed; the session continues without a log.
    file: Option<File>,
    path: PathBuf,
    seq: u64,
}

impl SessionLog {
    /// Opens a log for a new session under `root`.
    ///
    /// Never fails: a log that cannot be opened degrades to a no-op writer.
    #[must_use]
    pub fn create(root: &Path, id: SessionId) -> Self {
        let dir = root.join(SESSIONS_DIR).join(id.to_string());
        let path = dir.join(EVENTS_FILE);

        let file = std::fs::create_dir_all(&dir)
            .and_then(|()| OpenOptions::new().create(true).append(true).open(&path))
            .map_err(|err| {
                tracing::warn!(path = %path.display(), error = %err, "session log disabled");
            })
            .ok();

        Self { id, file, path, seq: 0 }
    }

    /// Creates a log that writes nowhere, for tests and ephemeral runs.
    #[must_use]
    pub fn disabled(id: SessionId) -> Self {
        Self { id, file: None, path: PathBuf::new(), seq: 0 }
    }

    /// The session identifier.
    #[must_use]
    pub const fn id(&self) -> SessionId {
        self.id
    }

    /// Where the log is being written, if it is.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.file.as_ref().map(|_| self.path.as_path())
    }

    /// Appends one event, stamping it with the next sequence number.
    ///
    /// The sequence number advances even when writing is disabled, so a replay of
    /// a partial log still has gap-free numbering up to the point it stopped.
    pub fn append(&mut self, kind: EventKind) {
        let event = Event::now(self.id, self.seq, kind);
        self.seq += 1;

        let Some(file) = self.file.as_mut() else {
            return;
        };

        match event.to_jsonl().map(|line| file.write_all(line.as_bytes())) {
            Ok(Ok(())) => {}
            Ok(Err(err)) => {
                tracing::warn!(error = %err, "session log write failed; disabling it");
                self.file = None;
            }
            Err(err) => {
                tracing::warn!(error = %err, "event could not be encoded; skipping it");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read_lines(path: &Path) -> Vec<Event> {
        std::fs::read_to_string(path)
            .expect("the log is readable")
            .lines()
            .map(|line| Event::from_jsonl(line).expect("each line is a valid event"))
            .collect()
    }

    #[test]
    fn events_are_appended_in_order_with_gap_free_sequence_numbers() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let mut log = SessionLog::create(dir.path(), SessionId::new());

        log.append(EventKind::UserMessage { content: "first".to_owned() });
        log.append(EventKind::UserMessage { content: "second".to_owned() });

        let events = read_lines(log.path().expect("the log is enabled"));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].seq, 0);
        assert_eq!(events[1].seq, 1);
    }

    #[test]
    fn every_event_carries_the_session_id() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let id = SessionId::new();
        let mut log = SessionLog::create(dir.path(), id);

        log.append(EventKind::UserMessage { content: "hi".to_owned() });

        let events = read_lines(log.path().expect("the log is enabled"));
        assert_eq!(events[0].session_id, id);
    }

    #[test]
    fn reopening_a_session_appends_rather_than_truncating() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let id = SessionId::new();

        let mut first = SessionLog::create(dir.path(), id);
        first.append(EventKind::UserMessage { content: "one".to_owned() });
        let path = first.path().expect("the log is enabled").to_path_buf();
        drop(first);

        let mut second = SessionLog::create(dir.path(), id);
        second.append(EventKind::UserMessage { content: "two".to_owned() });

        assert_eq!(read_lines(&path).len(), 2, "the first event must survive");
    }

    #[test]
    fn tool_events_round_trip_through_the_log() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let mut log = SessionLog::create(dir.path(), SessionId::new());

        log.append(EventKind::ToolCalled {
            call_id: "toolu_1".to_owned(),
            tool: "read_file".to_owned(),
            input: serde_json::json!({ "path": "src/lib.rs" }),
        });

        let events = read_lines(log.path().expect("the log is enabled"));
        match &events[0].kind {
            EventKind::ToolCalled { tool, input, .. } => {
                assert_eq!(tool, "read_file");
                assert_eq!(input["path"], "src/lib.rs");
            }
            other => panic!("expected a tool call event, got {other:?}"),
        }
    }

    #[test]
    fn a_disabled_log_still_counts_events_and_never_panics() {
        let mut log = SessionLog::disabled(SessionId::new());
        log.append(EventKind::UserMessage { content: "hi".to_owned() });
        assert!(log.path().is_none());
        assert_eq!(log.seq, 1);
    }

    #[test]
    fn an_unwritable_root_degrades_instead_of_failing() {
        // A path whose parent is a *file* cannot become a directory.
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let blocker = dir.path().join("blocked");
        std::fs::write(&blocker, "not a directory").expect("file is writable");

        let mut log = SessionLog::create(&blocker, SessionId::new());
        log.append(EventKind::UserMessage { content: "hi".to_owned() });

        assert!(log.path().is_none(), "the session continues without a log");
    }
}
