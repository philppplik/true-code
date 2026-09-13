//! Provider abstraction.
//!
//! # Why a hand-rolled trait instead of an LLM framework
//!
//! Vendor SDKs and multi-provider crates churn fast, and their abstractions leak
//! their author's assumptions. A narrow trait plus ~150 lines of `reqwest` per
//! vendor is more code today and far less coupling tomorrow: the *interface* is
//! ours, only the *implementation* is replaceable. See `docs/adr/0003-provider-trait.md`.
//!
//! # Cancellation
//!
//! There is no cancel method. Dropping the returned stream aborts the HTTP request,
//! which is exactly what `Esc` in the TUI does. One mechanism, no state to get wrong.

pub mod anthropic;
pub mod openai;

use std::pin::Pin;

use futures::Stream;
use tc_config::{ModelInfo, catalog::ProviderKind};
use tc_core::{Content, Delta, Message, Price};

/// A stream of normalised response increments.
pub type DeltaStream = Pin<Box<dyn Stream<Item = Result<Delta, ProviderError>> + Send>>;

/// Errors a provider can raise.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    /// The request never reached the provider, or the connection broke.
    #[error("network error talking to {provider}: {source}")]
    Network {
        /// Which provider was being called.
        provider: &'static str,
        /// The underlying transport error.
        source: reqwest::Error,
    },

    /// The provider answered with a non-success status.
    ///
    /// The body is included because vendor error messages are the fastest way to
    /// understand a 400 — swallowing them wastes the user's time.
    #[error("{provider} returned HTTP {status}: {body}")]
    Api {
        /// Which provider answered.
        provider: &'static str,
        /// HTTP status code.
        status: u16,
        /// Response body, truncated to keep terminal output readable.
        body: String,
    },

    /// The provider sent something we could not interpret.
    #[error("cannot decode {provider} stream: {detail}")]
    Decode {
        /// Which provider sent the payload.
        provider: &'static str,
        /// What exactly failed.
        detail: String,
    },
}

/// Maximum number of body bytes kept in an [`ProviderError::Api`].
const MAX_ERROR_BODY: usize = 2_000;

/// Truncates an error body so a stray HTML page cannot flood the terminal.
fn truncate_body(body: &str) -> String {
    if body.len() <= MAX_ERROR_BODY {
        return body.to_owned();
    }
    let mut end = MAX_ERROR_BODY;
    while end > 0 && !body.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}… ({} bytes total)", &body[..end], body.len())
}

/// A tool offered to the model, in provider-neutral form.
///
/// Deliberately not `tc_tools::Tool`: the provider layer must not depend on the
/// tool layer, or adding a tool would mean touching every vendor client. The
/// agent maps between the two.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolSchema {
    /// Name the model calls the tool by.
    pub name: String,
    /// Description shown to the model.
    pub description: String,
    /// JSON Schema for the arguments.
    pub input_schema: serde_json::Value,
}

/// A request for one model turn.
#[derive(Debug, Clone)]
pub struct Request {
    /// System prompt. Kept prefix-stable so prompt caching can work.
    pub system: Option<String>,
    /// Conversation so far, oldest first.
    pub messages: Vec<Message>,
    /// Tools the model may call this turn.
    pub tools: Vec<ToolSchema>,
    /// Upper bound on generated tokens.
    pub max_tokens: u32,
}

/// Default output cap. Generous enough for an explanation, small enough that a
/// runaway generation cannot silently cost a fortune.
pub const DEFAULT_MAX_TOKENS: u32 = 4_096;

impl Request {
    /// Creates a request with no tools and the default output cap.
    #[must_use]
    pub fn new(system: Option<String>, messages: Vec<Message>) -> Self {
        Self { system, messages, tools: Vec::new(), max_tokens: DEFAULT_MAX_TOKENS }
    }

    /// Offers the given tools to the model.
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolSchema>) -> Self {
        self.tools = tools;
        self
    }
}

/// A model endpoint true-code can talk to.
#[async_trait::async_trait]
pub trait Provider: Send + Sync + std::fmt::Debug {
    /// Qualified model identifier, e.g. `anthropic/claude-sonnet-4-5`.
    fn id(&self) -> &str;

    /// Size of the context window, in tokens.
    fn context_window(&self) -> u32;

    /// Indicative price of this model.
    fn price(&self) -> Price;

    /// Starts a turn and returns the normalised delta stream.
    ///
    /// Drop the stream to abort the turn.
    async fn stream(&self, request: Request) -> Result<DeltaStream, ProviderError>;
}

/// Builds the provider for a catalogue entry.
#[must_use]
pub fn provider_for(info: &'static ModelInfo, api_key: String) -> Box<dyn Provider> {
    match info.provider {
        ProviderKind::Anthropic => Box::new(anthropic::Anthropic::new(info, api_key)),
        ProviderKind::OpenAi => Box::new(openai::OpenAi::new(info, api_key)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_error_bodies_are_passed_through_unchanged() {
        assert_eq!(truncate_body("bad request"), "bad request");
    }

    #[test]
    fn long_error_bodies_are_truncated_with_a_hint() {
        let body = "x".repeat(MAX_ERROR_BODY + 500);
        let truncated = truncate_body(&body);
        assert!(truncated.len() < body.len());
        assert!(truncated.contains("bytes total"));
    }

    #[test]
    fn truncation_never_splits_a_multi_byte_character() {
        let body = "ä".repeat(MAX_ERROR_BODY);
        let truncated = truncate_body(&body);
        assert!(truncated.contains("bytes total"));
    }

    #[test]
    fn requests_default_to_a_bounded_output_length() {
        let request = Request::new(None, vec![Message::user("hi")]);
        assert_eq!(request.max_tokens, DEFAULT_MAX_TOKENS);
    }
}
