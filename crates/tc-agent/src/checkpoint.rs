//! Checkpoints and undo.
//!
//! Confirmation (ADR 0006) lets you see a change before it lands. This is the
//! other half: taking it back afterwards. Both are needed — people approve
//! changes they then regret, and "I could see it coming" is no comfort once it
//! has landed.
//!
//! # How it works
//!
//! Before a tool modifies a file, its current bytes are copied into the session's
//! `backups/` directory and a `Checkpointed` event is appended to the log — in
//! that order, so a crash between the two loses nothing that matters.
//!
//! Undo is then derived entirely from the event log, exactly as ADR 0004 promised:
//! find the newest `Checkpointed` that no `Reverted` refers to, restore it, append
//! a `Reverted`. Nothing else has to be kept in sync, and undo works from a *later
//! process* — `true-code undo` tomorrow reverts what the agent wrote today.
//!
//! # What this is not
//!
//! It is not version control. It undoes changes true-code made, one at a time,
//! newest first. It knows nothing about edits you made yourself in between, and
//! restoring a checkpoint will overwrite them. Commit your work.

use std::path::{Path, PathBuf};

use tc_core::{Event, EventKind};

/// Directory, relative to the session directory, holding saved file contents.
const BACKUPS_DIR: &str = "backups";

/// Errors raised while checkpointing or undoing.
#[derive(Debug, thiserror::Error)]
pub enum UndoError {
    /// There is nothing left to undo.
    #[error("nothing to undo — true-code has not changed any files in this session")]
    NothingToUndo,

    /// No session has been recorded in this project.
    #[error("no session log found in {0} — nothing to undo")]
    NoSession(String),

    /// The saved copy is gone.
    #[error("the saved copy of `{path}` is missing — it cannot be restored")]
    BackupMissing {
        /// Which file could not be restored.
        path: String,
    },

    /// A filesystem operation failed.
    #[error("{operation} failed for `{path}`: {source}")]
    Io {
        /// What was being attempted.
        operation: &'static str,
        /// The path involved.
        path: String,
        /// The underlying error.
        source: std::io::Error,
    },
}

/// Saves file contents before they are overwritten.
#[derive(Debug)]
pub struct Checkpoints {
    dir: PathBuf,
    next: u32,
}

impl Checkpoints {
    /// Creates a store inside the given session directory.
    #[must_use]
    pub fn new(session_dir: impl Into<PathBuf>) -> Self {
        Self { dir: session_dir.into().join(BACKUPS_DIR), next: 0 }
    }

    /// Copies a file's current contents aside.
    ///
    /// Returns the backup's file name, or `None` when the file does not exist yet
    /// — that is not a failure, it is what "undo means delete it again" looks like.
    pub fn capture(&mut self, absolute: &Path, display: &str) -> Result<Option<String>, UndoError> {
        let io_error = |operation: &'static str| {
            move |source: std::io::Error| UndoError::Io {
                operation,
                path: display.to_owned(),
                source,
            }
        };

        let contents = match std::fs::read(absolute) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(io_error("read")(source)),
            Ok(contents) => contents,
        };

        std::fs::create_dir_all(&self.dir).map_err(io_error("create directory"))?;

        // Numbered rather than named after the file: two edits to the same path
        // must produce two distinct backups, or undoing twice restores the wrong
        // state. The extension keeps the directory obvious to a human.
        let name = format!("{:04}.bak", self.next);
        self.next += 1;

        std::fs::write(self.dir.join(&name), contents).map_err(io_error("write backup"))?;
        Ok(Some(name))
    }
}

/// A change that can still be undone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Undoable {
    /// Sequence number of the `Checkpointed` event.
    pub seq: u64,
    /// Path relative to the workspace root.
    pub path: String,
    /// Backup file name, or `None` when the file was created by the change.
    pub backup: Option<String>,
}

/// Finds the newest change that has not been undone yet.
///
/// Derived from the log rather than from separate bookkeeping, so it is correct
/// across process boundaries and cannot drift out of sync.
pub fn last_undoable(events_path: &Path) -> Result<Option<Undoable>, UndoError> {
    let raw = std::fs::read_to_string(events_path).map_err(|source| UndoError::Io {
        operation: "read session log",
        path: events_path.display().to_string(),
        source,
    })?;

    let mut checkpoints: Vec<Undoable> = Vec::new();
    let mut reverted: std::collections::HashSet<u64> = std::collections::HashSet::new();

    for line in raw.lines() {
        // A truncated final line is expected after a crash and must not make the
        // whole log unreadable — that is why the format is one event per line.
        let Ok(event) = Event::from_jsonl(line) else {
            continue;
        };
        match event.kind {
            EventKind::Checkpointed { path, backup, .. } => {
                checkpoints.push(Undoable { seq: event.seq, path, backup });
            }
            EventKind::Reverted { checkpoint_seq, .. } => {
                reverted.insert(checkpoint_seq);
            }
            _ => {}
        }
    }

    Ok(checkpoints.into_iter().rev().find(|entry| !reverted.contains(&entry.seq)))
}

/// Puts a checkpointed file back the way it was.
pub fn restore(session_dir: &Path, root: &Path, entry: &Undoable) -> Result<(), UndoError> {
    let absolute = root.join(&entry.path);
    let io_error = |operation: &'static str| {
        move |source: std::io::Error| UndoError::Io { operation, path: entry.path.clone(), source }
    };

    let Some(backup) = &entry.backup else {
        // The file did not exist before, so undoing it means removing it. An
        // already-absent file is the desired end state, not an error.
        return match std::fs::remove_file(&absolute) {
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(io_error("delete")(source)),
            Ok(()) => Ok(()),
        };
    };

    let saved = session_dir.join(BACKUPS_DIR).join(backup);
    if !saved.exists() {
        return Err(UndoError::BackupMissing { path: entry.path.clone() });
    }

    if let Some(parent) = absolute.parent() {
        std::fs::create_dir_all(parent).map_err(io_error("create directory"))?;
    }
    std::fs::copy(&saved, &absolute).map_err(io_error("restore"))?;
    Ok(())
}

/// Locates the most recently written session directory under a project root.
pub fn newest_session(root: &Path) -> Result<PathBuf, UndoError> {
    let sessions = root.join(".truecode").join("sessions");

    let newest = std::fs::read_dir(&sessions)
        .map_err(|_| UndoError::NoSession(sessions.display().to_string()))?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().join("events.jsonl").exists())
        .max_by_key(|entry| {
            // Ordered by the log's modification time rather than the directory
            // name: session ids are random, so the name says nothing about age.
            entry
                .path()
                .join("events.jsonl")
                .metadata()
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
        })
        .map(|entry| entry.path());

    newest.ok_or_else(|| UndoError::NoSession(sessions.display().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_core::SessionId;

    /// Writes a session log containing the given events.
    fn log_with(dir: &Path, kinds: Vec<EventKind>) -> PathBuf {
        let path = dir.join("events.jsonl");
        let session = SessionId::new();
        let mut out = String::new();
        for (seq, kind) in kinds.into_iter().enumerate() {
            let event = Event::now(session, seq as u64, kind);
            out.push_str(&event.to_jsonl().expect("the event is serialisable"));
        }
        std::fs::write(&path, out).expect("the log is writable");
        path
    }

    fn checkpointed(path: &str, backup: Option<&str>) -> EventKind {
        EventKind::Checkpointed {
            path: path.to_owned(),
            backup: backup.map(str::to_owned),
            tool: "patch".to_owned(),
        }
    }

    #[test]
    fn capturing_an_existing_file_saves_its_contents() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let target = dir.path().join("a.rs");
        std::fs::write(&target, "before\n").expect("file is writable");

        let mut store = Checkpoints::new(dir.path());
        let name = store.capture(&target, "a.rs").expect("capture succeeds").expect("file exists");

        let saved = dir.path().join(BACKUPS_DIR).join(name);
        assert_eq!(std::fs::read_to_string(saved).unwrap(), "before\n");
    }

    #[test]
    fn capturing_a_missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let mut store = Checkpoints::new(dir.path());

        let captured =
            store.capture(&dir.path().join("absent.rs"), "absent.rs").expect("capture succeeds");

        assert_eq!(captured, None, "a file that will be created has nothing to save");
    }

    #[test]
    fn two_captures_of_one_file_produce_two_backups() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let target = dir.path().join("a.rs");
        let mut store = Checkpoints::new(dir.path());

        std::fs::write(&target, "one\n").expect("file is writable");
        let first = store.capture(&target, "a.rs").unwrap().unwrap();
        std::fs::write(&target, "two\n").expect("file is writable");
        let second = store.capture(&target, "a.rs").unwrap().unwrap();

        assert_ne!(first, second, "undoing twice must reach two different states");
        let backups = dir.path().join(BACKUPS_DIR);
        assert_eq!(std::fs::read_to_string(backups.join(first)).unwrap(), "one\n");
        assert_eq!(std::fs::read_to_string(backups.join(second)).unwrap(), "two\n");
    }

    #[test]
    fn the_newest_unreverted_checkpoint_is_the_one_to_undo() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let path = log_with(
            dir.path(),
            vec![checkpointed("a.rs", Some("0000.bak")), checkpointed("b.rs", Some("0001.bak"))],
        );

        let entry = last_undoable(&path).expect("the log is readable").expect("something to undo");

        assert_eq!(entry.path, "b.rs");
        assert_eq!(entry.seq, 1);
    }

    #[test]
    fn an_already_reverted_checkpoint_is_skipped() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let path = log_with(
            dir.path(),
            vec![
                checkpointed("a.rs", Some("0000.bak")),
                checkpointed("b.rs", Some("0001.bak")),
                EventKind::Reverted { path: "b.rs".to_owned(), checkpoint_seq: 1 },
            ],
        );

        let entry = last_undoable(&path).expect("the log is readable").expect("something to undo");

        assert_eq!(entry.path, "a.rs", "undo must walk backwards, not repeat itself");
    }

    #[test]
    fn repeated_edits_to_one_file_undo_one_step_at_a_time() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let path = log_with(
            dir.path(),
            vec![
                checkpointed("a.rs", Some("0000.bak")),
                checkpointed("a.rs", Some("0001.bak")),
                EventKind::Reverted { path: "a.rs".to_owned(), checkpoint_seq: 1 },
            ],
        );

        let entry = last_undoable(&path).expect("the log is readable").expect("something to undo");

        assert_eq!(entry.backup.as_deref(), Some("0000.bak"), "the older state comes next");
    }

    #[test]
    fn a_log_with_no_changes_has_nothing_to_undo() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let path = log_with(dir.path(), vec![EventKind::UserMessage { content: "hi".to_owned() }]);

        assert_eq!(last_undoable(&path).expect("the log is readable"), None);
    }

    #[test]
    fn a_truncated_final_line_does_not_make_the_log_unreadable() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let path = log_with(dir.path(), vec![checkpointed("a.rs", Some("0000.bak"))]);

        let mut raw = std::fs::read_to_string(&path).unwrap();
        raw.push_str("{\"session_id\":\"trunc");
        std::fs::write(&path, raw).expect("the log is writable");

        let entry = last_undoable(&path).expect("the log is still readable").expect("an entry");
        assert_eq!(entry.path, "a.rs");
    }

    #[test]
    fn restoring_puts_the_old_contents_back() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).expect("dir is creatable");
        std::fs::write(root.join("a.rs"), "changed\n").expect("file is writable");

        let session = dir.path().join("session");
        std::fs::create_dir_all(session.join(BACKUPS_DIR)).expect("dir is creatable");
        std::fs::write(session.join(BACKUPS_DIR).join("0000.bak"), "original\n")
            .expect("file is writable");

        let entry =
            Undoable { seq: 0, path: "a.rs".to_owned(), backup: Some("0000.bak".to_owned()) };
        restore(&session, &root, &entry).expect("the restore succeeds");

        assert_eq!(std::fs::read_to_string(root.join("a.rs")).unwrap(), "original\n");
    }

    #[test]
    fn undoing_a_created_file_deletes_it() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let root = dir.path().join("project");
        std::fs::create_dir_all(&root).expect("dir is creatable");
        std::fs::write(root.join("new.rs"), "created\n").expect("file is writable");

        let entry = Undoable { seq: 0, path: "new.rs".to_owned(), backup: None };
        restore(dir.path(), &root, &entry).expect("the restore succeeds");

        assert!(!root.join("new.rs").exists(), "a created file must be removed again");
    }

    #[test]
    fn undoing_an_already_deleted_file_is_not_an_error() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let entry = Undoable { seq: 0, path: "gone.rs".to_owned(), backup: None };

        assert!(restore(dir.path(), dir.path(), &entry).is_ok(), "the end state is what matters");
    }

    #[test]
    fn a_missing_backup_is_reported_rather_than_silently_skipped() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let entry =
            Undoable { seq: 0, path: "a.rs".to_owned(), backup: Some("nope.bak".to_owned()) };

        let error = restore(dir.path(), dir.path(), &entry).expect_err("the backup is gone");

        assert!(matches!(error, UndoError::BackupMissing { .. }));
    }

    #[test]
    fn a_project_with_no_sessions_reports_nothing_to_undo() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");

        let error = newest_session(dir.path()).expect_err("there are no sessions");

        assert!(matches!(error, UndoError::NoSession(_)));
    }

    #[test]
    fn the_most_recently_written_session_is_chosen() {
        let dir = tempfile::tempdir().expect("temp dir is creatable");
        let sessions = dir.path().join(".truecode").join("sessions");

        for name in ["aaa-old", "zzz-new"] {
            std::fs::create_dir_all(sessions.join(name)).expect("dir is creatable");
            std::fs::write(sessions.join(name).join("events.jsonl"), "").expect("file is writable");
            // Ensure the modification times differ on coarse-grained clocks.
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        let newest = newest_session(dir.path()).expect("a session exists");
        assert!(newest.ends_with("zzz-new"), "picked {}", newest.display());
    }
}
