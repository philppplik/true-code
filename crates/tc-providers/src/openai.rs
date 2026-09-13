//! OpenAI Chat Completions API.
//!
//! Two differences from Anthropic that matter for a harness:
//!
//! * Usage is **not** streamed unless `stream_options.include_usage` is set. Without
//!   it the cost display silently reads zero — which is worse than no display at all.
//! * The stream is terminated by a literal `data: [DONE]` sentinel that is not JSON.

use eventsource_stream::Eventsource as _;
use futures::StreamExt;
use serde::Deserialize;
use tc_config::ModelInfo;
use tc_core::{Delta, Price, Role, StopReason, Usage};

use crate::{DeltaStream, Provider, ProviderError, Request, truncate_body};

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

        let mut announced_model = false;

        let stream = response
            .bytes_stream()
            .eventsource()
            .filter_map(move |event| {
                let mapped = match event {
                    Err(err) => Some(Err(ProviderError::Decode {
                        provider: PROVIDER,
                        detail: err.to_string(),
                    })),
                    Ok(event) => match parse_event(&event.data, &mut announced_model) {
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
    let mut messages: Vec<serde_json::Value> = Vec::with_capacity(request.messages.len() + 1);

    if let Some(system) = &request.system {
        messages.push(serde_json::json!({ "role": "system", "content": system }));
    }
    for message in &request.messages {
        messages.push(serde_json::json!({
            "role": match message.role {
                Role::Assistant => "assistant",
                Role::System => "system",
                Role::User => "user",
            },
            "content": message.content,
        }));
    }

    serde_json::json!({
        "model": api_model,
        "max_completion_tokens": request.max_tokens,
        "stream": true,
        // Without this, no usage is streamed and the cost display reads zero.
        "stream_options": { "include_usage": true },
        "messages": messages,
    })
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

/// Maps one SSE payload to a [`Delta`].
///
/// `announced_model` guards the one-shot [`Delta::Started`]: OpenAI repeats the
/// model name on every chunk, and the UI must not restart its header each time.
fn parse_event(data: &str, announced_model: &mut bool) -> Result<Option<Delta>, String> {
    if data.trim() == DONE {
        return Ok(None);
    }

    let chunk: Chunk = serde_json::from_str(data).map_err(|err| err.to_string())?;

    if !*announced_model && !chunk.model.is_empty() {
        *announced_model = true;
        return Ok(Some(Delta::Started { model: chunk.model }));
    }

    // The final chunk carries usage and no choices.
    if let Some(usage) = chunk.usage {
        let cache_read_tokens =
            usage.prompt_tokens_details.map_or(0, |details| details.cached_tokens);
        return Ok(Some(Delta::Completed {
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
        }));
    }

    let text = chunk
        .choices
        .into_iter()
        .find_map(|choice| choice.delta.content)
        .filter(|text| !text.is_empty());

    Ok(text.map(|text| Delta::Text { text }))
}

/// Normalises OpenAI's finish reasons.
fn map_stop_reason(raw: Option<&str>) -> StopReason {
    match raw {
        Some("stop" | "tool_calls") => StopReason::EndTurn,
        Some("length") => StopReason::MaxTokens,
        _ => StopReason::Other(tc_core::message::OtherReason),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tc_core::Message;

    #[test]
    fn the_first_chunk_announces_the_model_once() {
        let mut announced = false;
        let data = r#"{"model":"gpt-4.1-mini","choices":[{"delta":{"content":"a"}}]}"#;

        let first = parse_event(data, &mut announced).expect("chunk parses");
        assert_eq!(first, Some(Delta::Started { model: "gpt-4.1-mini".to_owned() }));

        let second = parse_event(data, &mut announced).expect("chunk parses");
        assert_eq!(second, Some(Delta::Text { text: "a".to_owned() }));
    }

    #[test]
    fn the_done_sentinel_is_not_treated_as_json() {
        let mut announced = true;
        assert_eq!(parse_event("[DONE]", &mut announced).expect("sentinel is handled"), None);
    }

    #[test]
    fn cached_tokens_are_split_out_of_the_prompt_total() {
        let mut announced = true;
        let data = r#"{"choices":[{"finish_reason":"stop","delta":{}}],
            "usage":{"prompt_tokens":1000,"completion_tokens":50,
            "prompt_tokens_details":{"cached_tokens":800}}}"#;

        match parse_event(data, &mut announced).expect("chunk parses") {
            Some(Delta::Completed { usage, stop_reason }) => {
                assert_eq!(stop_reason, StopReason::EndTurn);
                assert_eq!(usage.input_tokens, 200, "cached tokens must not be counted twice");
                assert_eq!(usage.cache_read_tokens, 800);
                assert_eq!(usage.output_tokens, 50);
            }
            other => panic!("expected a completed delta, got {other:?}"),
        }
    }

    #[test]
    fn empty_content_deltas_produce_no_output() {
        let mut announced = true;
        let data = r#"{"choices":[{"delta":{"content":""}}]}"#;
        assert_eq!(parse_event(data, &mut announced).expect("chunk parses"), None);
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
}
