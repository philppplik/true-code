//! Anthropic Messages API.
//!
//! Streaming shape (the parts true-code consumes):
//!
//! ```text
//! message_start        → model name, input/cache token counts
//! content_block_delta  → { delta: { type: "text_delta", text: "…" } }
//! message_delta        → { delta: { stop_reason }, usage: { output_tokens } }
//! message_stop         → end of stream
//! ```
//!
//! Everything else (ping, content_block_start/stop, thinking blocks) is ignored
//! rather than treated as an error: providers add event types over time, and an
//! unknown event must not kill a running turn.

use eventsource_stream::Eventsource as _;
use futures::StreamExt;
use serde::Deserialize;
use tc_config::ModelInfo;
use tc_core::{Delta, Price, Role, StopReason, Usage};

use crate::{DeltaStream, Provider, ProviderError, Request, truncate_body};

/// Name used in error messages.
const PROVIDER: &str = "anthropic";

/// API version header value required by the Messages API.
const API_VERSION: &str = "2023-06-01";

/// Client for Anthropic's Messages API.
#[derive(Debug)]
pub struct Anthropic {
    info: &'static ModelInfo,
    api_key: String,
    http: reqwest::Client,
}

impl Anthropic {
    /// Creates a client for the given catalogue entry.
    #[must_use]
    pub fn new(info: &'static ModelInfo, api_key: String) -> Self {
        Self { info, api_key, http: reqwest::Client::new() }
    }
}

#[async_trait::async_trait]
impl Provider for Anthropic {
    fn id(&self) -> &str {
        self.info.id
    }

    fn context_window(&self) -> u32 {
        self.info.context_window
    }

    fn price(&self) -> Price {
        self.info.price
    }

    async fn stream(&self, request: Request) -> Result<DeltaStream, ProviderError> {
        let body = build_body(self.info.api_model, &request);

        let response = self
            .http
            .post(format!("{}/v1/messages", self.info.base_url))
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", API_VERSION)
            .json(&body)
            .send()
            .await
            .map_err(|source| ProviderError::Network { provider: PROVIDER, source })?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(ProviderError::Api {
                provider: PROVIDER,
                status: status.as_u16(),
                body: truncate_body(&body),
            });
        }

        // `usage` arrives in two halves: input counts up front, output counts at the
        // end. We carry the first half along so the final `Completed` delta is whole.
        let mut pending = Usage::default();

        let stream = response
            .bytes_stream()
            .eventsource()
            .filter_map(move |event| {
                let mapped = match event {
                    Err(err) => Some(Err(ProviderError::Decode {
                        provider: PROVIDER,
                        detail: err.to_string(),
                    })),
                    Ok(event) => match parse_event(&event.data, &mut pending) {
                        Ok(delta) => delta.map(Ok),
                        Err(detail) => {
                            Some(Err(ProviderError::Decode { provider: PROVIDER, detail }))
                        }
                    },
                };
                std::future::ready(mapped)
            })
            .boxed();

        Ok(stream)
    }
}

/// Builds the JSON request body.
fn build_body(api_model: &str, request: &Request) -> serde_json::Value {
    let messages: Vec<serde_json::Value> = request
        .messages
        .iter()
        .filter(|message| message.role != Role::System)
        .map(|message| {
            serde_json::json!({
                "role": match message.role {
                    Role::Assistant => "assistant",
                    _ => "user",
                },
                "content": message.content,
            })
        })
        .collect();

    let mut body = serde_json::json!({
        "model": api_model,
        "max_tokens": request.max_tokens,
        "stream": true,
        "messages": messages,
    });

    if let Some(system) = &request.system {
        // `cache_control` on the system block is what makes prompt caching pay off.
        // It only works while the prefix stays byte-identical between turns.
        body["system"] = serde_json::json!([{
            "type": "text",
            "text": system,
            "cache_control": { "type": "ephemeral" },
        }]);
    }
    body
}

/// The subset of the Anthropic stream we act on.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    MessageStart {
        message: MessageStart,
    },
    ContentBlockDelta {
        delta: BlockDelta,
    },
    MessageDelta {
        delta: MessageDeltaBody,
        usage: OutputUsage,
    },
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
struct MessageStart {
    model: String,
    #[serde(default)]
    usage: InputUsage,
}

/// Field names mirror the Anthropic wire format exactly. Renaming them to please
/// a lint would mean adding `#[serde(rename)]` to every field — more code, one more
/// place for the mapping to silently drift from the API.
#[derive(Debug, Default, Deserialize)]
#[allow(clippy::struct_field_names)]
struct InputUsage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BlockDelta {
    TextDelta {
        text: String,
    },
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
struct MessageDeltaBody {
    #[serde(default)]
    stop_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OutputUsage {
    #[serde(default)]
    output_tokens: u32,
}

/// Maps one SSE payload to a [`Delta`], accumulating usage in `pending`.
///
/// Returns `Ok(None)` for events that carry no information for the UI.
fn parse_event(data: &str, pending: &mut Usage) -> Result<Option<Delta>, String> {
    let event: Event = serde_json::from_str(data).map_err(|err| err.to_string())?;

    Ok(match event {
        Event::MessageStart { message } => {
            pending.input_tokens = message.usage.input_tokens;
            pending.cache_read_tokens = message.usage.cache_read_input_tokens;
            pending.cache_write_tokens = message.usage.cache_creation_input_tokens;
            Some(Delta::Started { model: message.model })
        }
        Event::ContentBlockDelta { delta: BlockDelta::TextDelta { text } } => {
            Some(Delta::Text { text })
        }
        Event::MessageDelta { delta, usage } => {
            pending.output_tokens = usage.output_tokens;
            Some(Delta::Completed {
                stop_reason: map_stop_reason(delta.stop_reason.as_deref()),
                usage: *pending,
            })
        }
        Event::ContentBlockDelta { .. } | Event::Ignored => None,
    })
}

/// Normalises Anthropic's stop reasons.
fn map_stop_reason(raw: Option<&str>) -> StopReason {
    match raw {
        Some("end_turn" | "tool_use") => StopReason::EndTurn,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("stop_sequence") => StopReason::StopSequence,
        _ => StopReason::Other(tc_core::message::OtherReason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_core::Message;

    #[test]
    fn message_start_reports_the_model_and_seeds_input_usage() {
        let mut usage = Usage::default();
        let data = r#"{"type":"message_start","message":{"model":"claude-sonnet-4-5",
            "usage":{"input_tokens":120,"cache_read_input_tokens":900}}}"#;

        let delta = parse_event(data, &mut usage).expect("event parses");

        assert_eq!(delta, Some(Delta::Started { model: "claude-sonnet-4-5".to_owned() }));
        assert_eq!(usage.input_tokens, 120);
        assert_eq!(usage.cache_read_tokens, 900);
    }

    #[test]
    fn text_deltas_become_text_increments() {
        let mut usage = Usage::default();
        let data = r#"{"type":"content_block_delta","index":0,
            "delta":{"type":"text_delta","text":"Hallo"}}"#;

        let delta = parse_event(data, &mut usage).expect("event parses");

        assert_eq!(delta, Some(Delta::Text { text: "Hallo".to_owned() }));
    }

    #[test]
    fn message_delta_completes_the_turn_with_the_full_usage() {
        let mut usage = Usage { input_tokens: 120, ..Usage::default() };
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},
            "usage":{"output_tokens":42}}"#;

        let delta = parse_event(data, &mut usage).expect("event parses");

        match delta {
            Some(Delta::Completed { stop_reason, usage }) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 120);
                assert_eq!(usage.output_tokens, 42);
            }
            other => panic!("expected a completed delta, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_types_are_ignored_not_fatal() {
        let mut usage = Usage::default();
        let data = r#"{"type":"some_future_event","payload":{}}"#;
        assert_eq!(parse_event(data, &mut usage).expect("event parses"), None);
    }

    #[test]
    fn malformed_json_is_reported_as_a_decode_error() {
        let mut usage = Usage::default();
        assert!(parse_event("{not json", &mut usage).is_err());
    }

    #[test]
    fn max_tokens_is_surfaced_so_truncation_is_visible() {
        let mut usage = Usage::default();
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"max_tokens"},
            "usage":{"output_tokens":4096}}"#;

        match parse_event(data, &mut usage).expect("event parses") {
            Some(Delta::Completed { stop_reason, .. }) => {
                assert_eq!(stop_reason, StopReason::MaxTokens);
            }
            other => panic!("expected a completed delta, got {other:?}"),
        }
    }

    #[test]
    fn the_system_prompt_is_sent_as_a_cacheable_block() {
        let request = Request::new(Some("be terse".to_owned()), vec![Message::user("hi")]);
        let body = build_body("claude-sonnet-4-5", &request);

        assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
        assert_eq!(body["system"][0]["text"], "be terse");
    }

    #[test]
    fn system_messages_are_not_duplicated_into_the_message_array() {
        let request = Request::new(
            Some("be terse".to_owned()),
            vec![Message::system("ignored"), Message::user("hi")],
        );
        let body = build_body("claude-sonnet-4-5", &request);

        let messages = body["messages"].as_array().expect("messages is an array");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }
}
