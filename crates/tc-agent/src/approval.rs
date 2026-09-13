//! Asking a human before changing anything.
//!
//! Per ADR 0002 the loop does not call the UI; it calls an [`Approver`] the caller
//! supplies. That is what lets the same loop drive a TUI modal, a headless policy
//! and a test, without any of them knowing about the others.
//!
//! # The default is "no"
//!
//! An approver that cannot ask anyone — headless mode, CI — denies. Defaulting to
//! yes would mean a piped prompt could rewrite a repository with nothing between
//! the model and the disk, and the failure would be silent until someone read the
//! diff. Opting in is one flag; opting out of a surprise is not possible.

use tc_tools::Effect;

/// What the human decided about one tool call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    /// Run it, this once.
    Approve,
    /// Run it, and stop asking for this tool for the rest of the session.
    ///
    /// Scoped to a tool rather than to everything, because "yes to all edits" and
    /// "yes to all shell commands" are very different promises.
    ApproveToolForSession,
    /// Do not run it. The model is told, and carries on.
    Deny,
    /// Do not run it, and end the run.
    Abort,
}

/// A pending request for approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRequest {
    /// Name of the tool asking.
    pub tool: String,
    /// Exactly what it would do.
    pub effect: Effect,
}

impl ApprovalRequest {
    /// A one-line description, for a prompt title or a log line.
    #[must_use]
    pub fn summary(&self) -> String {
        self.effect.summary()
    }
}

/// Decides whether a tool call may proceed.
#[async_trait::async_trait]
pub trait Approver: Send + Sync + std::fmt::Debug {
    /// Answers one request.
    async fn approve(&self, request: &ApprovalRequest) -> Decision;
}

/// Refuses everything.
///
/// The correct behaviour wherever there is no human to ask.
#[derive(Debug, Default, Clone, Copy)]
pub struct DenyAll;

#[async_trait::async_trait]
impl Approver for DenyAll {
    async fn approve(&self, _request: &ApprovalRequest) -> Decision {
        Decision::Deny
    }
}

/// Approves everything.
///
/// Only for an explicit, documented opt-in — `--yes` in headless mode — and for
/// tests. Never the default anywhere.
#[derive(Debug, Default, Clone, Copy)]
pub struct ApproveAll;

#[async_trait::async_trait]
impl Approver for ApproveAll {
    async fn approve(&self, _request: &ApprovalRequest) -> Decision {
        Decision::Approve
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_tools::{FileDiff, Risk};

    fn write_request() -> ApprovalRequest {
        ApprovalRequest {
            tool: "write_file".to_owned(),
            effect: Effect::Write(FileDiff::new("src/lib.rs", Some("a\n"), "b\n")),
        }
    }

    #[tokio::test]
    async fn the_default_approver_refuses() {
        assert_eq!(DenyAll.approve(&write_request()).await, Decision::Deny);
    }

    #[tokio::test]
    async fn the_opt_in_approver_accepts() {
        assert_eq!(ApproveAll.approve(&write_request()).await, Decision::Approve);
    }

    #[test]
    fn a_write_request_summarises_the_file_and_the_line_counts() {
        assert_eq!(write_request().summary(), "modify src/lib.rs  +1 −1");
    }

    #[test]
    fn a_command_request_summarises_the_command() {
        let request = ApprovalRequest {
            tool: "shell".to_owned(),
            effect: Effect::Execute { command: "cargo test".to_owned(), risk: Risk::Normal },
        };
        assert_eq!(request.summary(), "run `cargo test`");
    }
}
