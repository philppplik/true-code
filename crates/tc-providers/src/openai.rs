//! OpenAI Chat Completions API.
//!
//! Three differences from Anthropic that matter for a harness:
//!
//! * Usage is **not** streamed unless `stream_options.include_usage` is set. Without
//!   it the cost display silently reads zero — which is worse than no display at all.
//! * The stream is terminated by a literal `data: [DONE]` sentinel that is not JSON.
//! * Tool calls are identified by an **array index**, not by a block id, and the
//!   id and name arrive only in the first fragment. Arguments are a JSON *string*,
//!   not an object.

use std::collections::HashMap;

use eventsource_stream::Eventsource as _;
use futures::StreamExt;
use serde::Deserialize;
use tc_config::ModelInfo;
use tc_core::{Delta, Price, Role, StopReason, ToolCall, Usage};

use crate::{Content, DeltaStream, Provider, ProviderError, Request, truncate_body};

/// Name used in error messages.
const PROVIDER: &str = "openai";

/// Sentinel that ends an OpenAI stream.
const DONE: &str = "[DONE]";

/// Client for OpenAI's Chat Completions API.
#[derive(Debug)]
pub struct OpenAi {
    info: &'static ModelInfo,
    api_key: String,
    http: reqwest::Client,
}

impl OpenAi {
    /// Creates a client for the given catalogue entry.
    #[must_use]
    pub fn new(info: &'static ModelInfo, api_key: String) -> Self {
        Self { info, api_key, http: reqwest::Client::new() }
    }
}

#[async_trait::async_trait]
impl Provider for OpenAi {
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
            .post(format!("{}/v1/chat/completions", self.info.base_url))
            .bearer_auth(&self.api_key)
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
            .flat_map(move |event| {
                let mapped: Vec<Result<Delta, ProviderError>> = match event {
                    Err(err) => vec![Err(ProviderError::Decode {
                        provider: PROVIDER,
                        detail: err.to_string(),
                    })],
                    Ok(event) => match parse_event(&event.data, &mut state) {
                        Ok(deltas) => deltas.into_iter().map(Ok).collect(),
                        Err(detail) => {
                            vec![Err(ProviderError::Decode { provider: PROVIDER, detail })]
                        }
                    },
                };
                futures::stream::iter(mapped)
            })
            .boxed();

        Ok(stream)
    }
}

/// Builds the JSON request body.
fn build_body(api_model: &str, request: &Request) -> serde_json::Value {
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(request.messages.len() + 1);

    if let Some(system) = &request.system {
        messages.push(serde_json::json!({ "role": "system", "content": system }));
    }
    for message in &request.messages {
        messages.extend(render_message(message));
    }

    let mut body = serde_json::json!({
        "model": api_model,
        "max_completion_tokens": request.max_tokens,
        "stream": true,
        // Without this, no usage is streamed and the cost display reads zero.
        "stream_options": { "include_usage": true },
        "messages": messages,
    });

    if !request.tools.is_empty() {
        body["tools"] = request
            .tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": tool.name,
                        "description": tool.description,
                        "parameters": tool.input_schema,
                    },
                })
            })
            .collect();
    }
    body
}

/// Renders one message, which may expand into several wire messages.
///
/// Tool results are their own top-level message with `role: "tool"` here, whereas
/// Anthropic nests them inside a user message — so one domain message can become
/// several.
fn render_message(message: &tc_core::Message) -> Vec<serde_json::Value> {
    let mut rendered = Vec::new();
    let mut text = String::new();
    let mut tool_calls = Vec::new();

    for block in &message.content {
        match block {
            Content::Text { text: chunk } => text.push_str(chunk),
            Content::ToolCall(call) => tool_calls.push(serde_json::json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    // Arguments go out as a JSON *string*, matching the wire format.
                    "arguments": call.input.to_string(),
                },
            })),
            Content::ToolResult(result) => rendered.push(serde_json::json!({
                "role": "tool",
                "tool_call_id": result.call_id,
                "content": result.output,
            })),
        }
    }

    if !text.is_empty() || !tool_calls.is_empty() {
        let role = match message.role {
            Role::Assistant => "assistant",
            Role::System => "system",
            Role::User => "user",
        };
        let mut wire = serde_json::json!({ "role": role, "content": text });
        if !tool_calls.is_empty() {
            wire["tool_calls"] = serde_json::Value::Array(tool_calls);
        }
        // Tool results must precede the assistant text they belong to only when
        // they were produced first; within a single message the order is stable.
        rendered.insert(0, wire);
    }
    rendered
}

/// Mutable state carried across the chunks of one stream.
#[derive(Debug, Default)]
struct StreamState {
    announced_model: bool,
    /// Tool calls still being assembled, keyed by their position in the array.
    pending_tools: HashMap<u32, PartialTool>,
}

/// A tool call whose arguments are still streaming in.
#[derive(Debug, Default)]
struct PartialTool {
    id: String,
    name: String,
    arguments: String,
}

/// One `chat.completion.chunk`.
#[derive(Debug, Deserialize)]
struct Chunk {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Option<ChunkUsage>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    #[serde(default)]
    delta: ChoiceDelta,
    #[serde(default)]
    finish_reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ChoiceDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCallDelta>,
}

#[derive(Debug, Deserialize)]
struct ToolCallDelta {
    #[serde(default)]
    index: u32,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<FunctionDelta>,
}

#[derive(Debug, Deserialize)]
struct FunctionDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChunkUsage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
    #[serde(default)]
    prompt_tokens_details: Option<PromptDetails>,
}

#[derive(Debug, Deserialize)]
struct PromptDetails {
    #[serde(default)]
    cached_tokens: u32,
}

/// Maps one SSE payload to zero or more deltas.
///
/// Returns several because a final chunk can close pending tool calls *and*
/// complete the turn.
fn parse_event(data: &str, state: &mut StreamState) -> Result<Vec<Delta>, String> {
    if data.trim() == DONE {
        return Ok(Vec::new());
    }

    let chunk: Chunk = serde_json::from_str(data).map_err(|err| err.to_string())?;
    let mut deltas = Vec::new();

    if !state.announced_model && !chunk.model.is_empty() {
        state.announced_model = true;
        deltas.push(Delta::Started { model: chunk.model });
    }

    for choice in &chunk.choices {
        for fragment in &choice.delta.tool_calls {
            let pending = state.pending_tools.entry(fragment.index).or_default();
            if let Some(id) = &fragment.id {
                pending.id.clone_from(id);
            }
            if let Some(function) = &fragment.function {
                if let Some(name) = &function.name {
                    pending.name.clone_from(name);
                }
                if let Some(arguments) = &function.arguments {
                    pending.arguments.push_str(arguments);
                }
            }
        }

        if let Some(text) = &choice.delta.content
            && !text.is_empty()
        {
            deltas.push(Delta::Text { text: text.clone() });
        }

        // `finish_reason` is the only signal that tool arguments are complete;
        // there is no per-call stop event.
        if choice.finish_reason.is_some() {
            deltas.extend(drain_tools(state)?);
        }
    }

    if let Some(usage) = chunk.usage {
        deltas.extend(drain_tools(state)?);

        let cache_read_tokens =
            usage.prompt_tokens_details.map_or(0, |details| details.cached_tokens);
        deltas.push(Delta::Completed {
            stop_reason: map_stop_reason(
                chunk.choices.first().and_then(|choice| choice.finish_reason.as_deref()),
            ),
            usage: Usage {
                // `prompt_tokens` includes cached tokens; splitting them keeps the
                // cache-hit rate honest instead of double-counting.
                input_tokens: usage.prompt_tokens.saturating_sub(cache_read_tokens),
                output_tokens: usage.completion_tokens,
                cache_read_tokens,
                cache_write_tokens: 0,
            },
        });
    }

    Ok(deltas)
}

/// Emits every buffered tool call, in array order.
fn drain_tools(state: &mut StreamState) -> Result<Vec<Delta>, String> {
    let mut indices: Vec<u32> = state.pending_tools.keys().copied().collect();
    indices.sort_unstable();

    let mut deltas = Vec::with_capacity(indices.len());
    for index in indices {
        let Some(pending) = state.pending_tools.remove(&index) else {
            continue;
        };
        let raw = if pending.arguments.trim().is_empty() { "{}" } else { &pending.arguments };
        let input: serde_json::Value = serde_json::from_str(raw).map_err(|err| {
            format!("tool `{}` sent arguments that are not valid JSON: {err}", pending.name)
        })?;
        deltas.push(Delta::ToolCall(ToolCall { id: pending.id, name: pending.name, input }));
    }
    Ok(deltas)
}

/// Normalises OpenAI's finish reasons.
fn map_stop_reason(raw: Option<&str>) -> StopReason {
    match raw {
        Some("stop") => StopReason::EndTurn,
        Some("tool_calls") => StopReason::ToolUse,
        Some("length") => StopReason::MaxTokens,
        _ => StopReason::Other(tc_core::message::OtherReason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolSchema;
    use tc_core::{Message, ToolResult};

    #[test]
    fn the_first_chunk_announces_the_model_once() {
        let mut state = StreamState::default();
        let data = r#"{"model":"gpt-4.1-mini","choices":[{"delta":{"content":"a"}}]}"#;

        let first = parse_event(data, &mut state).expect("chunk parses");
        assert_eq!(
            first,
            vec![
                Delta::Started { model: "gpt-4.1-mini".to_owned() },
                Delta::Text { text: "a".to_owned() }
            ]
        );

        let second = parse_event(data, &mut state).expect("chunk parses");
        assert_eq!(second, vec![Delta::Text { text: "a".to_owned() }]);
    }

    #[test]
    fn the_done_sentinel_is_not_treated_as_json() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };
        assert!(parse_event("[DONE]", &mut state).expect("sentinel is handled").is_empty());
    }

    #[test]
    fn a_tool_call_is_assembled_across_chunks_and_emitted_on_finish() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };

        let opening = r#"{"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"call_1","function":{"name":"read_file","arguments":""}}]}}]}"#;
        assert!(parse_event(opening, &mut state).expect("chunk parses").is_empty());

        for fragment in [r#"{\"pa"#, r#"th\":\"a.rs\"}"#] {
            let data = format!(
                r#"{{"choices":[{{"delta":{{"tool_calls":[
                   {{"index":0,"function":{{"arguments":"{fragment}"}}}}]}}}}]}}"#
            );
            assert!(
                parse_event(&data, &mut state).expect("chunk parses").is_empty(),
                "a partial argument must never be emitted"
            );
        }

        let finish = r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        match parse_event(finish, &mut state).expect("chunk parses").as_slice() {
            [Delta::ToolCall(call)] => {
                assert_eq!(call.id, "call_1");
                assert_eq!(call.name, "read_file");
                assert_eq!(call.input["path"], "a.rs");
            }
            other => panic!("expected one tool call, got {other:?}"),
        }
    }

    #[test]
    fn parallel_tool_calls_are_emitted_in_array_order() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };

        let opening = r#"{"choices":[{"delta":{"tool_calls":[
            {"index":1,"id":"call_b","function":{"name":"grep","arguments":"{}"}},
            {"index":0,"id":"call_a","function":{"name":"list_dir","arguments":"{}"}}]}}]}"#;
        parse_event(opening, &mut state).expect("chunk parses");

        let finish = r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#;
        match parse_event(finish, &mut state).expect("chunk parses").as_slice() {
            [Delta::ToolCall(first), Delta::ToolCall(second)] => {
                assert_eq!(first.name, "list_dir", "index 0 must come first");
                assert_eq!(second.name, "grep");
            }
            other => panic!("expected two tool calls, got {other:?}"),
        }
    }

    #[test]
    fn a_tool_call_is_not_emitted_twice_when_usage_follows_the_finish_chunk() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };

        let opening = r#"{"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"call_1","function":{"name":"list_dir","arguments":"{}"}}]}}]}"#;
        parse_event(opening, &mut state).expect("chunk parses");
        parse_event(r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#, &mut state)
            .expect("chunk parses");

        let usage = r#"{"choices":[],"usage":{"prompt_tokens":10,"completion_tokens":2}}"#;
        let deltas = parse_event(usage, &mut state).expect("chunk parses");

        assert_eq!(deltas.len(), 1, "only the completion delta remains: {deltas:?}");
        assert!(matches!(deltas[0], Delta::Completed { .. }));
    }

    #[test]
    fn cached_tokens_are_split_out_of_the_prompt_total() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };
        let data = r#"{"choices":[{"finish_reason":"stop","delta":{}}],
            "usage":{"prompt_tokens":1000,"completion_tokens":50,
            "prompt_tokens_details":{"cached_tokens":800}}}"#;

        match parse_event(data, &mut state).expect("chunk parses").as_slice() {
            [Delta::Completed { usage, stop_reason }] => {
                assert_eq!(*stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 200, "cached tokens must not be counted twice");
                assert_eq!(usage.cache_read_tokens, 800);
                assert_eq!(usage.output_tokens, 50);
            }
            other => panic!("expected a completed delta, got {other:?}"),
        }
    }

    #[test]
    fn tool_calls_finish_reason_maps_to_tool_use() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };
        let data = r#"{"choices":[{"finish_reason":"tool_calls","delta":{}}],
            "usage":{"prompt_tokens":10,"completion_tokens":5}}"#;

        match parse_event(data, &mut state).expect("chunk parses").as_slice() {
            [Delta::Completed { stop_reason, .. }] => {
                assert_eq!(*stop_reason, StopReason::ToolUse);
            }
            other => panic!("expected a completed delta, got {other:?}"),
        }
    }

    #[test]
    fn empty_content_deltas_produce_no_output() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };
        let data = r#"{"choices":[{"delta":{"content":""}}]}"#;
        assert!(parse_event(data, &mut state).expect("chunk parses").is_empty());
    }

    #[test]
    fn malformed_tool_arguments_are_reported_rather_than_silently_dropped() {
        let mut state = StreamState { announced_model: true, ..StreamState::default() };
        let opening = r#"{"choices":[{"delta":{"tool_calls":[
            {"index":0,"id":"c","function":{"name":"grep","arguments":"{not json"}}]}}]}"#;
        parse_event(opening, &mut state).expect("chunk parses");

        let error =
            parse_event(r#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#, &mut state)
                .expect_err("the arguments are malformed");
        assert!(error.contains("grep"), "the message must name the tool: {error}");
    }

    #[test]
    fn usage_streaming_is_requested_explicitly() {
        let request = Request::new(None, vec![Message::user("hi")]);
        let body = build_body("gpt-4.1-mini", &request);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn the_system_prompt_is_sent_as_the_first_message() {
        let request = Request::new(Some("be terse".to_owned()), vec![Message::user("hi")]);
        let body = build_body("gpt-4.1-mini", &request);

        let messages = body["messages"].as_array().expect("messages is an array");
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "be terse");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn tools_are_wrapped_in_the_function_envelope() {
        let request = Request::new(None, vec![Message::user("hi")]).with_tools(vec![ToolSchema {
            name: "read_file".to_owned(),
            description: "Read a file".to_owned(),
            input_schema: serde_json::json!({ "type": "object" }),
        }]);

        let body = build_body("gpt-4.1-mini", &request);

        assert_eq!(body["tools"][0]["type"], "function");
        assert_eq!(body["tools"][0]["function"]["name"], "read_file");
        assert_eq!(body["tools"][0]["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn tool_results_become_their_own_tool_role_message() {
        let request = Request::new(
            None,
            vec![Message::tool_results(vec![ToolResult {
                call_id: "call_1".to_owned(),
                output: "fn main() {}".to_owned(),
                is_error: false,
            }])],
        );
        let body = build_body("gpt-4.1-mini", &request);

        assert_eq!(body["messages"][0]["role"], "tool");
        assert_eq!(body["messages"][0]["tool_call_id"], "call_1");
    }

    #[test]
    fn assistant_tool_calls_are_sent_with_stringified_arguments() {
        let message = tc_core::Message {
            role: Role::Assistant,
            content: vec![Content::ToolCall(ToolCall {
                id: "call_1".to_owned(),
                name: "read_file".to_owned(),
                input: serde_json::json!({ "path": "a.rs" }),
            })],
        };
        let request = Request::new(None, vec![message]);
        let body = build_body("gpt-4.1-mini", &request);

        let arguments = body["messages"][0]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .expect("arguments are a JSON string, not an object");
        assert!(arguments.contains("\"path\""), "unexpected arguments: {arguments}");
    }
}
