//! Conversation messages and the normalised streaming protocol.
//!
//! Every provider (Anthropic, OpenAI, Ollama, …) speaks a slightly different
//! dialect. Providers translate into the types below so that the agent loop and
//! the UI only ever see one shape.

use serde::{Deserialize, Serialize};

use crate::cost::Usage;

/// Who authored a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Instructions that frame the whole session. Kept prefix-stable for prompt caching.
    System,
    /// Input from the human operator.
    User,
    /// Output from the model.
    Assistant,
}

/// A single message in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Author of the message.
    pub role: Role,
    /// Plain-text content. Tool calls get their own variants in a later phase.
    pub content: String,
}

impl Message {
    /// Creates a [`Role::System`] message.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: Role::System, content: content.into() }
    }

    /// Creates a [`Role::User`] message.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into() }
    }

    /// Creates a [`Role::Assistant`] message.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into() }
    }
}

/// Why the model stopped generating.
///
/// Providers report this very differently (`end_turn`, `stop`, `length`, …);
/// normalising it here keeps the agent loop provider-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The model finished its turn on its own.
    EndTurn,
    /// The output token limit was hit — the answer is truncated.
    MaxTokens,
    /// A configured stop sequence matched.
    StopSequence,
    /// The user aborted the turn (`Esc`).
    Aborted,
    /// The provider reported a reason we do not model yet.
    Other(#[serde(default)] OtherReason),
}

/// Free-form stop reason reported by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct OtherReason;

/// One increment of a streaming response.
///
/// The UI consumes these over an `mpsc` channel so that rendering never blocks
/// on the network, and the network never blocks on rendering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Delta {
    /// The provider accepted the request and started generating.
    Started {
        /// Model identifier as reported by the provider, e.g. `claude-sonnet-4-5`.
        model: String,
    },
    /// A chunk of assistant text.
    Text {
        /// The text fragment. May be a partial word — never assume token boundaries.
        text: String,
    },
    /// The turn finished. Carries the final accounting.
    Completed {
        /// Why generation stopped.
        stop_reason: StopReason,
        /// Token counts for this turn.
        usage: Usage,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors_assign_the_expected_role() {
        assert_eq!(Message::system("s").role, Role::System);
        assert_eq!(Message::user("u").role, Role::User);
        assert_eq!(Message::assistant("a").role, Role::Assistant);
    }

    #[test]
    fn text_delta_round_trips_through_json() {
        let delta = Delta::Text { text: "hello".to_owned() };
        let encoded = serde_json::to_string(&delta).expect("delta is serialisable");
        let decoded: Delta = serde_json::from_str(&encoded).expect("delta is deserialisable");
        assert_eq!(delta, decoded);
    }

    #[test]
    fn role_serialises_lowercase_for_provider_payloads() {
        let encoded = serde_json::to_string(&Role::Assistant).expect("role is serialisable");
        assert_eq!(encoded, "\"assistant\"");
    }
}
