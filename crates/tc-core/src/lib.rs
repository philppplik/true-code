//! Core domain model of **true-code**.
//!
//! This crate is deliberately free of I/O, UI and provider specifics. It defines
//! the vocabulary that every other crate speaks:
//!
//! * [`message`] — conversation messages and streaming deltas
//! * [`cost`] — token usage and money, tracked from the first turn on
//! * [`event`] — the append-only event log that makes sessions resumable and replayable
//!
//! # Design rules
//!
//! 1. **No UI dependency.** The agent engine must run headless (CI, `-p` mode, ACP).
//! 2. **Append-only.** Events are never mutated, only appended — that is what buys us
//!    resume, replay, cost accounting and post-mortem debugging for free.
//! 3. **Cost is a first-class citizen**, not a reporting afterthought.

#![doc(html_root_url = "https://docs.rs/tc-core/0.1.0")]

pub mod cost;
pub mod event;
pub mod message;

pub use cost::{Cost, Price, Usage};
pub use event::{Event, EventKind, SessionId};
pub use message::{Delta, Message, Role, StopReason};

/// Errors raised by the core domain model.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A session log line could not be encoded or decoded.
    #[error("event log is corrupt: {0}")]
    CorruptEventLog(#[from] serde_json::Error),

    /// A price string could not be parsed into a [`Price`].
    #[error("invalid price specification: {0}")]
    InvalidPrice(String),
}

/// Convenience result alias for this crate.
pub type Result<T> = std::result::Result<T, CoreError>;
