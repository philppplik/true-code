//! Anthropic Messages API.
//!
//! Streaming shape (the parts true-code consumes):
//!
//! ```text
//! message_start        → model name, input/cache token counts
//! content_block_start  → begins a text or tool_use block
//! content_block_delta  → text_delta, or input_json_delta for tool arguments
//! content_block_stop   → a tool_use block is now complete
//! message_delta        → { delta: { stop_reason }, usage: { output_tokens } }
//! ```
//!
//! Everything else (ping, thinking blocks, …) is ignored rather than treated as
//! an error: providers add event types over time, and an unknown event must not
//! kill a running turn.
//!
//! Tool arguments arrive as *partial JSON* spread across many deltas. They are
//! buffered per block index and parsed once the block closes, so no consumer
//! ever sees half an object.

use std::collections::HashMap;

use eventsource_stream::Eventsource as _;
use futures::StreamExt;
use serde::Deserialize;
use tc_config::ModelInfo;
use tc_core::{Delta, Price, Role, StopReason, ToolCall, Usage};

use crate::{Content, DeltaStream, Provider, ProviderError, Request, truncate_body};

/// Name used in error messages.
const PROVIDER: &str = "anthropic";

/// API version header value required by the Messages API.
const API_VERSION: &str = "2023-06-01";

/// Client for Anthropic's Messages API.
#[derive(Debug)]
pub struct Anthropic {
    info: ModelInfo,
    api_key: String,
    http: reqwest::Client,
}

impl Anthropic {
    /// Creates a client for the given catalogue entry.
    #[must_use]
    pub fn new(info: ModelInfo, api_key: String) -> Self {
        Self { info, api_key, http: reqwest::Client::new() }
    }
}

#[async_trait::async_trait]
impl Provider for Anthropic {
    fn id(&self) -> &str {
        &self.info.id
    }

    fn context_window(&self) -> u32 {
        self.info.context_window
    }

    fn price(&self) -> Price {
        self.info.price_or_free()
    }

    async fn stream(&self, request: Request) -> Result<DeltaStream, ProviderError> {
        let body = build_body(&self.info.api_model, &request);

        let response = self
            .http
            .post(format!("{}/v1/messages", self.info.vendor.base_url))
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

        let mut state = StreamState::default();

        let stream = response
            .bytes_stream()
            .eventsource()
            .filter_map(move |event| {
                let mapped = match event {
                    Err(err) => Some(Err(ProviderError::Decode {
                        provider: PROVIDER,
                        detail: err.to_string(),
                    })),
                    Ok(event) => match parse_event(&event.data, &mut state) {
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
                "content": message.content.iter().map(content_block).collect::<Vec<_>>(),
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

    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema,
                })
            })
            .collect();
    }
    body
}

/// Renders one content block in Anthropic's wire format.
fn content_block(content: &Content) -> serde_json::Value {
    match content {
        Content::Text { text } => serde_json::json!({ "type": "text", "text": text }),
        Content::ToolCall(call) => serde_json::json!({
            "type": "tool_use",
            "id": call.id,
            "name": call.name,
            "input": call.input,
        }),
        Content::ToolResult(result) => serde_json::json!({
            "type": "tool_result",
            "tool_use_id": result.call_id,
            "content": result.output,
            "is_error": result.is_error,
        }),
    }
}

/// Mutable state carried across the events of one stream.
#[derive(Debug, Default)]
struct StreamState {
    /// Usage arrives in two halves: input counts up front, output counts at the end.
    usage: Usage,
    /// Tool blocks still being assembled, keyed by content-block index.
    pending_tools: HashMap<u32, PartialTool>,
}

/// A tool call whose arguments are still streaming in.
#[derive(Debug)]
struct PartialTool {
    id: String,
    name: String,
    /// Concatenated `partial_json` fragments.
    arguments: String,
}

/// The subset of the Anthropic stream we act on.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    MessageStart {
        message: MessageStart,
    },
    ContentBlockStart {
        index: u32,
        content_block: BlockStart,
    },
    ContentBlockDelta {
        index: u32,
        delta: BlockDelta,
    },
    ContentBlockStop {
        index: u32,
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
enum BlockStart {
    ToolUse {
        id: String,
        name: String,
    },
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum BlockDelta {
    TextDelta {
        text: String,
    },
    InputJsonDelta {
        partial_json: String,
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

/// Maps one SSE payload to a [`Delta`], accumulating state as it goes.
///
/// Returns `Ok(None)` for events that carry no information for the consumer.
fn parse_event(data: &str, state: &mut StreamState) -> Result<Option<Delta>, String> {
    let event: Event = serde_json::from_str(data).map_err(|err| err.to_string())?;

    Ok(match event {
        Event::MessageStart { message } => {
            state.usage.input_tokens = message.usage.input_tokens;
            state.usage.cache_read_tokens = message.usage.cache_read_input_tokens;
            state.usage.cache_write_tokens = message.usage.cache_creation_input_tokens;
            Some(Delta::Started { model: message.model })
        }

        Event::ContentBlockStart { index, content_block: BlockStart::ToolUse { id, name } } => {
            state.pending_tools.insert(index, PartialTool { id, name, arguments: String::new() });
            None
        }

        Event::ContentBlockDelta { delta: BlockDelta::TextDelta { text }, .. } => {
            Some(Delta::Text { text })
        }

        Event::ContentBlockDelta { index, delta: BlockDelta::InputJsonDelta { partial_json } } => {
            if let Some(pending) = state.pending_tools.get_mut(&index) {
                pending.arguments.push_str(&partial_json);
            }
            None
        }

        Event::ContentBlockStop { index } => match state.pending_tools.remove(&index) {
            Some(pending) => Some(Delta::ToolCall(finish_tool(pending)?)),
            None => None,
        },

        Event::MessageDelta { delta, usage } => {
            state.usage.output_tokens = usage.output_tokens;
            Some(Delta::Completed {
                stop_reason: map_stop_reason(delta.stop_reason.as_deref()),
                usage: state.usage,
            })
        }

        Event::ContentBlockStart { .. } | Event::ContentBlockDelta { .. } | Event::Ignored => None,
    })
}

/// Parses a completed tool block's buffered arguments.
fn finish_tool(pending: PartialTool) -> Result<ToolCall, String> {
    // A tool called with no arguments streams no input_json_delta at all, so an
    // empty buffer means `{}` rather than malformed JSON.
    let raw = if pending.arguments.trim().is_empty() { "{}" } else { &pending.arguments };

    let input: serde_json::Value = serde_json::from_str(raw).map_err(|err| {
        format!("tool `{}` sent arguments that are not valid JSON: {err}", pending.name)
    })?;

    Ok(ToolCall { id: pending.id, name: pending.name, input })
}

/// Normalises Anthropic's stop reasons.
fn map_stop_reason(raw: Option<&str>) -> StopReason {
    match raw {
        Some("end_turn") => StopReason::EndTurn,
        Some("tool_use") => StopReason::ToolUse,
        Some("max_tokens") => StopReason::MaxTokens,
        Some("stop_sequence") => StopReason::StopSequence,
        _ => StopReason::Other(tc_core::message::OtherReason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolSchema;
    use tc_core::{Message, ToolResult};

    #[test]
    fn message_start_reports_the_model_and_seeds_input_usage() {
        let mut state = StreamState::default();
        let data = r#"{"type":"message_start","message":{"model":"claude-sonnet-4-5",
            "usage":{"input_tokens":120,"cache_read_input_tokens":900}}}"#;

        let delta = parse_event(data, &mut state).expect("event parses");

        assert_eq!(delta, Some(Delta::Started { model: "claude-sonnet-4-5".to_owned() }));
        assert_eq!(state.usage.input_tokens, 120);
        assert_eq!(state.usage.cache_read_tokens, 900);
    }

    #[test]
    fn text_deltas_become_text_increments() {
        let mut state = StreamState::default();
        let data = r#"{"type":"content_block_delta","index":0,
            "delta":{"type":"text_delta","text":"Hallo"}}"#;

        let delta = parse_event(data, &mut state).expect("event parses");

        assert_eq!(delta, Some(Delta::Text { text: "Hallo".to_owned() }));
    }

    #[test]
    fn a_tool_call_is_emitted_only_once_its_arguments_are_complete() {
        let mut state = StreamState::default();

        let start = r#"{"type":"content_block_start","index":1,
            "content_block":{"type":"tool_use","id":"toolu_1","name":"read_file","input":{}}}"#;
        assert_eq!(parse_event(start, &mut state).expect("event parses"), None);

        // Arguments arrive split across deltas, mid-token.
        for fragment in [r#"{"pa"#, r#"th":"sr"#, r#"c/main.rs"}"#] {
            let data = format!(
                r#"{{"type":"content_block_delta","index":1,
                   "delta":{{"type":"input_json_delta","partial_json":{}}}}}"#,
                serde_json::to_string(fragment).unwrap()
            );
            assert_eq!(
                parse_event(&data, &mut state).expect("event parses"),
                None,
                "a partial argument must never be emitted"
            );
        }

        let stop = r#"{"type":"content_block_stop","index":1}"#;
        match parse_event(stop, &mut state).expect("event parses") {
            Some(Delta::ToolCall(call)) => {
                assert_eq!(call.id, "toolu_1");
                assert_eq!(call.name, "read_file");
                assert_eq!(call.input["path"], "src/main.rs");
            }
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_called_without_arguments_yields_an_empty_object() {
        let mut state = StreamState::default();
        let start = r#"{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"toolu_2","name":"list_dir","input":{}}}"#;
        parse_event(start, &mut state).expect("event parses");

        match parse_event(r#"{"type":"content_block_stop","index":0}"#, &mut state)
            .expect("event parses")
        {
            Some(Delta::ToolCall(call)) => assert_eq!(call.input, serde_json::json!({})),
            other => panic!("expected a tool call, got {other:?}"),
        }
    }

    #[test]
    fn two_tool_calls_in_one_turn_do_not_mix_their_arguments() {
        let mut state = StreamState::default();

        for (index, id, name) in [(1, "toolu_a", "read_file"), (2, "toolu_b", "grep")] {
            let data = format!(
                r#"{{"type":"content_block_start","index":{index},
                   "content_block":{{"type":"tool_use","id":"{id}","name":"{name}"}}}}"#
            );
            parse_event(&data, &mut state).expect("event parses");
        }

        // Interleaved, which is what the API actually does.
        for (index, fragment) in [(1, r#"{"path":"a.rs"}"#), (2, r#"{"pattern":"fn"}"#)] {
            let data = format!(
                r#"{{"type":"content_block_delta","index":{index},
                   "delta":{{"type":"input_json_delta","partial_json":{}}}}}"#,
                serde_json::to_string(fragment).unwrap()
            );
            parse_event(&data, &mut state).expect("event parses");
        }

        let first = parse_event(r#"{"type":"content_block_stop","index":1}"#, &mut state).unwrap();
        let second = parse_event(r#"{"type":"content_block_stop","index":2}"#, &mut state).unwrap();

        match (first, second) {
            (Some(Delta::ToolCall(a)), Some(Delta::ToolCall(b))) => {
                assert_eq!(a.input["path"], "a.rs");
                assert_eq!(b.input["pattern"], "fn");
            }
            other => panic!("expected two tool calls, got {other:?}"),
        }
    }

    #[test]
    fn malformed_tool_arguments_are_reported_rather_than_silently_dropped() {
        let mut state = StreamState::default();
        let start = r#"{"type":"content_block_start","index":0,
            "content_block":{"type":"tool_use","id":"t","name":"read_file"}}"#;
        parse_event(start, &mut state).expect("event parses");

        let data = r#"{"type":"content_block_delta","index":0,
            "delta":{"type":"input_json_delta","partial_json":"{not json"}}"#;
        parse_event(data, &mut state).expect("event parses");

        let error = parse_event(r#"{"type":"content_block_stop","index":0}"#, &mut state)
            .expect_err("the arguments are malformed");
        assert!(error.contains("read_file"), "the message must name the tool: {error}");
    }

    #[test]
    fn stopping_a_text_block_produces_nothing() {
        let mut state = StreamState::default();
        let stop = r#"{"type":"content_block_stop","index":0}"#;
        assert_eq!(parse_event(stop, &mut state).expect("event parses"), None);
    }

    #[test]
    fn message_delta_completes_the_turn_with_the_full_usage() {
        let mut state = StreamState {
            usage: Usage { input_tokens: 120, ..Usage::default() },
            ..StreamState::default()
        };
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},
            "usage":{"output_tokens":42}}"#;

        match parse_event(data, &mut state).expect("event parses") {
            Some(Delta::Completed { stop_reason, usage }) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 120);
                assert_eq!(usage.output_tokens, 42);
            }
            other => panic!("expected a completed delta, got {other:?}"),
        }
    }

    #[test]
    fn tool_use_is_a_distinct_stop_reason_from_end_turn() {
        let mut state = StreamState::default();
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},
            "usage":{"output_tokens":10}}"#;

        match parse_event(data, &mut state).expect("event parses") {
            Some(Delta::Completed { stop_reason, .. }) => {
                assert_eq!(stop_reason, StopReason::ToolUse);
            }
            other => panic!("expected a completed delta, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_types_are_ignored_not_fatal() {
        let mut state = StreamState::default();
        let data = r#"{"type":"some_future_event","payload":{}}"#;
        assert_eq!(parse_event(data, &mut state).expect("event parses"), None);
    }

    #[test]
    fn malformed_json_is_reported_as_a_decode_error() {
        let mut state = StreamState::default();
        assert!(parse_event("{not json", &mut state).is_err());
    }

    #[test]
    fn max_tokens_is_surfaced_so_truncation_is_visible() {
        let mut state = StreamState::default();
        let data = r#"{"type":"message_delta","delta":{"stop_reason":"max_tokens"},
            "usage":{"output_tokens":4096}}"#;

        match parse_event(data, &mut state).expect("event parses") {
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

    #[test]
    fn no_tools_key_is_sent_when_no_tools_are_available() {
        let request = Request::new(None, vec![Message::user("hi")]);
        let body = build_body("claude-sonnet-4-5", &request);
        assert!(body.get("tools").is_none(), "an empty tools array confuses some models");
    }

    #[test]
    fn tool_schemas_are_sent_in_the_expected_shape() {
        let mut request = Request::new(None, vec![Message::user("hi")]);
        request.tools = vec![ToolSchema {
            name: "read_file".to_owned(),
            description: "Read a file".to_owned(),
            input_schema: serde_json::json!({ "type": "object" }),
        }];

        let body = build_body("claude-sonnet-4-5", &request);

        assert_eq!(body["tools"][0]["name"], "read_file");
        assert_eq!(body["tools"][0]["input_schema"]["type"], "object");
    }

    #[test]
    fn tool_results_are_sent_back_with_their_call_id() {
        let request = Request::new(
            None,
            vec![Message::tool_results(vec![ToolResult {
                call_id: "toolu_1".to_owned(),
                output: "fn main() {}".to_owned(),
                is_error: false,
            }])],
        );
        let body = build_body("claude-sonnet-4-5", &request);

        let block = &body["messages"][0]["content"][0];
        assert_eq!(block["type"], "tool_result");
        assert_eq!(block["tool_use_id"], "toolu_1");
        assert_eq!(block["is_error"], false);
    }
}
