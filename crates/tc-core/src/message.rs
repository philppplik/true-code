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
    /// Input from the human operator, and tool results fed back to the model.
    User,
    /// Output from the model.
    Assistant,
}

/// A request from the model to run a tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Provider-assigned identifier. The matching [`ToolResult`] must echo it.
    ///
    /// Getting this wrong is a silent failure: the model receives a result it
    /// cannot attribute and usually retries the same call forever.
    pub id: String,
    /// Name of the tool to run.
    pub name: String,
    /// Arguments, shaped by the tool's input schema.
    pub input: serde_json::Value,
}

/// The outcome of running a tool, fed back to the model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Identifier of the [`ToolCall`] this answers.
    pub call_id: String,
    /// What the tool produced, or the error text if it failed.
    pub output: String,
    /// Whether the tool failed.
    ///
    /// A failed tool is *context*, not a crash: the error text goes back to the
    /// model, which is the agent's main channel for learning what actually works.
    pub is_error: bool,
}

/// One piece of a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Content {
    /// Plain text.
    Text {
        /// The text.
        text: String,
    },
    /// The model asking for a tool to be run.
    ToolCall(ToolCall),
    /// The harness answering a tool call.
    ToolResult(ToolResult),
}

/// A single message in the conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    /// Author of the message.
    pub role: Role,
    /// One or more content blocks. A single turn can mix text and tool calls.
    pub content: Vec<Content>,
}

impl Message {
    /// Creates a message with a single text block.
    #[must_use]
    pub fn text(role: Role, content: impl Into<String>) -> Self {
        Self { role, content: vec![Content::Text { text: content.into() }] }
    }

    /// Creates a [`Role::System`] message.
    #[must_use]
    pub fn system(content: impl Into<String>) -> Self {
        Self::text(Role::System, content)
    }

    /// Creates a [`Role::User`] message.
    #[must_use]
    pub fn user(content: impl Into<String>) -> Self {
        Self::text(Role::User, content)
    }

    /// Creates a [`Role::Assistant`] message.
    #[must_use]
    pub fn assistant(content: impl Into<String>) -> Self {
        Self::text(Role::Assistant, content)
    }

    /// Creates the user-side message carrying tool results back to the model.
    ///
    /// All results for one assistant turn belong in a single message: providers
    /// reject a conversation where a tool call is not answered in the very next
    /// message.
    #[must_use]
    pub fn tool_results(results: Vec<ToolResult>) -> Self {
        Self { role: Role::User, content: results.into_iter().map(Content::ToolResult).collect() }
    }

    /// Concatenates every text block, ignoring tool calls and results.
    #[must_use]
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                Content::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    /// Returns every tool call in this message.
    #[must_use]
    pub fn tool_calls(&self) -> Vec<&ToolCall> {
        self.content
            .iter()
            .filter_map(|block| match block {
                Content::ToolCall(call) => Some(call),
                _ => None,
            })
            .collect()
    }
}

/// Why the model stopped generating.
///
/// Providers report this very differently (`end_turn`, `stop`, `length`, …);
/// normalising it here keeps the agent loop provider-agnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The model finished its turn on its own.
    ///
    /// The default: a turn under construction has nothing unusual to report yet.
    #[default]
    EndTurn,
    /// The model is waiting for one or more tools to run.
    ToolUse,
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
    /// A complete tool call.
    ///
    /// Providers stream tool arguments as partial JSON. The provider layer
    /// buffers those fragments and emits this only once the arguments parse, so
    /// no consumer ever has to handle half a JSON object.
    ToolCall(ToolCall),
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
    fn text_content_joins_every_text_block() {
        let message = Message {
            role: Role::Assistant,
            content: vec![
                Content::Text { text: "Hal".to_owned() },
                Content::Text { text: "lo".to_owned() },
            ],
        };
        assert_eq!(message.text_content(), "Hallo");
    }

    #[test]
    fn text_content_ignores_tool_blocks() {
        let message = Message {
            role: Role::Assistant,
            content: vec![
                Content::Text { text: "reading".to_owned() },
                Content::ToolCall(ToolCall {
                    id: "call_1".to_owned(),
                    name: "read_file".to_owned(),
                    input: serde_json::json!({ "path": "src/lib.rs" }),
                }),
            ],
        };
        assert_eq!(message.text_content(), "reading");
        assert_eq!(message.tool_calls().len(), 1);
        assert_eq!(message.tool_calls()[0].name, "read_file");
    }

    #[test]
    fn tool_results_are_grouped_into_one_user_message() {
        let message = Message::tool_results(vec![
            ToolResult { call_id: "a".to_owned(), output: "1".to_owned(), is_error: false },
            ToolResult { call_id: "b".to_owned(), output: "2".to_owned(), is_error: true },
        ]);
        assert_eq!(message.role, Role::User);
        assert_eq!(message.content.len(), 2);
    }

    #[test]
    fn text_delta_round_trips_through_json() {
        let delta = Delta::Text { text: "hello".to_owned() };
        let encoded = serde_json::to_string(&delta).expect("delta is serialisable");
        let decoded: Delta = serde_json::from_str(&encoded).expect("delta is deserialisable");
        assert_eq!(delta, decoded);
    }

    #[test]
    fn tool_call_delta_round_trips_through_json() {
        let delta = Delta::ToolCall(ToolCall {
            id: "call_1".to_owned(),
            name: "grep".to_owned(),
            input: serde_json::json!({ "pattern": "fn main" }),
        });
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
