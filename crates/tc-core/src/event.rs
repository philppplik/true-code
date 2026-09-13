//! The append-only session event log.
//!
//! Everything true-code knows about a session is derived from this log: resume,
//! replay, cost accounting and post-mortem debugging. Getting the shape right
//! *now* is what makes those features nearly free later — retrofitting a log onto
//! a mutable session is the expensive path.
//!
//! On disk this is JSON Lines (`events.jsonl`): one self-contained JSON object per
//! line, appended and never rewritten. That keeps it greppable, tail-able and
//! resilient against a crash mid-write — a truncated last line costs one event,
//! not the session.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cost::{Cost, Usage};
use crate::message::StopReason;

/// Identifier of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Generates a fresh identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// What happened.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    /// A session was opened.
    SessionStarted {
        /// Provider/model in use at start, e.g. `anthropic/claude-sonnet-4-5`.
        model: String,
        /// Version of true-code that wrote this log, for replay compatibility.
        tool_version: String,
    },
    /// The operator sent a prompt.
    UserMessage {
        /// The prompt text.
        content: String,
    },
    /// The model produced an answer.
    AssistantMessage {
        /// The complete assistant text for the turn.
        content: String,
    },
    /// The model asked for a tool to run.
    ///
    /// Recorded *before* execution, so a session that crashes mid-tool still
    /// shows what was attempted — which is usually the interesting part.
    ToolCalled {
        /// Identifier tying this to its [`EventKind::ToolCompleted`].
        call_id: String,
        /// Name of the tool.
        tool: String,
        /// Arguments the model supplied.
        input: serde_json::Value,
    },
    /// A human decided whether a change could go ahead.
    ///
    /// Recorded separately from the tool call so the audit trail answers "who
    /// allowed this?", not only "what ran?".
    ApprovalDecided {
        /// Identifier of the tool call this decides.
        call_id: String,
        /// Name of the tool.
        tool: String,
        /// One-line description of what was proposed.
        summary: String,
        /// Whether it was allowed.
        approved: bool,
    },
    /// A file's contents were saved before a tool changed it.
    ///
    /// Written *before* the change is applied. If the process dies mid-write, the
    /// checkpoint already exists — which is the whole point of recording it first.
    Checkpointed {
        /// Path relative to the workspace root.
        path: String,
        /// File name inside the session's `backups/` directory.
        ///
        /// `None` means the file did not exist, so undoing means deleting it again.
        backup: Option<String>,
        /// Tool that was about to change it.
        tool: String,
    },
    /// A checkpoint was rolled back.
    Reverted {
        /// Path relative to the workspace root.
        path: String,
        /// Sequence number of the [`EventKind::Checkpointed`] this undoes.
        ///
        /// Referencing the event rather than the path is what makes "undo again"
        /// walk backwards correctly through repeated edits to the same file.
        checkpoint_seq: u64,
    },
    /// A tool finished.
    ToolCompleted {
        /// Identifier of the matching [`EventKind::ToolCalled`].
        call_id: String,
        /// Whether the tool failed.
        is_error: bool,
        /// How long it ran, in milliseconds.
        duration_ms: u64,
        /// Size of the output in bytes, before any truncation for the model.
        output_bytes: usize,
    },
    /// A turn finished, with its accounting.
    TurnCompleted {
        /// Why generation stopped.
        stop_reason: StopReason,
        /// Tokens consumed by the turn.
        usage: Usage,
        /// Money spent on the turn.
        cost: Cost,
    },
    /// Something went wrong. Errors are events, not silent exits.
    Error {
        /// Human-readable description.
        message: String,
    },
}

/// A single, immutable entry in the session log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Session this event belongs to.
    pub session_id: SessionId,
    /// Monotonic sequence number within the session, starting at 0.
    ///
    /// Makes `replay --step N` addressable without counting lines.
    pub seq: u64,
    /// Wall-clock time in UTC, RFC 3339.
    pub at: DateTime<Utc>,
    /// What happened.
    #[serde(flatten)]
    pub kind: EventKind,
}

impl Event {
    /// Creates an event stamped with the current time.
    #[must_use]
    pub fn now(session_id: SessionId, seq: u64, kind: EventKind) -> Self {
        Self { session_id, seq, at: Utc::now(), kind }
    }

    /// Encodes the event as a single JSON Lines record, newline included.
    pub fn to_jsonl(&self) -> crate::Result<String> {
        let mut line = serde_json::to_string(self)?;
        line.push('\n');
        Ok(line)
    }

    /// Parses a single JSON Lines record.
    pub fn from_jsonl(line: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(line.trim_end())?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> Event {
        Event::now(
            SessionId::new(),
            0,
            EventKind::UserMessage { content: "explain this diff".to_owned() },
        )
    }

    #[test]
    fn jsonl_round_trips_without_loss() {
        let event = sample_event();
        let line = event.to_jsonl().expect("event is serialisable");
        let parsed = Event::from_jsonl(&line).expect("event is parsable");
        assert_eq!(event, parsed);
    }

    #[test]
    fn jsonl_is_exactly_one_line() {
        let line = sample_event().to_jsonl().expect("event is serialisable");
        assert_eq!(line.matches('\n').count(), 1);
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn kind_is_flattened_so_logs_stay_greppable() {
        let line = sample_event().to_jsonl().expect("event is serialisable");
        assert!(line.contains("\"kind\":\"user_message\""), "unexpected encoding: {line}");
    }

    #[test]
    fn session_ids_are_unique() {
        assert_ne!(SessionId::new(), SessionId::new());
    }
}
