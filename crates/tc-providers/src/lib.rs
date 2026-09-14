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

impl ProviderError {
    /// What the user should do about it, when we can say something useful.
    ///
    /// The raw message is what the vendor said; this is what it means for the
    /// person reading it. An HTTP status a user has to search for is a status we
    /// failed to explain.
    #[must_use]
    pub fn advice(&self) -> Option<String> {
        match self {
            Self::Network { .. } => Some(
                "Could not reach the provider. Check your connection, then try again — \
                 nothing was sent, so nothing was charged."
                    .to_owned(),
            ),

            Self::Api { provider, status, body } => match status {
                401 | 403 => Some(format!(
                    "Your {provider} key was rejected. Check it with `truecode auth status`, \
                     or replace it with `truecode auth login {provider}`."
                )),
                404 => Some(format!(
                    "{provider} has no such model. Find a real id with `truecode models <search>`, \
                     then use `truecode --model <id>`."
                )),
                // 402 is how a gateway says "out of credit"; a generic HTTP
                // message here sends people hunting through their own code.
                402 => Some(format!("Your {provider} account is out of credit.")),
                429 => Some(
                    "Rate limited. Wait a moment, or switch to a smaller model with \
                     `truecode --model <id>`."
                        .to_owned(),
                ),
                500..=599 => Some(format!(
                    "{provider} is having trouble on their end. This is not your setup."
                )),
                400 if body.contains("context") || body.contains("token") => Some(
                    "The request was too large for this model's context window. Start a fresh \
                     session, or pick a model with a bigger window."
                        .to_owned(),
                ),
                _ => None,
            },

            Self::Decode { .. } => Some(
                "The provider sent something true-code could not read. Re-run with --verbose \
                 to see the raw stream, and please report it."
                    .to_owned(),
            ),
        }
    }

    /// The message plus the advice, ready to show.
    #[must_use]
    pub fn explain(&self) -> String {
        match self.advice() {
            Some(advice) => format!("{self}\n\n{advice}"),
            None => self.to_string(),
        }
    }
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

/// Builds the provider for a resolved model.
///
/// Dispatch is on the wire protocol, not on the vendor: OpenRouter and any other
/// OpenAI-compatible gateway reuse the OpenAI client with a different base URL,
/// so supporting one more of them is a table entry rather than a new module.
#[must_use]
pub fn provider_for(info: ModelInfo, api_key: String) -> Box<dyn Provider> {
    match info.vendor.kind {
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

    fn api_error(status: u16, body: &str) -> ProviderError {
        ProviderError::Api { provider: "openrouter", status, body: body.to_owned() }
    }

    #[test]
    fn a_rejected_key_says_how_to_replace_it() {
        let advice = api_error(401, "invalid key").advice().expect("401 is explainable");

        assert!(advice.contains("truecode auth login openrouter"), "unexpected: {advice}");
    }

    #[test]
    fn a_missing_model_points_at_the_search_command() {
        // This is the one people hit by typing a model id that looks plausible.
        let advice = api_error(404, "no such model").advice().expect("404 is explainable");

        assert!(advice.contains("truecode models"), "unexpected: {advice}");
    }

    #[test]
    fn running_out_of_credit_is_named_as_such() {
        // 402 sends people hunting through their own code otherwise.
        let advice = api_error(402, "insufficient credits").advice().expect("402 is explainable");

        assert!(advice.contains("out of credit"), "unexpected: {advice}");
    }

    #[test]
    fn a_provider_outage_is_not_blamed_on_the_user() {
        let advice = api_error(503, "upstream").advice().expect("5xx is explainable");

        assert!(advice.contains("not your setup"), "unexpected: {advice}");
    }

    #[test]
    fn a_context_overflow_is_distinguished_from_other_bad_requests() {
        let overflow = api_error(400, "maximum context length exceeded");
        let other = api_error(400, "unsupported parameter foo");

        assert!(overflow.advice().expect("explainable").contains("context window"));
        assert!(other.advice().is_none(), "guessing at an unknown 400 would mislead");
    }

    #[test]
    fn explain_keeps_the_providers_own_words_and_adds_ours() {
        // The vendor message is often the fastest route to the real cause, so it
        // is never replaced — only accompanied.
        let explained = api_error(401, "invalid api key").explain();

        assert!(explained.contains("invalid api key"));
        assert!(explained.contains("truecode auth"));
    }

    #[test]
    fn an_unexplainable_error_is_shown_as_it_came() {
        let bare = api_error(418, "teapot");
        assert_eq!(bare.explain(), bare.to_string());
    }

    #[test]
    fn requests_default_to_a_bounded_output_length() {
        let request = Request::new(None, vec![Message::user("hi")]);
        assert_eq!(request.max_tokens, DEFAULT_MAX_TOKENS);
    }
}
